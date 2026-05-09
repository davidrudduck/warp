use super::{
    settings_page::{
        Category, MatchData, PageType, SettingsPageEvent,
        SettingsPageMeta, SettingsPageViewHandle, SettingsWidget,
    },
    SettingsSection,
};
use crate::appearance::Appearance;
use ::ai::api_keys::ApiKeyManager;
use warpui::{
    elements::{
        Container, Element, Flex, ParentElement,
    },
    ui_components::components::UiComponent,
    AppContext, Entity, ModelHandle, SingletonEntity, View, ViewContext, ViewHandle,
};

const ITEM_VERTICAL_SPACING: f32 = 24.;

pub struct DirectApiSettingsPageView {
    page: PageType<Self>,
    api_key_manager: ModelHandle<ApiKeyManager>,
}

impl DirectApiSettingsPageView {
    pub fn new(ctx: &mut ViewContext<Self>) -> Self {
        let api_key_manager = ApiKeyManager::handle(ctx);

        Self {
            page: Self::build_page(ctx),
            api_key_manager,
        }
    }

    fn build_page(_ctx: &mut ViewContext<Self>) -> PageType<Self> {
        let categories = vec![
            Category::new("", vec![Box::new(TitleWidget::default())]),
            Category::new(
                "Provider Configuration",
                vec![Box::new(ProviderConfigWidget::default())],
            ),
        ];

        PageType::new_categorized(categories, None)
    }
}

impl Entity for DirectApiSettingsPageView {
    type Event = SettingsPageEvent;
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
struct ProviderConfigWidget {}

impl SettingsWidget for ProviderConfigWidget {
    type View = DirectApiSettingsPageView;

    fn search_terms(&self) -> &str {
        "provider select api key test connection save openai anthropic ollama gemini"
    }

    fn render(
        &self,
        view: &DirectApiSettingsPageView,
        appearance: &Appearance,
        app: &AppContext,
    ) -> Box<dyn Element> {
        let mut column = Flex::column();

        // Show instructions
        let instructions = vec![
            "To configure Direct API access:",
            "",
            "1. Provider Selection:",
            "   • OpenAI: Use your own OpenAI API key (gpt-4o, etc.)",
            "   • Anthropic: Use your own Anthropic API key (claude-3-5-sonnet, etc.)",
            "   • Google Gemini: Use your own Google API key (gemini-2.0-flash, etc.)",
            "   • Ollama: Run models locally on your machine (no API key needed)",
            "",
            "2. API Key Storage:",
            "   • API keys are stored securely in your system keychain",
            "   • Access is requested only when first using AI features",
            "   • Keys are encrypted and never sent to Warp servers",
            "",
            "3. Configuration:",
            "   • Use the Settings → Direct API menu (coming soon in UI)",
            "   • Or configure via command line tools",
            "",
            "4. Current Status:",
        ];

        for line in instructions {
            let text_elem = appearance
                .ui_builder()
                .span(line)
                .build()
                .finish();

            column.add_child(
                Container::new(text_elem)
                    .with_margin_bottom(if line.is_empty() { 8. } else { 4. })
                    .finish(),
            );
        }

        // Show current API key status
        let keys = view.api_key_manager.as_ref(app).keys(app);
        let mut has_keys = false;

        column.add_child(
            Container::new(
                appearance
                    .ui_builder()
                    .span("")
                    .build()
                    .with_margin_top(8.)
                    .finish(),
            )
            .finish(),
        );

        if keys.openai.is_some() {
            has_keys = true;
            column.add_child(
                appearance
                    .ui_builder()
                    .span("   ✓ OpenAI API key configured")
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
                    .span("   ✓ Anthropic API key configured")
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
                    .span("   ✓ Google Gemini API key configured")
                    .build()
                    .with_margin_bottom(4.)
                    .finish(),
            );
        }

        if !has_keys {
            column.add_child(
                appearance
                    .ui_builder()
                    .span("   ⚠ No API keys configured yet")
                    .build()
                    .with_margin_bottom(4.)
                    .finish(),
            );
        }

        column.add_child(
            Container::new(
                appearance
                    .ui_builder()
                    .span("")
                    .build()
                    .with_margin_top(16.)
                    .finish(),
            )
            .finish(),
        );

        column.add_child(
            appearance
                .ui_builder()
                .span("Note: Full UI for provider selection and API key management is coming soon.")
                .build()
                .finish(),
        );

        column.finish()
    }
}
