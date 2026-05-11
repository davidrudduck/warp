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
use std::cell::RefCell;
use warp_core::ui::theme::color::internal_colors;
use warpui::{
    elements::{Container, CornerRadius, Element, Fill, Flex, ParentElement, Radius},
    ui_components::components::{Coords, UiComponent, UiComponentStyles},
    AppContext, Entity, ModelHandle, SingletonEntity, TypedActionView, View, ViewContext,
    ViewHandle,
};

const ITEM_VERTICAL_SPACING: f32 = 24.;
const DROPDOWN_WIDTH: f32 = 225.;
const INPUT_BORDER_RADIUS: f32 = 6.;
const INPUT_PADDING_VERTICAL: f32 = 10.;
const INPUT_PADDING_HORIZONTAL: f32 = 12.;

fn render_chromed_input(
    editor: ViewHandle<EditorView>,
    appearance: &Appearance,
) -> Box<dyn Element> {
    let theme = appearance.theme();
    let bg_fill = theme.surface_2();
    let bg_solid = bg_fill.into_solid();
    let text_color = internal_colors::text_main(theme, bg_solid);
    let border_fill = Fill::Solid(internal_colors::neutral_4(theme));

    appearance
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
        .finish()
}

#[derive(Debug, Clone)]
pub enum DirectApiPageAction {
    SelectProvider(String),
    TestConnection,
    SaveApiKey,
    UpdateModelList,
    ToggleApiKeyVisibility,
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
}

pub struct DirectApiSettingsPageView {
    page: PageType<Self>,
    api_key_manager: ModelHandle<ApiKeyManager>,
    provider_dropdown: ViewHandle<Dropdown<DirectApiPageAction>>,
    api_key_editor: ViewHandle<EditorView>,
    base_url_editor: ViewHandle<EditorView>,
    selected_provider: RefCell<ProviderType>,
    test_result: RefCell<Option<Result<String, String>>>,
    is_testing: RefCell<bool>,
    show_api_key: RefCell<bool>,
    test_button: ViewHandle<ActionButton>,
    save_button: ViewHandle<ActionButton>,
    update_model_list_button: ViewHandle<ActionButton>,
    toggle_visibility_button: ViewHandle<ActionButton>,
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
            ActionButton::new("Save to Keychain", NakedTheme).on_click(|ctx| {
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

        Self {
            page: Self::build_page(ctx),
            api_key_manager,
            provider_dropdown,
            api_key_editor,
            base_url_editor,
            selected_provider: RefCell::new(ProviderType::OpenAI),
            test_result: RefCell::new(None),
            is_testing: RefCell::new(false),
            show_api_key: RefCell::new(false),
            test_button,
            save_button,
            update_model_list_button,
            toggle_visibility_button,
        }
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

        *self.selected_provider.borrow_mut() = provider;
        *self.test_result.borrow_mut() = None;
        ctx.notify();
    }

    fn handle_test_connection(&mut self, ctx: &mut ViewContext<Self>) {
        let provider = self.selected_provider.borrow().clone();
        let api_key = self.api_key_editor.as_ref(ctx).buffer_text(ctx);

        if api_key.is_empty()
            && provider != ProviderType::Ollama
            && provider != ProviderType::Custom
        {
            *self.test_result.borrow_mut() = Some(Err("API key cannot be empty".to_string()));
            ctx.notify();
            return;
        }

        // For Ollama, no API key is needed
        if provider == ProviderType::Ollama {
            *self.test_result.borrow_mut() =
                Some(Ok("Ollama runs locally - no API key needed".to_string()));
            ctx.notify();
            return;
        }

        // For Custom provider, API key is optional
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

        // TODO: Implement actual API validation
        // For now, just validate format
        let result = match provider {
            ProviderType::OpenAI => {
                if api_key.starts_with("sk-") {
                    Ok("API key format valid (full test pending)".to_string())
                } else {
                    Err("OpenAI API keys should start with 'sk-'".to_string())
                }
            }
            ProviderType::Anthropic => {
                if api_key.starts_with("sk-ant-") {
                    Ok("API key format valid (full test pending)".to_string())
                } else {
                    Err("Anthropic API keys should start with 'sk-ant-'".to_string())
                }
            }
            ProviderType::GoogleGemini => {
                if !api_key.is_empty() {
                    Ok("API key format valid (full test pending)".to_string())
                } else {
                    Err("Google API key cannot be empty".to_string())
                }
            }
            ProviderType::Ollama => Ok("Ollama runs locally - no API key needed".to_string()),
            ProviderType::OpenRouter => {
                if !api_key.is_empty() {
                    Ok("API key format valid (full test pending)".to_string())
                } else {
                    Err("OpenRouter API key cannot be empty".to_string())
                }
            }
            ProviderType::Custom => {
                Ok("Custom provider configured (full test pending)".to_string())
            }
        };

        *self.is_testing.borrow_mut() = false;
        *self.test_result.borrow_mut() = Some(result);
        self.test_button.update(ctx, |button, ctx| {
            button.set_disabled(false, ctx);
        });
        ctx.notify();
    }

    fn handle_save_api_key(&mut self, ctx: &mut ViewContext<Self>) {
        let provider = self.selected_provider.borrow().clone();
        let api_key = self.api_key_editor.as_ref(ctx).buffer_text(ctx);

        if api_key.is_empty()
            && provider != ProviderType::Ollama
            && provider != ProviderType::Custom
        {
            *self.test_result.borrow_mut() = Some(Err("Cannot save empty API key".to_string()));
            ctx.notify();
            return;
        }

        // Save to keychain via ApiKeyManager
        self.api_key_manager.update(ctx, |manager, ctx| {
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
                    if !api_key.is_empty() {
                        manager.set_openai_key(Some(api_key.clone()), ctx);
                    }
                }
            }
        });

        *self.test_result.borrow_mut() = Some(Ok(format!(
            "{} API key saved successfully to keychain",
            provider.as_str()
        )));
        ctx.notify();
    }

    fn handle_update_model_list(&mut self, ctx: &mut ViewContext<Self>) {
        let provider = self.selected_provider.borrow().clone();

        // TODO: Implement actual model list fetching from provider APIs
        *self.test_result.borrow_mut() = Some(Ok(format!(
            "Model list update for {} is not yet implemented",
            provider.as_str()
        )));
        ctx.notify();
    }

    fn handle_toggle_api_key_visibility(&mut self, ctx: &mut ViewContext<Self>) {
        let new_show = !*self.show_api_key.borrow();
        *self.show_api_key.borrow_mut() = new_show;

        self.api_key_editor.update(ctx, |editor, ctx| {
            editor.set_is_password(!new_show, ctx);
        });

        let tooltip = if new_show {
            "Hide API key"
        } else {
            "Show API key"
        };
        self.toggle_visibility_button.update(ctx, |button, ctx| {
            button.set_active(new_show, ctx);
            button.set_tooltip(Some(tooltip), ctx);
        });

        ctx.notify();
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
