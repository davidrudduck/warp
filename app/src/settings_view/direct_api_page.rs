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
use ::ai::url_validation::normalize_direct_api_base_url;
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
const ROW_VERTICAL_SPACING: f32 = 14.;
const DROPDOWN_WIDTH: f32 = 225.;
const ROW_KEY_INPUT_MAX_WIDTH: f32 = 260.;
const BASE_URL_INPUT_MAX_WIDTH: f32 = 420.;
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
        ProviderType::Ollama | ProviderType::OpenRouter | ProviderType::Custom => {
            normalize_direct_api_base_url(url)
        }
        ProviderType::OpenAI | ProviderType::Anthropic | ProviderType::GoogleGemini => {
            normalize_direct_api_base_url(url)
        }
    }
    .map_err(|_| invalid_base_url_message())
}

fn render_chromed_input_with_max_width(
    editor: ViewHandle<EditorView>,
    appearance: &Appearance,
    max_width: f32,
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
        .with_max_width(max_width)
        .finish()
}

#[derive(Debug, Clone, PartialEq)]
pub enum DirectApiPageAction {
    TestConnection(String),
    SaveApiKey(String),
    UpdateModelList(String),
    ToggleProviderEnabled(String),
    ToggleApiKeyVisibility(String),
    SelectModel(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
            ProviderType::Anthropic,
            ProviderType::GoogleGemini,
            ProviderType::Ollama,
            ProviderType::OpenAI,
            ProviderType::OpenRouter,
            ProviderType::Custom,
        ]
    }

    pub(super) fn from_provider_id(provider_id: ProviderId) -> Self {
        match provider_id {
            ProviderId::OpenAI => ProviderType::OpenAI,
            ProviderId::Anthropic => ProviderType::Anthropic,
            ProviderId::GoogleGemini => ProviderType::GoogleGemini,
            ProviderId::Ollama => ProviderType::Ollama,
            ProviderId::OpenRouter => ProviderType::OpenRouter,
            ProviderId::Custom => ProviderType::Custom,
        }
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
    provider_rows: Vec<ProviderRowState>,
    /// JSON-backed cache of provider model lists. Wrapped in `Arc` so the
    /// async fetch closure can clone a reference cheaply without taking
    /// ownership of the view's copy.
    model_cache: Arc<ModelListCache>,
}

struct ProviderRowState {
    provider: ProviderType,
    api_key_editor: ViewHandle<EditorView>,
    base_url_editor: Option<ViewHandle<EditorView>>,
    test_button: ViewHandle<ActionButton>,
    save_button: ViewHandle<ActionButton>,
    refresh_button: ViewHandle<ActionButton>,
    enable_button: ViewHandle<ActionButton>,
    toggle_visibility_button: ViewHandle<ActionButton>,
    model_dropdown: ViewHandle<Dropdown<DirectApiPageAction>>,
    test_result: RefCell<Option<Result<String, String>>>,
    show_api_key: Cell<bool>,
    cached_models: RefCell<Vec<ModelDescriptor>>,
    model_dropdown_has_items: Cell<bool>,
    fetch_in_flight: Cell<bool>,
}

impl DirectApiSettingsPageView {
    pub fn new(ctx: &mut ViewContext<Self>) -> Self {
        let api_key_manager = ApiKeyManager::handle(ctx);
        let provider_rows = ProviderType::all()
            .into_iter()
            .map(|provider| Self::build_provider_row(provider, ctx))
            .collect();

        let model_cache = Arc::new(ModelListCache::new().unwrap_or_else(|e| {
            log::warn!("Failed to create ModelListCache, using default: {e}");
            ModelListCache::default()
        }));

        let mut view = Self {
            page: Self::build_page(ctx),
            api_key_manager,
            provider_rows,
            model_cache,
        };

        view.hydrate_provider_rows_from_settings(ctx);
        for provider in ProviderType::all() {
            view.refresh_model_dropdown(provider, ctx);
        }
        view.sync_enable_button_labels(ctx);
        view
    }

    fn build_provider_row(provider: ProviderType, ctx: &mut ViewContext<Self>) -> ProviderRowState {
        let ui_font_size = Appearance::as_ref(ctx).ui_font_size();
        let provider_name = provider.as_str().to_string();
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
            editor.set_placeholder_text(provider.api_key_placeholder(), ctx);
        });

        let base_url_editor = provider.needs_base_url().then(|| {
            let editor = ctx.add_typed_action_view(|ctx| {
                let options = SingleLineEditorOptions {
                    text: TextOptions {
                        font_size_override: Some(ui_font_size),
                        ..Default::default()
                    },
                    ..Default::default()
                };
                EditorView::single_line(options, ctx)
            });

            editor.update(ctx, |editor, ctx| {
                editor.set_buffer_text(provider.default_base_url(), ctx);
                editor.set_placeholder_text(provider.base_url_placeholder(), ctx);
            });
            editor
        });

        let test_provider = provider_name.clone();
        let test_button = ctx.add_typed_action_view(move |_| {
            let provider = test_provider.clone();
            ActionButton::new("Test", NakedTheme).on_click(move |ctx| {
                ctx.dispatch_typed_action(DirectApiPageAction::TestConnection(provider.clone()));
            })
        });

        let save_provider = provider_name.clone();
        let save_button = ctx.add_typed_action_view(move |_| {
            let provider = save_provider.clone();
            ActionButton::new("Save", NakedTheme).on_click(move |ctx| {
                ctx.dispatch_typed_action(DirectApiPageAction::SaveApiKey(provider.clone()));
            })
        });

        let refresh_provider = provider_name.clone();
        let refresh_button = ctx.add_typed_action_view(move |_| {
            let provider = refresh_provider.clone();
            ActionButton::new("Refresh models", NakedTheme).on_click(move |ctx| {
                ctx.dispatch_typed_action(DirectApiPageAction::UpdateModelList(provider.clone()));
            })
        });

        let enable_provider = provider_name.clone();
        let enable_button = ctx.add_typed_action_view(move |_| {
            let provider = enable_provider.clone();
            ActionButton::new("Enable", NakedTheme).on_click(move |ctx| {
                ctx.dispatch_typed_action(DirectApiPageAction::ToggleProviderEnabled(
                    provider.clone(),
                ));
            })
        });

        let visibility_provider = provider_name;
        let toggle_visibility_button = ctx.add_typed_action_view(move |_| {
            let provider = visibility_provider.clone();
            ActionButton::new("", NakedTheme)
                .with_icon(Icon::Eye)
                .with_tooltip("Show API key")
                .on_click(move |ctx| {
                    ctx.dispatch_typed_action(DirectApiPageAction::ToggleApiKeyVisibility(
                        provider.clone(),
                    ));
                })
        });

        let model_dropdown = ctx.add_typed_action_view(|ctx| {
            let mut dropdown = Dropdown::new(ctx);
            dropdown.set_top_bar_max_width(DROPDOWN_WIDTH);
            dropdown.set_menu_width(DROPDOWN_WIDTH, ctx);
            dropdown
        });

        ProviderRowState {
            provider,
            api_key_editor,
            base_url_editor,
            test_button,
            save_button,
            refresh_button,
            enable_button,
            toggle_visibility_button,
            model_dropdown,
            test_result: RefCell::new(None),
            show_api_key: Cell::new(false),
            cached_models: RefCell::new(Vec::new()),
            model_dropdown_has_items: Cell::new(false),
            fetch_in_flight: Cell::new(false),
        }
    }

    fn build_page(_ctx: &mut ViewContext<Self>) -> PageType<Self> {
        let categories = vec![
            Category::new("", vec![Box::new(TitleWidget::default())]),
            Category::new("Providers", vec![Box::new(ProviderRowsWidget::default())]),
        ];

        PageType::new_categorized(categories, None)
    }

    fn provider_row(&self, provider: ProviderType) -> Option<&ProviderRowState> {
        self.provider_rows
            .iter()
            .find(|row| row.provider == provider)
    }

    fn handle_test_connection(&mut self, provider: ProviderType, ctx: &mut ViewContext<Self>) {
        let Some(row) = self.provider_row(provider) else {
            return;
        };
        let api_key = row.api_key_editor.as_ref(ctx).buffer_text(ctx);

        // Format validation first — shared with the save path.
        if let Err(msg) = provider.validate_api_key(&api_key) {
            *row.test_result.borrow_mut() = Some(Err(msg));
            ctx.notify();
            return;
        }

        // Custom providers also need a Base URL to be testable.
        if provider == ProviderType::Custom {
            let base_url = row
                .base_url_editor
                .as_ref()
                .map(|editor| editor.as_ref(ctx).buffer_text(ctx))
                .unwrap_or_default();
            if base_url.is_empty() {
                *row.test_result.borrow_mut() =
                    Some(Err("Base URL is required for custom providers".to_string()));
                ctx.notify();
                return;
            }
        }

        *row.test_result.borrow_mut() = None;
        row.test_button.update(ctx, |button, ctx| {
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

        *row.test_result.borrow_mut() = Some(Ok(msg));
        row.test_button.update(ctx, |button, ctx| {
            button.set_disabled(false, ctx);
        });
        ctx.notify();
    }

    fn handle_save_api_key(&mut self, provider: ProviderType, ctx: &mut ViewContext<Self>) {
        let Some(row) = self.provider_row(provider) else {
            return;
        };
        let api_key = row.api_key_editor.as_ref(ctx).buffer_text(ctx);

        // Same format validation as Test Connection — refuses to save a key
        // that would later fail format checks (e.g. a Stripe key pasted into
        // the Anthropic slot).
        if let Err(msg) = provider.validate_api_key(&api_key) {
            *row.test_result.borrow_mut() = Some(Err(msg));
            ctx.notify();
            return;
        }

        let base_url = if provider.needs_base_url() {
            let url = row
                .base_url_editor
                .as_ref()
                .map(|editor| editor.as_ref(ctx).buffer_text(ctx))
                .unwrap_or_default();
            if provider == ProviderType::Custom && url.is_empty() {
                *row.test_result.borrow_mut() =
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
                        *row.test_result.borrow_mut() = Some(Err(msg));
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
                    // Blank optional-key fields mean "leave existing key unchanged".
                    if !api_key.is_empty() {
                        manager.set_custom_key(Some(api_key.clone()), ctx);
                    }
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

        // Keep the saved key in the editor for follow-up actions (test,
        // save again, refresh models), but force it back to masked.
        self.apply_api_key_visibility(provider, false, ctx);
        self.sync_enable_button_label(provider, ctx);

        if let Some(row) = self.provider_row(provider) {
            *row.test_result.borrow_mut() = Some(Ok(format!(
                "{} settings saved successfully",
                provider.as_str()
            )));
        }
        ctx.notify();
    }

    fn handle_toggle_provider_enabled(
        &mut self,
        provider: ProviderType,
        ctx: &mut ViewContext<Self>,
    ) {
        let provider_id = provider.to_provider_id();
        let currently_enabled = {
            let keys = self.api_key_manager.as_ref(ctx).keys(ctx);
            provider_is_enabled(&keys, provider_id)
        };
        self.api_key_manager.update(ctx, |manager, ctx| {
            manager.set_provider_enabled(provider_id, !currently_enabled, ctx);
        });
        self.sync_enable_button_label(provider, ctx);

        if let Some(row) = self.provider_row(provider) {
            *row.test_result.borrow_mut() = Some(Ok(format!(
                "{} {}",
                provider.as_str(),
                if currently_enabled {
                    "disabled"
                } else {
                    "enabled"
                }
            )));
        }
        ctx.notify();
    }

    fn handle_update_model_list(
        &mut self,
        provider_type: ProviderType,
        ctx: &mut ViewContext<Self>,
    ) {
        let provider_id = provider_type.to_provider_id();
        let Some(row) = self.provider_row(provider_type) else {
            return;
        };

        if row.fetch_in_flight.get() {
            return;
        }

        let api_keys = self.api_key_manager.as_ref(ctx).keys(ctx);
        let api_key = match provider_id {
            ProviderId::OpenAI => api_keys.openai.clone(),
            ProviderId::Anthropic => api_keys.anthropic.clone(),
            ProviderId::GoogleGemini => api_keys.google.clone(),
            ProviderId::Ollama => None,
            ProviderId::OpenRouter => api_keys.open_router.clone(),
            ProviderId::Custom => api_keys.custom.clone(),
        };

        let base_url = if provider_type.needs_base_url() {
            let url = row
                .base_url_editor
                .as_ref()
                .map(|editor| editor.as_ref(ctx).buffer_text(ctx))
                .unwrap_or_default();
            if url.is_empty() {
                None
            } else {
                match normalized_base_url_for_provider(&provider_type, &url) {
                    Ok(url) => Some(url),
                    Err(msg) => {
                        *row.test_result.borrow_mut() = Some(Err(msg));
                        ctx.notify();
                        return;
                    }
                }
            }
        } else {
            None
        };

        let provider_impl: Arc<dyn ModelListProvider> = match provider_id.as_genai_adapter_kind() {
            Some(_) => match GenaiBackedListProvider::new(provider_id, api_key, base_url.clone()) {
                Ok(p) => Arc::new(p),
                Err(e) => {
                    *row.test_result.borrow_mut() =
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
                        *row.test_result.borrow_mut() =
                            Some(Err("Base URL is required for custom providers".to_string()));
                        ctx.notify();
                        return;
                    };
                    match CustomListProvider::new(url, api_key) {
                        Ok(provider) => Arc::new(provider),
                        Err(_) => {
                            *row.test_result.borrow_mut() = Some(Err(invalid_base_url_message()));
                            ctx.notify();
                            return;
                        }
                    }
                }
                ProviderId::OpenAI
                | ProviderId::Anthropic
                | ProviderId::GoogleGemini
                | ProviderId::Ollama => {
                    *row.test_result.borrow_mut() = Some(Err("Unsupported provider".to_string()));
                    ctx.notify();
                    return;
                }
            },
        };

        *row.test_result.borrow_mut() = Some(Ok("Fetching models...".to_string()));
        row.refresh_button.update(ctx, |button, ctx| {
            button.set_disabled(true, ctx);
        });
        row.fetch_in_flight.set(true);

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
        let provider = ProviderType::from_provider_id(provider_id);
        let Some(row) = self.provider_row(provider) else {
            return;
        };

        row.fetch_in_flight.set(false);

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
                        *row.test_result.borrow_mut() = Some(Ok(format!("Fetched {count} models")));
                    }
                    Err(e) => {
                        log::error!("Failed to cache models: {e}");
                        *row.test_result.borrow_mut() = Some(Err(format!(
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
                *row.test_result.borrow_mut() = Some(Err(msg));
            }
        }

        self.refresh_model_dropdown(provider, ctx);

        if let Some(row) = self.provider_row(provider) {
            row.refresh_button.update(ctx, |button, ctx| {
                button.set_disabled(false, ctx);
            });
        }
        ctx.notify();
    }

    fn handle_select_model(
        &mut self,
        provider: ProviderType,
        model_id: String,
        ctx: &mut ViewContext<Self>,
    ) {
        let provider_id = provider.to_provider_id();

        self.api_key_manager.update(ctx, |manager, ctx| {
            manager.set_selected_model(provider_id, model_id.clone(), ctx);
        });

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

        if let Some(row) = self.provider_row(provider) {
            *row.test_result.borrow_mut() = Some(Ok(format!("Selected model: {model_id}")));
        }
        ctx.notify();
    }

    fn refresh_model_dropdown(&mut self, provider: ProviderType, ctx: &mut ViewContext<Self>) {
        let provider_id = provider.to_provider_id();

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

        let mut items: Vec<DropdownItem<DirectApiPageAction>> = models
            .iter()
            .map(|m| {
                let label = m.display_name.clone().unwrap_or_else(|| m.id.clone());
                DropdownItem::new(
                    label,
                    DirectApiPageAction::SelectModel(format!("{}|{}", provider.as_str(), m.id)),
                )
            })
            .collect();

        let is_stale = if let Some(ref model_id) = saved_selection {
            !models.iter().any(|m| &m.id == model_id)
        } else {
            false
        };

        if is_stale {
            if let Some(ref model_id) = saved_selection {
                let truncated = model_id.chars().take(80).collect::<String>();
                let stale_label = format!("{truncated} (stale)");
                items.push(DropdownItem::new(
                    stale_label,
                    DirectApiPageAction::SelectModel(format!("{}|{}", provider.as_str(), model_id)),
                ));
            }
        }

        if let Some(row) = self.provider_row(provider) {
            row.model_dropdown_has_items.set(!items.is_empty());
            row.model_dropdown.update(ctx, |dropdown, ctx| {
                dropdown.set_items(items, ctx);

                if !models.is_empty() || is_stale {
                    if let Some(model_id) = saved_selection {
                        dropdown.set_selected_by_action(
                            DirectApiPageAction::SelectModel(format!(
                                "{}|{}",
                                provider.as_str(),
                                model_id
                            )),
                            ctx,
                        );
                    } else {
                        dropdown.set_selected_by_index(0, ctx);
                    }
                } else {
                    dropdown.set_selected_to_none(ctx);
                }
            });

            *row.cached_models.borrow_mut() = models;
        }
    }

    fn handle_toggle_api_key_visibility(
        &mut self,
        provider: ProviderType,
        ctx: &mut ViewContext<Self>,
    ) {
        let Some(row) = self.provider_row(provider) else {
            return;
        };
        let new_show = !row.show_api_key.get();
        self.apply_api_key_visibility(provider, new_show, ctx);
        ctx.notify();
    }

    fn apply_api_key_visibility(
        &mut self,
        provider: ProviderType,
        show: bool,
        ctx: &mut ViewContext<Self>,
    ) {
        let Some(row) = self.provider_row(provider) else {
            return;
        };
        row.show_api_key.set(show);

        row.api_key_editor.update(ctx, |editor, ctx| {
            editor.set_is_password(!show, ctx);
        });

        let tooltip = visibility_tooltip(show);
        row.toggle_visibility_button.update(ctx, |button, ctx| {
            button.set_active(show, ctx);
            button.set_tooltip(Some(tooltip), ctx);
        });
    }

    fn sync_enable_button_labels(&mut self, ctx: &mut ViewContext<Self>) {
        for provider in ProviderType::all() {
            self.sync_enable_button_label(provider, ctx);
        }
    }

    fn hydrate_provider_rows_from_settings(&mut self, ctx: &mut ViewContext<Self>) {
        let keys = self.api_key_manager.as_ref(ctx).keys(ctx);
        for provider in ProviderType::all() {
            let base_url = match provider {
                ProviderType::Ollama => keys
                    .ollama_base_url
                    .clone()
                    .filter(|url| !url.trim().is_empty())
                    .unwrap_or_else(|| provider.default_base_url().to_string()),
                ProviderType::OpenRouter => keys
                    .openrouter_base_url
                    .clone()
                    .filter(|url| !url.trim().is_empty())
                    .unwrap_or_else(|| provider.default_base_url().to_string()),
                ProviderType::Custom => keys.custom_base_url.clone().unwrap_or_default(),
                ProviderType::OpenAI | ProviderType::Anthropic | ProviderType::GoogleGemini => {
                    continue;
                }
            };

            if let Some(row) = self.provider_row(provider) {
                if let Some(editor) = &row.base_url_editor {
                    editor.update(ctx, |editor, ctx| {
                        editor.set_buffer_text(&base_url, ctx);
                    });
                }
            }
        }
    }

    fn sync_enable_button_label(&mut self, provider: ProviderType, ctx: &mut ViewContext<Self>) {
        let provider_id = provider.to_provider_id();
        let enabled = {
            let keys = self.api_key_manager.as_ref(ctx).keys(ctx);
            provider_is_enabled(&keys, provider_id)
        };

        if let Some(row) = self.provider_row(provider) {
            row.enable_button.update(ctx, |button, ctx| {
                button.set_label(if enabled { "Disable" } else { "Enable" }, ctx);
            });
        }
    }
}

fn provider_is_enabled(keys: &::ai::api_keys::ApiKeys, provider_id: ProviderId) -> bool {
    keys.enabled_providers
        .get(&provider_id)
        .copied()
        .unwrap_or_else(|| provider_has_required_config(keys, provider_id))
}

fn provider_has_required_config(keys: &::ai::api_keys::ApiKeys, provider_id: ProviderId) -> bool {
    match provider_id {
        ProviderId::OpenAI => has_non_empty_value(&keys.openai),
        ProviderId::Anthropic => has_non_empty_value(&keys.anthropic),
        ProviderId::GoogleGemini => has_non_empty_value(&keys.google),
        ProviderId::Ollama => has_non_empty_value(&keys.ollama_base_url),
        ProviderId::OpenRouter => has_non_empty_value(&keys.open_router),
        ProviderId::Custom => has_non_empty_value(&keys.custom_base_url),
    }
}

fn has_non_empty_value(value: &Option<String>) -> bool {
    value.as_ref().is_some_and(|value| !value.trim().is_empty())
}

impl Entity for DirectApiSettingsPageView {
    type Event = SettingsPageEvent;
}

impl TypedActionView for DirectApiSettingsPageView {
    type Action = DirectApiPageAction;

    fn handle_action(&mut self, action: &Self::Action, ctx: &mut ViewContext<Self>) {
        match action {
            DirectApiPageAction::TestConnection(provider_name) => {
                if let Some(provider) = ProviderType::from_str(provider_name) {
                    self.handle_test_connection(provider, ctx);
                }
            }
            DirectApiPageAction::SaveApiKey(provider_name) => {
                if let Some(provider) = ProviderType::from_str(provider_name) {
                    self.handle_save_api_key(provider, ctx);
                }
            }
            DirectApiPageAction::UpdateModelList(provider_name) => {
                if let Some(provider) = ProviderType::from_str(provider_name) {
                    self.handle_update_model_list(provider, ctx);
                }
            }
            DirectApiPageAction::ToggleProviderEnabled(provider_name) => {
                if let Some(provider) = ProviderType::from_str(provider_name) {
                    self.handle_toggle_provider_enabled(provider, ctx);
                }
            }
            DirectApiPageAction::ToggleApiKeyVisibility(provider_name) => {
                if let Some(provider) = ProviderType::from_str(provider_name) {
                    self.handle_toggle_api_key_visibility(provider, ctx);
                }
            }
            DirectApiPageAction::SelectModel(payload) => {
                if let Some((provider_name, model_id)) = payload.split_once('|') {
                    if let Some(provider) = ProviderType::from_str(provider_name) {
                        self.handle_select_model(provider, model_id.to_string(), ctx);
                    }
                }
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
struct ProviderRowsWidget {}

impl SettingsWidget for ProviderRowsWidget {
    type View = DirectApiSettingsPageView;

    fn search_terms(&self) -> &str {
        "provider api key save test enable disable model refresh openai anthropic ollama gemini openrouter custom"
    }

    fn render(
        &self,
        view: &DirectApiSettingsPageView,
        appearance: &Appearance,
        app: &AppContext,
    ) -> Box<dyn Element> {
        use warpui::elements::ChildView;

        let keys = view.api_key_manager.as_ref(app).keys(app);
        let mut column = Flex::column();

        for row in &view.provider_rows {
            let provider_id = row.provider.to_provider_id();
            let enabled = provider_is_enabled(&keys, provider_id);

            let provider_label = appearance
                .ui_builder()
                .span(row.provider.as_str())
                .build()
                .finish();
            let provider_cell = ConstrainedBox::new(provider_label)
                .with_width(170.)
                .finish();

            let key_editor = render_chromed_input_with_max_width(
                row.api_key_editor.clone(),
                appearance,
                ROW_KEY_INPUT_MAX_WIDTH,
            );
            let key_cell = Flex::row()
                .with_child(Container::new(key_editor).with_margin_right(8.).finish())
                .with_child(ChildView::new(&row.toggle_visibility_button).finish())
                .finish();

            let enabled_status = appearance
                .ui_builder()
                .span(if enabled { "Enabled" } else { "Disabled" })
                .build()
                .finish();

            let actions = Flex::row()
                .with_child(
                    Container::new(ChildView::new(&row.save_button).finish())
                        .with_margin_right(8.)
                        .finish(),
                )
                .with_child(
                    Container::new(ChildView::new(&row.test_button).finish())
                        .with_margin_right(8.)
                        .finish(),
                )
                .with_child(
                    Container::new(ChildView::new(&row.enable_button).finish())
                        .with_margin_right(8.)
                        .finish(),
                )
                .with_child(ChildView::new(&row.refresh_button).finish())
                .finish();

            let top_row = Flex::row()
                .with_child(
                    Container::new(provider_cell)
                        .with_margin_right(12.)
                        .finish(),
                )
                .with_child(Container::new(key_cell).with_margin_right(12.).finish())
                .with_child(Container::new(actions).with_margin_right(12.).finish())
                .with_child(enabled_status)
                .finish();

            let mut row_column = Flex::column().with_child(top_row);

            if let Some(base_url_editor) = &row.base_url_editor {
                let base_label = appearance.ui_builder().span("Base URL").build().finish();
                let base_editor = render_chromed_input_with_max_width(
                    base_url_editor.clone(),
                    appearance,
                    BASE_URL_INPUT_MAX_WIDTH,
                );
                let base_row = Flex::row()
                    .with_child(
                        Container::new(ConstrainedBox::new(base_label).with_width(170.).finish())
                            .with_margin_right(12.)
                            .finish(),
                    )
                    .with_child(base_editor)
                    .finish();
                row_column.add_child(
                    Container::new(base_row)
                        .with_margin_top(8.)
                        .with_margin_left(182.)
                        .finish(),
                );
            }

            if FeatureFlag::DirectApiModelSelection.is_enabled() {
                if row.model_dropdown_has_items.get() {
                    let model_label = appearance.ui_builder().span("Model").build().finish();
                    let model_row = Flex::row()
                        .with_child(
                            Container::new(
                                ConstrainedBox::new(model_label).with_width(170.).finish(),
                            )
                            .with_margin_right(12.)
                            .finish(),
                        )
                        .with_child(ChildView::new(&row.model_dropdown).finish())
                        .finish();
                    row_column.add_child(
                        Container::new(model_row)
                            .with_margin_top(8.)
                            .with_margin_left(182.)
                            .finish(),
                    );
                }
            }

            if let Some(result) = row.test_result.borrow().as_ref() {
                let message = match result {
                    Ok(msg) => format!("OK: {msg}"),
                    Err(msg) => format!("Error: {msg}"),
                };
                row_column.add_child(
                    Container::new(appearance.ui_builder().span(message).build().finish())
                        .with_margin_top(8.)
                        .with_margin_left(182.)
                        .finish(),
                );
            }

            column.add_child(
                Container::new(row_column.finish())
                    .with_margin_bottom(ROW_VERTICAL_SPACING)
                    .finish(),
            );
        }

        column.finish()
    }
}

#[cfg(test)]
#[path = "direct_api_page_tests.rs"]
mod tests;
