use super::{
    settings_page::{
        Category, MatchData, PageType, SettingsPageEvent, SettingsPageMeta, SettingsPageViewHandle,
        SettingsWidget,
    },
    SettingsSection,
};
use crate::appearance::Appearance;
use crate::editor::{EditorView, SingleLineEditorOptions, TextOptions};
use crate::view_components::action_button::{ActionButton, NakedTheme};
use crate::view_components::{Dropdown, DropdownItem};
use ::ai::api_keys::ApiKeyManager;
use std::cell::RefCell;
use warpui::{
    elements::{Container, Element, Flex, ParentElement},
    ui_components::components::UiComponent,
    AppContext, Entity, ModelHandle, SingletonEntity, TypedActionView, View, ViewContext,
    ViewHandle,
};

const ITEM_VERTICAL_SPACING: f32 = 24.;
const DROPDOWN_WIDTH: f32 = 225.;

#[derive(Debug, Clone)]
pub enum DirectApiPageAction {
    SelectProvider(String),
    TestConnection,
    SaveApiKey,
}

#[derive(Debug, Clone, PartialEq)]
enum ProviderType {
    OpenAI,
    Anthropic,
    GoogleGemini,
    Ollama,
}

impl ProviderType {
    fn as_str(&self) -> &'static str {
        match self {
            ProviderType::OpenAI => "OpenAI",
            ProviderType::Anthropic => "Anthropic",
            ProviderType::GoogleGemini => "Google Gemini",
            ProviderType::Ollama => "Ollama",
        }
    }

    fn from_str(s: &str) -> Option<Self> {
        match s {
            "OpenAI" => Some(ProviderType::OpenAI),
            "Anthropic" => Some(ProviderType::Anthropic),
            "Google Gemini" => Some(ProviderType::GoogleGemini),
            "Ollama" => Some(ProviderType::Ollama),
            _ => None,
        }
    }

    fn all() -> Vec<Self> {
        vec![
            ProviderType::OpenAI,
            ProviderType::Anthropic,
            ProviderType::GoogleGemini,
            ProviderType::Ollama,
        ]
    }
}

pub struct DirectApiSettingsPageView {
    page: PageType<Self>,
    api_key_manager: ModelHandle<ApiKeyManager>,
    provider_dropdown: ViewHandle<Dropdown<DirectApiPageAction>>,
    api_key_editor: ViewHandle<EditorView>,
    selected_provider: RefCell<ProviderType>,
    test_result: RefCell<Option<Result<String, String>>>,
    is_testing: RefCell<bool>,
    test_button: ViewHandle<ActionButton>,
    save_button: ViewHandle<ActionButton>,
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

        // Create API key input editor
        let api_key_editor = ctx.add_typed_action_view(|ctx| {
            let options = SingleLineEditorOptions {
                text: TextOptions {
                    font_size_override: Some(ui_font_size),
                    ..Default::default()
                },
                ..Default::default()
            };
            EditorView::single_line(options, ctx)
        });

        api_key_editor.update(ctx, |editor, ctx| {
            editor.set_buffer_text("", ctx);
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

        Self {
            page: Self::build_page(ctx),
            api_key_manager,
            provider_dropdown,
            api_key_editor,
            selected_provider: RefCell::new(ProviderType::OpenAI),
            test_result: RefCell::new(None),
            is_testing: RefCell::new(false),
            test_button,
            save_button,
        }
    }

    fn build_page(_ctx: &mut ViewContext<Self>) -> PageType<Self> {
        let categories = vec![
            Category::new("", vec![Box::new(TitleWidget::default())]),
            Category::new(
                "Provider Configuration",
                vec![
                    Box::new(ProviderSelectorWidget::default()),
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

    fn handle_select_provider(&mut self, provider_name: &str, _ctx: &mut ViewContext<Self>) {
        if let Some(provider) = ProviderType::from_str(provider_name) {
            *self.selected_provider.borrow_mut() = provider;
            *self.test_result.borrow_mut() = None;
        }
    }

    fn handle_test_connection(&mut self, ctx: &mut ViewContext<Self>) {
        let provider = self.selected_provider.borrow().clone();
        let api_key = self.api_key_editor.as_ref(ctx).buffer_text(ctx);

        if api_key.is_empty() && provider != ProviderType::Ollama {
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

        if api_key.is_empty() && provider != ProviderType::Ollama {
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
            }
        });

        *self.test_result.borrow_mut() = Some(Ok(format!(
            "{} API key saved successfully to keychain",
            provider.as_str()
        )));
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
            or run models locally with Ollama.";

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

        let provider = view.selected_provider.borrow().clone();

        // Label with note for Ollama
        let label = if provider == ProviderType::Ollama {
            appearance
                .ui_builder()
                .span("API Key (not required for Ollama)")
                .build()
                .with_margin_bottom(8.)
                .finish()
        } else {
            appearance
                .ui_builder()
                .span("API Key")
                .build()
                .with_margin_bottom(8.)
                .finish()
        };
        column.add_child(label);

        // Show actual editor
        let editor = ChildView::new(&view.api_key_editor).finish();
        column.add_child(
            Container::new(editor)
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
        row.add_child(ChildView::new(&view.save_button).finish());

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
