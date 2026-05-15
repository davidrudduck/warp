use super::{
    settings_page::{
        Category, MatchData, PageType, SettingsPageEvent, SettingsPageMeta, SettingsPageViewHandle,
        SettingsWidget,
    },
    SettingsSection,
};
use crate::appearance::Appearance;
use crate::editor::{EditorView, SingleLineEditorOptions, TextOptions};
use crate::ui_components::icons::Icon;
use crate::view_components::action_button::{ActionButton, NakedTheme};
use crate::view_components::{Dropdown, DropdownItem};
use ::ai::api_keys::ApiKeyManager;
use ::ai::model_registry::providers::custom::CustomListProvider;
use ::ai::model_registry::providers::genai_backed::GenaiBackedListProvider;
use ::ai::model_registry::providers::openrouter::OpenRouterListProvider;
use ::ai::model_registry::{
    ModelDescriptor, ModelListCache, ModelListError, ModelListProvider, ProviderId,
};
use ::ai::telemetry::AITelemetryEvent;
use ::ai::url_validation::{normalize_direct_api_base_url, normalize_openai_compatible_base_url};
use std::cell::{Cell, RefCell};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::Duration;
use warp_core::features::FeatureFlag;
use warp_core::send_telemetry_from_ctx;
use warp_core::ui::theme::color::internal_colors;
use warpui::{
    elements::{
        ConstrainedBox, Container, CornerRadius, Element, Fill, Flex, ParentElement, Radius,
    },
    ui_components::components::{Coords, UiComponent, UiComponentStyles},
    AppContext, Entity, ModelHandle, SingletonEntity, TypedActionView, View, ViewContext,
    ViewHandle,
};

/// Model list cache freshness threshold. Entries older than this are ignored
/// by `get()` and require an explicit refresh via "Update Model List".
const MODEL_CACHE_TTL: Duration = Duration::from_secs(86_400);

const ITEM_VERTICAL_SPACING: f32 = 24.;
const DROPDOWN_WIDTH: f32 = 225.;
const INPUT_MAX_WIDTH: f32 = 360.;
const INPUT_BORDER_RADIUS: f32 = 6.;
const INPUT_PADDING_VERTICAL: f32 = 10.;
const INPUT_PADDING_HORIZONTAL: f32 = 12.;

/// Tooltip text shown on the API Key visibility toggle button.
///
/// Pure function so the state-machine half of the toggle can be tested
/// without instantiating a view.
fn visibility_tooltip(show: bool) -> &'static str {
    if show {
        "Hide API key"
    } else {
        "Show API key"
    }
}

fn invalid_base_url_message() -> String {
    "Base URL must use https://, except http:// localhost or private LAN addresses".to_string()
}

fn normalized_base_url_for_provider(provider: &ProviderType, url: &str) -> Result<String, String> {
    match provider {
        ProviderType::Custom => normalize_openai_compatible_base_url(url),
        ProviderType::Ollama | ProviderType::OpenRouter => normalize_direct_api_base_url(url),
        ProviderType::OpenAI | ProviderType::Anthropic | ProviderType::GoogleGemini => {
            normalize_direct_api_base_url(url)
        }
    }
    .map_err(|_| invalid_base_url_message())
}

fn render_chromed_input(
    editor: ViewHandle<EditorView>,
    appearance: &Appearance,
) -> Box<dyn Element> {
    let theme = appearance.theme();
    let bg_fill = theme.surface_2();
    let bg_solid = bg_fill.into_solid();
    let text_color = internal_colors::text_main(theme, bg_solid);
    let border_fill = Fill::Solid(internal_colors::neutral_4(theme));

    let input = appearance
        .ui_builder()
        .text_input(editor)
        .with_style(UiComponentStyles {
            background: Some(bg_fill.into()),
            border_width: Some(1.),
            border_color: Some(border_fill),
            border_radius: Some(CornerRadius::with_all(Radius::Pixels(INPUT_BORDER_RADIUS))),
            font_color: Some(text_color),
            padding: Some(Coords {
                top: INPUT_PADDING_VERTICAL,
                bottom: INPUT_PADDING_VERTICAL,
                left: INPUT_PADDING_HORIZONTAL,
                right: INPUT_PADDING_HORIZONTAL,
            }),
            ..Default::default()
        })
        .build()
        .finish();

    ConstrainedBox::new(input)
        .with_max_width(INPUT_MAX_WIDTH)
        .finish()
}

#[derive(Debug, Clone, PartialEq)]
pub enum DirectApiPageAction {
    SelectProvider(String),
    TestConnection,
    SaveApiKey,
    UpdateModelList,
    ToggleApiKeyVisibility,
    SelectModel(String),
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum ProviderType {
    OpenAI,
    Anthropic,
    GoogleGemini,
    Ollama,
    OpenRouter,
    Custom,
}

impl ProviderType {
    pub(super) fn as_str(&self) -> &'static str {
        match self {
            ProviderType::OpenAI => "OpenAI",
            ProviderType::Anthropic => "Anthropic",
            ProviderType::GoogleGemini => "Google Gemini",
            ProviderType::Ollama => "Ollama",
            ProviderType::OpenRouter => "OpenRouter",
            ProviderType::Custom => "Custom (OpenAI-compatible)",
        }
    }

    pub(super) fn from_str(s: &str) -> Option<Self> {
        match s {
            "OpenAI" => Some(ProviderType::OpenAI),
            "Anthropic" => Some(ProviderType::Anthropic),
            "Google Gemini" => Some(ProviderType::GoogleGemini),
            "Ollama" => Some(ProviderType::Ollama),
            "OpenRouter" => Some(ProviderType::OpenRouter),
            "Custom (OpenAI-compatible)" => Some(ProviderType::Custom),
            _ => None,
        }
    }

    pub(super) fn all() -> Vec<Self> {
        vec![
            ProviderType::OpenAI,
            ProviderType::Anthropic,
            ProviderType::GoogleGemini,
            ProviderType::Ollama,
            ProviderType::OpenRouter,
            ProviderType::Custom,
        ]
    }

    pub(super) fn needs_base_url(&self) -> bool {
        matches!(
            self,
            ProviderType::Ollama | ProviderType::OpenRouter | ProviderType::Custom
        )
    }

    /// Map this UI-layer provider to the canonical `ProviderId` used by the
    /// model registry, cache, and persisted `ApiKeys::selected_models` map.
    pub(super) fn to_provider_id(&self) -> ProviderId {
        match self {
            ProviderType::OpenAI => ProviderId::OpenAI,
            ProviderType::Anthropic => ProviderId::Anthropic,
            ProviderType::GoogleGemini => ProviderId::GoogleGemini,
            ProviderType::Ollama => ProviderId::Ollama,
            ProviderType::OpenRouter => ProviderId::OpenRouter,
            ProviderType::Custom => ProviderId::Custom,
        }
    }

    pub(super) fn default_base_url(&self) -> &'static str {
        match self {
            ProviderType::Ollama => "http://localhost:11434",
            ProviderType::OpenRouter => "https://openrouter.ai/api/v1",
            ProviderType::Custom => "",
            ProviderType::OpenAI => "",
            ProviderType::Anthropic => "",
            ProviderType::GoogleGemini => "",
        }
    }

    pub(super) fn api_key_placeholder(&self) -> &'static str {
        match self {
            ProviderType::OpenAI => "sk-...",
            ProviderType::Anthropic => "sk-ant-...",
            ProviderType::GoogleGemini => "AIza...",
            ProviderType::Ollama => "Optional",
            ProviderType::OpenRouter => "sk-or-...",
            ProviderType::Custom => "Optional",
        }
    }

    pub(super) fn base_url_placeholder(&self) -> &'static str {
        match self {
            ProviderType::OpenAI => "",
            ProviderType::Anthropic => "",
            ProviderType::GoogleGemini => "",
            ProviderType::Ollama => "http://localhost:11434",
            ProviderType::OpenRouter => "https://openrouter.ai/api/v1",
            ProviderType::Custom => "https://api.example.com/v1",
        }
    }

    /// Validate the API key format for this provider. Returns `Ok(())` if
    /// the key is well-formed (or unused for Ollama/Custom), `Err(message)`
    /// with a user-facing reason otherwise. Centralised so the "Test
    /// Connection" and "Save Settings" flows can't drift apart — and so
    /// you can't save a key into a slot that would fail validation later.
    pub(super) fn validate_api_key(&self, key: &str) -> Result<(), String> {
        match self {
            ProviderType::OpenAI => {
                if key.is_empty() {
                    Err("OpenAI API key cannot be empty".to_string())
                } else if !key.starts_with("sk-") {
                    Err("OpenAI API keys should start with 'sk-'".to_string())
                } else {
                    Ok(())
                }
            }
            ProviderType::Anthropic => {
                if key.is_empty() {
                    Err("Anthropic API key cannot be empty".to_string())
                } else if !key.starts_with("sk-ant-") {
                    Err("Anthropic API keys should start with 'sk-ant-'".to_string())
                } else {
                    Ok(())
                }
            }
            ProviderType::GoogleGemini => {
                if key.is_empty() {
                    Err("Google Gemini API key cannot be empty".to_string())
                } else {
                    Ok(())
                }
            }
            ProviderType::OpenRouter => {
                if key.is_empty() {
                    Err("OpenRouter API key cannot be empty".to_string())
                } else {
                    Ok(())
                }
            }
            ProviderType::Ollama | ProviderType::Custom => Ok(()),
        }
    }
}

pub struct DirectApiSettingsPageView {
    page: PageType<Self>,
    api_key_manager: ModelHandle<ApiKeyManager>,
    provider_dropdown: ViewHandle<Dropdown<DirectApiPageAction>>,
    model_dropdown: ViewHandle<Dropdown<DirectApiPageAction>>,
    api_key_editor: ViewHandle<EditorView>,
    base_url_editor: ViewHandle<EditorView>,
    selected_provider: RefCell<ProviderType>,
    test_result: RefCell<Option<Result<String, String>>>,
    is_testing: RefCell<bool>,
    show_api_key: Cell<bool>,
    test_button: ViewHandle<ActionButton>,
    save_button: ViewHandle<ActionButton>,
    update_model_list_button: ViewHandle<ActionButton>,
    toggle_visibility_button: ViewHandle<ActionButton>,
    /// JSON-backed cache of provider model lists. Wrapped in `Arc` so the
    /// async fetch closure can clone a reference cheaply without taking
    /// ownership of the view's copy.
    model_cache: Arc<ModelListCache>,
    /// In-memory snapshot of the cached models for the currently-selected
    /// provider. Refreshed by `refresh_model_dropdown()` whenever the
    /// provider changes or a fetch completes.
    cached_models: RefCell<Vec<ModelDescriptor>>,
    /// Tracks which provider currently has an in-flight model list fetch.
    /// Used to prevent duplicate fetches and enable early-return on double-click.
    /// Set to Some(provider_id) when fetch starts, None when fetch completes or errors.
    fetch_in_flight: Cell<Option<ProviderId>>,
}

impl DirectApiSettingsPageView {
    pub fn new(ctx: &mut ViewContext<Self>) -> Self {
        let api_key_manager = ApiKeyManager::handle(ctx);

        let ui_font_size = Appearance::as_ref(ctx).ui_font_size();

        // Create provider dropdown
        let provider_dropdown = ctx.add_typed_action_view(|ctx| {
            let mut dropdown = Dropdown::new(ctx);
            dropdown.set_top_bar_max_width(DROPDOWN_WIDTH);
            dropdown.set_menu_width(DROPDOWN_WIDTH, ctx);

            let items = ProviderType::all()
                .into_iter()
                .map(|provider| {
                    DropdownItem::new(
                        provider.as_str().to_string(),
                        DirectApiPageAction::SelectProvider(provider.as_str().to_string()),
                    )
                })
                .collect();
            dropdown.add_items(items, ctx);
            dropdown.set_selected_by_index(0, ctx);
            dropdown
        });

        // Create model dropdown. Starts empty; populated by
        // `refresh_model_dropdown` once cached models are available.
        let model_dropdown = ctx.add_typed_action_view(|ctx| {
            let mut dropdown = Dropdown::new(ctx);
            dropdown.set_top_bar_max_width(DROPDOWN_WIDTH);
            dropdown.set_menu_width(DROPDOWN_WIDTH, ctx);
            dropdown
        });

        // Create API key input editor (masked by default; toggle button reveals)
        let api_key_editor = ctx.add_typed_action_view(|ctx| {
            let options = SingleLineEditorOptions {
                text: TextOptions {
                    font_size_override: Some(ui_font_size),
                    ..Default::default()
                },
                is_password: true,
                ..Default::default()
            };
            EditorView::single_line(options, ctx)
        });

        api_key_editor.update(ctx, |editor, ctx| {
            editor.set_buffer_text("", ctx);
            editor.set_placeholder_text(ProviderType::OpenAI.api_key_placeholder(), ctx);
        });

        // Create base URL editor
        let base_url_editor = ctx.add_typed_action_view(|ctx| {
            let options = SingleLineEditorOptions {
                text: TextOptions {
                    font_size_override: Some(ui_font_size),
                    ..Default::default()
                },
                ..Default::default()
            };
            EditorView::single_line(options, ctx)
        });

        base_url_editor.update(ctx, |editor, ctx| {
            editor.set_buffer_text("http://localhost:11434", ctx);
        });

        // Create Test Connection button
        let test_button = ctx.add_typed_action_view(|_| {
            ActionButton::new("Test Connection", NakedTheme).on_click(|ctx| {
                ctx.dispatch_typed_action(DirectApiPageAction::TestConnection);
            })
        });

        // Create Save button
        let save_button = ctx.add_typed_action_view(|_| {
            ActionButton::new("Save Settings", NakedTheme).on_click(|ctx| {
                ctx.dispatch_typed_action(DirectApiPageAction::SaveApiKey);
            })
        });

        // Create Update Model List button
        let update_model_list_button = ctx.add_typed_action_view(|_| {
            ActionButton::new("Update Model List", NakedTheme).on_click(|ctx| {
                ctx.dispatch_typed_action(DirectApiPageAction::UpdateModelList);
            })
        });

        // Create show/hide visibility toggle for the API Key input
        let toggle_visibility_button = ctx.add_typed_action_view(|_| {
            ActionButton::new("", NakedTheme)
                .with_icon(Icon::Eye)
                .with_tooltip("Show API key")
                .on_click(|ctx| {
                    ctx.dispatch_typed_action(DirectApiPageAction::ToggleApiKeyVisibility);
                })
        });

        let model_cache = Arc::new(ModelListCache::new().unwrap_or_else(|e| {
            log::warn!("Failed to create ModelListCache, using default: {e}");
            ModelListCache::default()
        }));

        let mut view = Self {
            page: Self::build_page(ctx),
            api_key_manager,
            provider_dropdown,
            model_dropdown,
            api_key_editor,
            base_url_editor,
            selected_provider: RefCell::new(ProviderType::OpenAI),
            test_result: RefCell::new(None),
            is_testing: RefCell::new(false),
            show_api_key: Cell::new(false),
            test_button,
            save_button,
            update_model_list_button,
            toggle_visibility_button,
            model_cache,
            cached_models: RefCell::new(Vec::new()),
            fetch_in_flight: Cell::new(None),
        };

        view.refresh_model_dropdown(ctx);
        view
    }

    fn build_page(_ctx: &mut ViewContext<Self>) -> PageType<Self> {
        let categories = vec![
            Category::new("", vec![Box::new(TitleWidget::default())]),
            Category::new(
                "Provider Configuration",
                vec![
                    Box::new(ProviderSelectorWidget::default()),
                    Box::new(BaseUrlInputWidget::default()),
                    Box::new(ApiKeyInputWidget::default()),
                    Box::new(ModelSelectorWidget::default()),
                    Box::new(ActionButtonsWidget::default()),
                    Box::new(StatusWidget::default()),
                ],
            ),
            Category::new(
                "Current Status",
                vec![Box::new(ConfiguredKeysWidget::default())],
            ),
        ];

        PageType::new_categorized(categories, None)
    }

    fn handle_select_provider(&mut self, provider_name: &str, ctx: &mut ViewContext<Self>) {
        let Some(provider) = ProviderType::from_str(provider_name) else {
            return;
        };

        let api_key_placeholder = provider.api_key_placeholder();
        let base_url_placeholder = provider.base_url_placeholder();
        let default_base_url = provider.default_base_url();
        let needs_base_url = provider.needs_base_url();

        self.api_key_editor.update(ctx, |editor, ctx| {
            editor.set_placeholder_text(api_key_placeholder, ctx);
        });

        if needs_base_url {
            self.base_url_editor.update(ctx, |editor, ctx| {
                // Only seed the buffer when empty — preserve any URL the user
                // typed previously, e.g. a Custom provider's endpoint, instead
                // of clobbering it on every re-selection of the dropdown.
                if editor.buffer_text(ctx).is_empty() {
                    editor.set_buffer_text(default_base_url, ctx);
                }
                editor.set_placeholder_text(base_url_placeholder, ctx);
            });
        }

        // Switching providers always clears the API key buffer and re-masks
        // the field — prevents a key entered for one provider from lingering
        // (possibly unmasked) under a different provider's label.
        self.clear_and_remask_api_key(ctx);

        *self.selected_provider.borrow_mut() = provider;
        *self.test_result.borrow_mut() = None;
        self.refresh_model_dropdown(ctx);
        ctx.notify();
    }

    fn handle_test_connection(&mut self, ctx: &mut ViewContext<Self>) {
        let provider = self.selected_provider.borrow().clone();
        let api_key = self.api_key_editor.as_ref(ctx).buffer_text(ctx);

        // Format validation first — shared with the save path.
        if let Err(msg) = provider.validate_api_key(&api_key) {
            *self.test_result.borrow_mut() = Some(Err(msg));
            ctx.notify();
            return;
        }

        // Custom providers also need a Base URL to be testable.
        if provider == ProviderType::Custom {
            let base_url = self.base_url_editor.as_ref(ctx).buffer_text(ctx);
            if base_url.is_empty() {
                *self.test_result.borrow_mut() =
                    Some(Err("Base URL is required for custom providers".to_string()));
                ctx.notify();
                return;
            }
        }

        // Set testing state and update button
        *self.is_testing.borrow_mut() = true;
        *self.test_result.borrow_mut() = None;
        self.test_button.update(ctx, |button, ctx| {
            button.set_disabled(true, ctx);
        });
        ctx.notify();

        // TODO: Implement actual API validation. For now, surface a
        // provider-appropriate "format valid, full test pending" message.
        let msg = match provider {
            ProviderType::Ollama => "Ollama runs locally - no API key needed".to_string(),
            ProviderType::Custom => "Custom provider configured (full test pending)".to_string(),
            ProviderType::OpenAI
            | ProviderType::Anthropic
            | ProviderType::GoogleGemini
            | ProviderType::OpenRouter => "API key format valid (full test pending)".to_string(),
        };

        *self.is_testing.borrow_mut() = false;
        *self.test_result.borrow_mut() = Some(Ok(msg));
        self.test_button.update(ctx, |button, ctx| {
            button.set_disabled(false, ctx);
        });
        ctx.notify();
    }

    fn handle_save_api_key(&mut self, ctx: &mut ViewContext<Self>) {
        let provider = self.selected_provider.borrow().clone();
        let api_key = self.api_key_editor.as_ref(ctx).buffer_text(ctx);

        // Same format validation as Test Connection — refuses to save a key
        // that would later fail format checks (e.g. a Stripe key pasted into
        // the Anthropic slot).
        if let Err(msg) = provider.validate_api_key(&api_key) {
            *self.test_result.borrow_mut() = Some(Err(msg));
            ctx.notify();
            return;
        }

        let base_url = if provider.needs_base_url() {
            let url = self.base_url_editor.as_ref(ctx).buffer_text(ctx);
            if provider == ProviderType::Custom && url.is_empty() {
                *self.test_result.borrow_mut() =
                    Some(Err("Base URL is required for custom providers".to_string()));
                ctx.notify();
                return;
            }
            if url.is_empty() {
                None
            } else {
                match normalized_base_url_for_provider(&provider, &url) {
                    Ok(url) => Some(url),
                    Err(msg) => {
                        *self.test_result.borrow_mut() = Some(Err(msg));
                        ctx.notify();
                        return;
                    }
                }
            }
        } else {
            None
        };

        // Save to settings.toml via ApiKeyManager.
        self.api_key_manager.update(ctx, |manager, ctx| {
            manager.set_selected_provider(Some(provider.to_provider_id()), ctx);

            match provider {
                ProviderType::OpenAI => {
                    manager.set_openai_key(Some(api_key.clone()), ctx);
                }
                ProviderType::Anthropic => {
                    manager.set_anthropic_key(Some(api_key.clone()), ctx);
                }
                ProviderType::GoogleGemini => {
                    manager.set_google_key(Some(api_key.clone()), ctx);
                }
                ProviderType::Ollama => {
                    // Ollama doesn't need an API key
                }
                ProviderType::OpenRouter => {
                    manager.set_open_router_key(Some(api_key.clone()), ctx);
                }
                ProviderType::Custom => {
                    // Custom providers can optionally have API keys
                    manager.set_custom_key((!api_key.is_empty()).then_some(api_key.clone()), ctx);
                }
            }

            match provider {
                ProviderType::Ollama => {
                    manager.set_ollama_base_url(base_url.clone(), ctx);
                }
                ProviderType::OpenRouter => {
                    manager.set_openrouter_base_url(base_url.clone(), ctx);
                }
                ProviderType::Custom => {
                    manager.set_custom_base_url(base_url.clone(), ctx);
                }
                ProviderType::OpenAI | ProviderType::Anthropic | ProviderType::GoogleGemini => {
                    // These providers use fixed hosted endpoints.
                }
            }
        });

        // Wipe the buffer and re-mask after a successful save. settings.toml
        // is the source of truth from here on; leaving the cleartext key
        // visible (and revealable via the eye toggle) after the user has
        // already saved it is a shoulder-surfing footgun.
        self.clear_and_remask_api_key(ctx);

        *self.test_result.borrow_mut() = Some(Ok(format!(
            "{} settings saved successfully",
            provider.as_str()
        )));
        ctx.notify();
    }

    fn handle_update_model_list(&mut self, ctx: &mut ViewContext<Self>) {
        let provider_type = self.selected_provider.borrow().clone();
        let provider_id = provider_type.to_provider_id();

        // Guard: if a fetch is already in flight for this provider, ignore the
        // double-click to prevent overlapping network calls.
        if self.fetch_in_flight.get() == Some(provider_id) {
            return;
        }

        // Pull the current API key (if any) from the saved settings entries.
        // The in-memory editor buffer is intentionally not consulted — saving
        // is required first, which keeps "fetch models" and "save key" flows
        // distinct and avoids leaking unsaved keys into network calls.
        let api_keys = self.api_key_manager.as_ref(ctx).keys(ctx);
        let api_key = match provider_id {
            ProviderId::OpenAI => api_keys.openai.clone(),
            ProviderId::Anthropic => api_keys.anthropic.clone(),
            ProviderId::GoogleGemini => api_keys.google.clone(),
            ProviderId::Ollama => None,
            ProviderId::OpenRouter => api_keys.open_router.clone(),
            ProviderId::Custom => api_keys.custom.clone(),
        };

        // Resolve base URL for providers that need one, preferring the live
        // editor buffer so the user can fetch against an unsaved URL.
        let base_url = if provider_type.needs_base_url() {
            let url = self.base_url_editor.as_ref(ctx).buffer_text(ctx);
            if url.is_empty() {
                None
            } else {
                match normalized_base_url_for_provider(&provider_type, &url) {
                    Ok(url) => Some(url),
                    Err(msg) => {
                        *self.test_result.borrow_mut() = Some(Err(msg));
                        ctx.notify();
                        return;
                    }
                }
            }
        } else {
            None
        };

        // Build the provider-specific list implementation. Three buckets:
        //   - genai-backed (OpenAI/Anthropic/Gemini/Ollama)
        //   - OpenRouter (raw reqwest)
        //   - Custom (raw reqwest, OpenAI-compatible)
        let provider_impl: Arc<dyn ModelListProvider> = match provider_id.as_genai_adapter_kind() {
            Some(_) => match GenaiBackedListProvider::new(provider_id, api_key, base_url.clone()) {
                Ok(p) => Arc::new(p),
                Err(e) => {
                    *self.test_result.borrow_mut() =
                        Some(Err(format!("Failed to create provider: {e}")));
                    ctx.notify();
                    return;
                }
            },
            None => match provider_id {
                ProviderId::OpenRouter => Arc::new(OpenRouterListProvider::new(
                    api_key.unwrap_or_default(),
                    base_url,
                )),
                ProviderId::Custom => {
                    let Some(url) = base_url else {
                        *self.test_result.borrow_mut() =
                            Some(Err("Base URL is required for custom providers".to_string()));
                        ctx.notify();
                        return;
                    };
                    match CustomListProvider::new(url, api_key) {
                        Ok(provider) => Arc::new(provider),
                        Err(_) => {
                            *self.test_result.borrow_mut() = Some(Err(invalid_base_url_message()));
                            ctx.notify();
                            return;
                        }
                    }
                }
                ProviderId::OpenAI
                | ProviderId::Anthropic
                | ProviderId::GoogleGemini
                | ProviderId::Ollama => {
                    *self.test_result.borrow_mut() = Some(Err("Unsupported provider".to_string()));
                    ctx.notify();
                    return;
                }
            },
        };

        // Stage a "fetching" message and disable the button so the user
        // doesn't kick off overlapping fetches against the same provider.
        *self.test_result.borrow_mut() = Some(Ok("Fetching models...".to_string()));
        self.update_model_list_button.update(ctx, |button, ctx| {
            button.set_disabled(true, ctx);
        });

        // Mark this provider's fetch as in-flight to guard against double-clicks.
        self.fetch_in_flight.set(Some(provider_id));

        ctx.notify();

        let cache = Arc::clone(&self.model_cache);
        let _ = ctx.spawn(
            async move {
                let start = instant::Instant::now();
                let result = provider_impl.list_models().await;
                let elapsed_ms = start.elapsed().as_millis() as u64;
                (provider_id, cache, result, elapsed_ms)
            },
            Self::on_models_fetched,
        );
    }

    /// Callback invoked when an async model list fetch completes. Persists
    /// the result to the cache (on success), refreshes the dropdown, and
    /// surfaces a status message in the test_result field.
    fn on_models_fetched(
        &mut self,
        result: (
            ProviderId,
            Arc<ModelListCache>,
            Result<Vec<ModelDescriptor>, ModelListError>,
            u64,
        ),
        ctx: &mut ViewContext<Self>,
    ) {
        let (provider_id, cache, models_result, duration_ms) = result;

        // Clear the in-flight guard for this provider now that the fetch completed
        // (success or error). This allows subsequent fetches to proceed.
        if self.fetch_in_flight.get() == Some(provider_id) {
            self.fetch_in_flight.set(None);
        }

        match models_result {
            Ok(models) => {
                let count = models.len();
                send_telemetry_from_ctx!(
                    AITelemetryEvent::DirectApiModelListFetchSucceeded {
                        provider: provider_id,
                        model_count: count,
                        duration_ms,
                    },
                    ctx
                );
                match cache.set(provider_id, models) {
                    Ok(()) => {
                        *self.test_result.borrow_mut() =
                            Some(Ok(format!("Fetched {count} models")));
                    }
                    Err(e) => {
                        log::error!("Failed to cache models: {e}");
                        *self.test_result.borrow_mut() = Some(Err(format!(
                            "Fetched {count} models but cache write failed: {e}"
                        )));
                    }
                }
            }
            Err(err) => {
                let error_kind: &'static str = match &err {
                    ModelListError::AuthFailed => "auth_failed",
                    ModelListError::RateLimited { .. } => "rate_limited",
                    ModelListError::Offline => "offline",
                    ModelListError::Unsupported => "unsupported",
                    ModelListError::Network(_) => "network",
                    ModelListError::ParseFailed(_) => "parse_failed",
                    ModelListError::Cancelled => "cancelled",
                };
                send_telemetry_from_ctx!(
                    AITelemetryEvent::DirectApiModelListFetchFailed {
                        provider: provider_id,
                        error_kind,
                    },
                    ctx
                );
                let msg = match err {
                    ModelListError::AuthFailed => "Authentication failed".to_string(),
                    ModelListError::RateLimited { retry_after_secs } => format!(
                        "Rate limited (retry after {}s)",
                        retry_after_secs.unwrap_or(60)
                    ),
                    ModelListError::Offline => "Provider unreachable (offline)".to_string(),
                    ModelListError::Unsupported => {
                        "Provider does not support model listing".to_string()
                    }
                    ModelListError::Network(msg) => format!("Network error: {msg}"),
                    ModelListError::ParseFailed(msg) => format!("Parse error: {msg}"),
                    ModelListError::Cancelled => "Cancelled".to_string(),
                };
                *self.test_result.borrow_mut() = Some(Err(msg));
            }
        }

        // Only refresh the dropdown if the response is for the currently
        // selected provider — otherwise we'd clobber the user's view with
        // a stale provider's list.
        if self.selected_provider.borrow().to_provider_id() == provider_id {
            self.refresh_model_dropdown(ctx);
        }

        self.update_model_list_button.update(ctx, |button, ctx| {
            button.set_disabled(false, ctx);
        });
        ctx.notify();
    }

    fn handle_select_model(&mut self, model_id: String, ctx: &mut ViewContext<Self>) {
        let provider_id = self.selected_provider.borrow().to_provider_id();

        self.api_key_manager.update(ctx, |manager, ctx| {
            manager.set_selected_model(provider_id, model_id.clone(), ctx);
        });

        // Hash the model ID rather than logging it raw — custom providers can
        // expose internal model names which we don't want to ship to
        // telemetry.
        let mut hasher = DefaultHasher::new();
        model_id.hash(&mut hasher);
        let model_id_hash = hasher.finish();
        send_telemetry_from_ctx!(
            AITelemetryEvent::DirectApiModelSelected {
                provider: provider_id,
                model_id_hash,
            },
            ctx
        );

        *self.test_result.borrow_mut() = Some(Ok(format!("Selected model: {model_id}")));
        ctx.notify();
    }

    /// Rebuild the model dropdown items from the cache for the currently
    /// selected provider, then restore the previously-selected model (if any
    /// matches a freshly-cached ID).
    ///
    /// **Stale model handling**: If the user's saved selection is no longer
    /// in the fresh model list, it's added to the dropdown with a "(stale)"
    /// suffix. This allows the user to see their prior choice and either keep
    /// it (for temporary provider outages) or switch to a current model. Stale
    /// models appear at the end of the list after all fresh models.
    fn refresh_model_dropdown(&mut self, ctx: &mut ViewContext<Self>) {
        let provider_id = self.selected_provider.borrow().to_provider_id();

        let models = self
            .model_cache
            .get(provider_id, MODEL_CACHE_TTL)
            .map(|entry| entry.models)
            .unwrap_or_default();

        let saved_selection = self
            .api_key_manager
            .as_ref(ctx)
            .keys(ctx)
            .selected_models
            .get(&provider_id)
            .cloned();

        // Build dropdown items from fresh models, then append stale selection
        // if it's not in the fresh list.
        let mut items: Vec<DropdownItem<DirectApiPageAction>> = models
            .iter()
            .map(|m| {
                let label = m.display_name.clone().unwrap_or_else(|| m.id.clone());
                DropdownItem::new(label, DirectApiPageAction::SelectModel(m.id.clone()))
            })
            .collect();

        // Check if saved selection is stale (not in fresh model list)
        let is_stale = if let Some(ref model_id) = saved_selection {
            !models.iter().any(|m| &m.id == model_id)
        } else {
            false
        };

        // If stale, add it as a special item with "(stale)" suffix at the end.
        // Truncate the model_id to prevent malicious/buggy upstream providers
        // from breaking the dropdown layout with excessively long IDs.
        if is_stale {
            if let Some(ref model_id) = saved_selection {
                let truncated = model_id.chars().take(80).collect::<String>();
                let stale_label = format!("{truncated} (stale)");
                items.push(DropdownItem::new(
                    stale_label,
                    DirectApiPageAction::SelectModel(model_id.clone()),
                ));
            }
        }

        self.model_dropdown.update(ctx, |dropdown, ctx| {
            dropdown.set_items(items, ctx);

            if !models.is_empty() || is_stale {
                if let Some(model_id) = saved_selection {
                    dropdown
                        .set_selected_by_action(DirectApiPageAction::SelectModel(model_id), ctx);
                } else {
                    dropdown.set_selected_by_index(0, ctx);
                }
            } else {
                dropdown.set_selected_to_none(ctx);
            }
        });

        *self.cached_models.borrow_mut() = models;
    }

    fn handle_toggle_api_key_visibility(&mut self, ctx: &mut ViewContext<Self>) {
        let new_show = !self.show_api_key.get();
        self.apply_api_key_visibility(new_show, ctx);
        ctx.notify();
    }

    /// Drive the API key visibility state machine: editor masking, toggle
    /// button's active styling, tooltip, and the page's local `show_api_key`
    /// flag. Centralised so `handle_select_provider` and `handle_save_api_key`
    /// can reset to masked without duplicating the wiring.
    fn apply_api_key_visibility(&mut self, show: bool, ctx: &mut ViewContext<Self>) {
        self.show_api_key.set(show);

        self.api_key_editor.update(ctx, |editor, ctx| {
            editor.set_is_password(!show, ctx);
        });

        let tooltip = visibility_tooltip(show);
        self.toggle_visibility_button.update(ctx, |button, ctx| {
            button.set_active(show, ctx);
            button.set_tooltip(Some(tooltip), ctx);
        });
    }

    /// Clear the API key buffer and force the visibility back to masked.
    /// Called after a successful save and whenever the provider changes, so
    /// a previously-typed (possibly revealed) key never bleeds across
    /// provider switches or lingers after the user has saved it.
    fn clear_and_remask_api_key(&mut self, ctx: &mut ViewContext<Self>) {
        self.api_key_editor.update(ctx, |editor, ctx| {
            editor.set_buffer_text("", ctx);
        });
        self.apply_api_key_visibility(false, ctx);
    }
}

impl Entity for DirectApiSettingsPageView {
    type Event = SettingsPageEvent;
}

impl TypedActionView for DirectApiSettingsPageView {
    type Action = DirectApiPageAction;

    fn handle_action(&mut self, action: &Self::Action, ctx: &mut ViewContext<Self>) {
        match action {
            DirectApiPageAction::SelectProvider(provider_name) => {
                self.handle_select_provider(provider_name, ctx);
            }
            DirectApiPageAction::TestConnection => {
                self.handle_test_connection(ctx);
            }
            DirectApiPageAction::SaveApiKey => {
                self.handle_save_api_key(ctx);
            }
            DirectApiPageAction::UpdateModelList => {
                self.handle_update_model_list(ctx);
            }
            DirectApiPageAction::ToggleApiKeyVisibility => {
                self.handle_toggle_api_key_visibility(ctx);
            }
            DirectApiPageAction::SelectModel(model_id) => {
                self.handle_select_model(model_id.clone(), ctx);
            }
        }
    }
}

impl View for DirectApiSettingsPageView {
    fn ui_name() -> &'static str {
        "DirectApiSettingsPage"
    }

    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        self.page.render(self, app)
    }
}

impl SettingsPageMeta for DirectApiSettingsPageView {
    fn section() -> SettingsSection {
        SettingsSection::DirectApi
    }

    fn should_render(&self, _ctx: &AppContext) -> bool {
        true
    }

    fn update_filter(&mut self, query: &str, ctx: &mut ViewContext<Self>) -> MatchData {
        self.page.update_filter(query, ctx)
    }

    fn scroll_to_widget(&mut self, widget_id: &'static str) {
        self.page.scroll_to_widget(widget_id)
    }

    fn clear_highlighted_widget(&mut self) {
        self.page.clear_highlighted_widget();
    }
}

impl From<ViewHandle<DirectApiSettingsPageView>> for SettingsPageViewHandle {
    fn from(view_handle: ViewHandle<DirectApiSettingsPageView>) -> Self {
        SettingsPageViewHandle::DirectApi(view_handle)
    }
}

#[derive(Default)]
struct TitleWidget {}

impl SettingsWidget for TitleWidget {
    type View = DirectApiSettingsPageView;

    fn search_terms(&self) -> &str {
        "direct api provider openai anthropic ollama gemini api key llm model settings"
    }

    fn render(
        &self,
        _view: &DirectApiSettingsPageView,
        appearance: &Appearance,
        _app: &AppContext,
    ) -> Box<dyn Element> {
        let description_text = "Configure direct API access to LLM providers. \
            This feature allows you to use your own API keys for OpenAI, Anthropic, Google Gemini, \
            OpenRouter, or run models locally with Ollama. You can also configure custom OpenAI-compatible providers.";

        let description = appearance
            .ui_builder()
            .span(description_text)
            .with_soft_wrap()
            .build()
            .with_margin_bottom(ITEM_VERTICAL_SPACING)
            .finish();

        Flex::column().with_child(description).finish()
    }
}

#[derive(Default)]
struct ProviderSelectorWidget {}

impl SettingsWidget for ProviderSelectorWidget {
    type View = DirectApiSettingsPageView;

    fn search_terms(&self) -> &str {
        "provider select openai anthropic ollama gemini dropdown"
    }

    fn render(
        &self,
        view: &DirectApiSettingsPageView,
        appearance: &Appearance,
        _app: &AppContext,
    ) -> Box<dyn Element> {
        use warpui::elements::ChildView;

        let mut column = Flex::column();

        // Label
        let label = appearance
            .ui_builder()
            .span("Provider")
            .build()
            .with_margin_bottom(8.)
            .finish();
        column.add_child(label);

        // Dropdown
        let dropdown = ChildView::new(&view.provider_dropdown).finish();
        column.add_child(
            Container::new(dropdown)
                .with_margin_bottom(ITEM_VERTICAL_SPACING)
                .finish(),
        );

        column.finish()
    }
}

#[derive(Default)]
struct BaseUrlInputWidget {}

impl SettingsWidget for BaseUrlInputWidget {
    type View = DirectApiSettingsPageView;

    fn search_terms(&self) -> &str {
        "base url endpoint server ollama openrouter custom"
    }

    fn render(
        &self,
        view: &DirectApiSettingsPageView,
        appearance: &Appearance,
        _app: &AppContext,
    ) -> Box<dyn Element> {
        let provider = view.selected_provider.borrow().clone();

        // Only show for providers that need base URL.
        if !provider.needs_base_url() {
            return Container::new(appearance.ui_builder().span("").build().finish()).finish();
        }

        let mut column = Flex::column();

        // Label
        let label = appearance
            .ui_builder()
            .span("Base URL")
            .build()
            .with_margin_bottom(8.)
            .finish();
        column.add_child(label);

        // Chromed editor
        let editor = render_chromed_input(view.base_url_editor.clone(), appearance);
        column.add_child(
            Container::new(editor)
                .with_margin_bottom(ITEM_VERTICAL_SPACING)
                .finish(),
        );

        column.finish()
    }
}

#[derive(Default)]
struct ApiKeyInputWidget {}

impl SettingsWidget for ApiKeyInputWidget {
    type View = DirectApiSettingsPageView;

    fn search_terms(&self) -> &str {
        "api key input text field password"
    }

    fn render(
        &self,
        view: &DirectApiSettingsPageView,
        appearance: &Appearance,
        _app: &AppContext,
    ) -> Box<dyn Element> {
        use warpui::elements::ChildView;

        let mut column = Flex::column();

        // Label
        let label = appearance
            .ui_builder()
            .span("API Key")
            .build()
            .with_margin_bottom(8.)
            .finish();
        column.add_child(label);

        // Chromed editor + eye toggle, side-by-side
        let editor = render_chromed_input(view.api_key_editor.clone(), appearance);
        let toggle = ChildView::new(&view.toggle_visibility_button).finish();
        let row = Flex::row()
            .with_child(Container::new(editor).with_margin_right(8.).finish())
            .with_child(toggle)
            .finish();

        column.add_child(
            Container::new(row)
                .with_margin_bottom(ITEM_VERTICAL_SPACING)
                .finish(),
        );

        column.finish()
    }
}

#[derive(Default)]
struct ModelSelectorWidget {}

impl SettingsWidget for ModelSelectorWidget {
    type View = DirectApiSettingsPageView;

    fn search_terms(&self) -> &str {
        "model select dropdown llm gpt claude gemini llama list"
    }

    fn render(
        &self,
        view: &DirectApiSettingsPageView,
        appearance: &Appearance,
        _app: &AppContext,
    ) -> Box<dyn Element> {
        use warpui::elements::ChildView;

        // Hide widget when DirectApiModelSelection feature flag is disabled
        if !FeatureFlag::DirectApiModelSelection.is_enabled() {
            return Container::new(Flex::column().finish()).finish();
        }

        let mut column = Flex::column();

        // Label
        let label = appearance
            .ui_builder()
            .span("Model")
            .build()
            .with_margin_bottom(8.)
            .finish();
        column.add_child(label);

        let cached_models = view.cached_models.borrow();

        if cached_models.is_empty() {
            // Placeholder when no models are cached yet. Drives users to the
            // "Update Model List" button before they try to pick a model.
            let placeholder = appearance
                .ui_builder()
                .span("Click 'Update Model List' to fetch available models")
                .build()
                .with_margin_bottom(ITEM_VERTICAL_SPACING)
                .finish();
            column.add_child(placeholder);
        } else {
            let dropdown = ChildView::new(&view.model_dropdown).finish();
            column.add_child(
                Container::new(dropdown)
                    .with_margin_bottom(ITEM_VERTICAL_SPACING)
                    .finish(),
            );
        }

        column.finish()
    }
}

#[derive(Default)]
struct ActionButtonsWidget {}

impl SettingsWidget for ActionButtonsWidget {
    type View = DirectApiSettingsPageView;

    fn search_terms(&self) -> &str {
        "test connection save button action"
    }

    fn render(
        &self,
        view: &DirectApiSettingsPageView,
        _appearance: &Appearance,
        _app: &AppContext,
    ) -> Box<dyn Element> {
        use warpui::elements::ChildView;

        let mut row = Flex::row();

        // Test Connection button
        row.add_child(
            Container::new(ChildView::new(&view.test_button).finish())
                .with_margin_right(12.)
                .finish(),
        );

        // Save button
        row.add_child(
            Container::new(ChildView::new(&view.save_button).finish())
                .with_margin_right(12.)
                .finish(),
        );

        // Update Model List button
        row.add_child(ChildView::new(&view.update_model_list_button).finish());

        Container::new(row.finish())
            .with_margin_bottom(ITEM_VERTICAL_SPACING)
            .finish()
    }
}

#[derive(Default)]
struct StatusWidget {}

impl SettingsWidget for StatusWidget {
    type View = DirectApiSettingsPageView;

    fn search_terms(&self) -> &str {
        "status result error success message feedback"
    }

    fn render(
        &self,
        view: &DirectApiSettingsPageView,
        appearance: &Appearance,
        _app: &AppContext,
    ) -> Box<dyn Element> {
        let test_result = view.test_result.borrow();

        if let Some(result) = test_result.as_ref() {
            match result {
                Ok(msg) => {
                    let message = format!("✓ {}", msg);
                    Container::new(appearance.ui_builder().span(message).build().finish())
                        .with_margin_bottom(ITEM_VERTICAL_SPACING)
                        .finish()
                }
                Err(msg) => {
                    let message = format!("✗ {}", msg);
                    Container::new(appearance.ui_builder().span(message).build().finish())
                        .with_margin_bottom(ITEM_VERTICAL_SPACING)
                        .finish()
                }
            }
        } else {
            Container::new(appearance.ui_builder().span("").build().finish()).finish()
        }
    }
}

#[derive(Default)]
struct ConfiguredKeysWidget {}

impl SettingsWidget for ConfiguredKeysWidget {
    type View = DirectApiSettingsPageView;

    fn search_terms(&self) -> &str {
        "configured keys status openai anthropic gemini current"
    }

    fn render(
        &self,
        view: &DirectApiSettingsPageView,
        appearance: &Appearance,
        app: &AppContext,
    ) -> Box<dyn Element> {
        let mut column = Flex::column();

        let description = appearance
            .ui_builder()
            .span("Currently configured API keys:")
            .build()
            .with_margin_bottom(12.)
            .finish();
        column.add_child(description);

        // Show current API key status
        let keys = view.api_key_manager.as_ref(app).keys(app);
        let mut has_keys = false;

        if keys.openai.is_some() {
            has_keys = true;
            column.add_child(
                appearance
                    .ui_builder()
                    .span("✓ OpenAI API key configured")
                    .build()
                    .with_margin_bottom(4.)
                    .finish(),
            );
        }

        if keys.anthropic.is_some() {
            has_keys = true;
            column.add_child(
                appearance
                    .ui_builder()
                    .span("✓ Anthropic API key configured")
                    .build()
                    .with_margin_bottom(4.)
                    .finish(),
            );
        }

        if keys.google.is_some() {
            has_keys = true;
            column.add_child(
                appearance
                    .ui_builder()
                    .span("✓ Google Gemini API key configured")
                    .build()
                    .with_margin_bottom(4.)
                    .finish(),
            );
        }

        if keys.open_router.is_some() {
            has_keys = true;
            column.add_child(
                appearance
                    .ui_builder()
                    .span("✓ OpenRouter API key configured")
                    .build()
                    .with_margin_bottom(4.)
                    .finish(),
            );
        }

        if !has_keys {
            column.add_child(
                appearance
                    .ui_builder()
                    .span("⚠ No API keys configured yet")
                    .build()
                    .with_margin_bottom(4.)
                    .finish(),
            );
        }

        column.finish()
    }
}

#[cfg(test)]
#[path = "direct_api_page_tests.rs"]
mod tests;
