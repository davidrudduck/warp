# Direct API Input Visibility Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the API Key and Base URL inputs on Settings → Agents → Direct API actually visible, mask the API key by default with an eye toggle, and show per-provider placeholders.

**Architecture:** Single-file fix in `app/src/settings_view/direct_api_page.rs` plus a 6-line runtime setter added to `EditorView`. Pure placeholder/url logic moves onto `ProviderType` so it can be unit-tested without a UI harness. Input chrome reuses the `text_input(editor).with_style(UiComponentStyles { ... })` pattern already established in `paste_auth_token_modal.rs:278-295`.

**Tech Stack:** Rust 1.92.0, WarpUI (entity-component), `EditorView`, `ActionButton`, `Icon`, `UiComponentStyles`.

**Spec:** `docs/superpowers/specs/2026-05-11-direct-api-input-visibility-design.md`

---

## File Map

- **Modify** `app/src/editor/view/mod.rs` — add `EditorView::set_is_password` runtime setter.
- **Modify** `app/src/settings_view/direct_api_page.rs` — main UI changes (chrome, placeholders, toggle).
- **Create** `app/src/settings_view/direct_api_page_tests.rs` — unit tests for `ProviderType` placeholder/URL methods.

---

## Task 1: Add runtime `set_is_password` to `EditorView`

**Files:**
- Modify: `app/src/editor/view/mod.rs` (insert near the existing `set_buffer_text` method at ~line 4491)

The current code already has `EditorView.is_password: bool` as a runtime field (`mod.rs:1911`) used by the rendering paths (`mod.rs:4153`, `4168`, `7327`). What's missing is a public mutator. Adding one is a localized, mechanical change with no impact on other consumers.

- [ ] **Step 1: Add the setter method**

Find the existing `pub fn set_buffer_text` (around line 4491). Add this method immediately above or below it, inside the same `impl EditorView` block:

```rust
/// Toggle whether this editor masks its content as a password field.
/// Triggers a re-render via `ctx.notify()`.
pub fn set_is_password(&mut self, is_password: bool, ctx: &mut ViewContext<Self>) {
    if self.is_password != is_password {
        self.is_password = is_password;
        ctx.notify();
    }
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p warp`

Expected: no new errors. If the workspace package containing `EditorView` is named differently, run `cargo check --workspace` instead.

- [ ] **Step 3: Commit**

```bash
git add app/src/editor/view/mod.rs
git commit -m "feat(editor): add runtime EditorView::set_is_password setter

Enables toggling password masking after construction. Used by the
Direct API settings page to implement a show/hide eye toggle."
```

---

## Task 2: Extract pure helpers on `ProviderType` for placeholders + default URLs

**Files:**
- Modify: `app/src/settings_view/direct_api_page.rs:42-92` (the `impl ProviderType` block)

This extraction makes the per-provider strings testable without a UI harness. The existing `default_base_url` already follows this pattern — we extend it with placeholder accessors.

- [ ] **Step 1: Write the failing test file**

Create new file `app/src/settings_view/direct_api_page_tests.rs`:

```rust
use super::direct_api_page::ProviderType;

#[test]
fn api_key_placeholder_for_each_provider() {
    assert_eq!(ProviderType::OpenAI.api_key_placeholder(), "sk-...");
    assert_eq!(ProviderType::Anthropic.api_key_placeholder(), "sk-ant-...");
    assert_eq!(ProviderType::GoogleGemini.api_key_placeholder(), "AIza...");
    assert_eq!(ProviderType::Ollama.api_key_placeholder(), "Optional");
    assert_eq!(ProviderType::OpenRouter.api_key_placeholder(), "sk-or-...");
    assert_eq!(ProviderType::Custom.api_key_placeholder(), "Optional");
}

#[test]
fn base_url_placeholder_for_each_provider() {
    assert_eq!(ProviderType::OpenAI.base_url_placeholder(), "");
    assert_eq!(ProviderType::Anthropic.base_url_placeholder(), "");
    assert_eq!(ProviderType::GoogleGemini.base_url_placeholder(), "");
    assert_eq!(
        ProviderType::Ollama.base_url_placeholder(),
        "http://localhost:11434"
    );
    assert_eq!(
        ProviderType::OpenRouter.base_url_placeholder(),
        "https://openrouter.ai/api/v1"
    );
    assert_eq!(
        ProviderType::Custom.base_url_placeholder(),
        "https://api.example.com/v1"
    );
}

#[test]
fn default_base_url_only_prefilled_for_known_endpoints() {
    assert_eq!(ProviderType::Ollama.default_base_url(), "http://localhost:11434");
    assert_eq!(
        ProviderType::OpenRouter.default_base_url(),
        "https://openrouter.ai/api/v1"
    );
    assert_eq!(ProviderType::Custom.default_base_url(), "");
}
```

- [ ] **Step 2: Wire the test file into `direct_api_page.rs`**

`ProviderType` is currently a non-`pub` enum scoped to the file. Promote it to `pub(super)` so the sibling test module can reach it, and wire the test file at the bottom of `direct_api_page.rs` immediately above the final closing brace (after `ConfiguredKeysWidget`'s `impl` block ends, around line 793):

```rust
#[cfg(test)]
#[path = "direct_api_page_tests.rs"]
mod tests;
```

Update the `ProviderType` declaration at `direct_api_page.rs:32-40`:

```rust
#[derive(Debug, Clone, PartialEq)]
pub(super) enum ProviderType {
    OpenAI,
    Anthropic,
    GoogleGemini,
    Ollama,
    OpenRouter,
    Custom,
}
```

Update the `impl ProviderType` methods at lines 42, 54, 66, 77, 84 to be `pub(super)`:

```rust
impl ProviderType {
    pub(super) fn as_str(&self) -> &'static str { /* unchanged */ }
    pub(super) fn from_str(s: &str) -> Option<Self> { /* unchanged */ }
    pub(super) fn all() -> Vec<Self> { /* unchanged */ }
    pub(super) fn needs_base_url(&self) -> bool { /* unchanged */ }
    pub(super) fn default_base_url(&self) -> &'static str { /* unchanged */ }
    // ... new methods below ...
}
```

Now also update the test file to import via the parent module. Change the first line of `direct_api_page_tests.rs` to:

```rust
use super::ProviderType;
```

(The `mod tests;` block lives inside `direct_api_page.rs`, so `super` resolves to `direct_api_page`.)

- [ ] **Step 3: Run the test to verify it fails**

Run: `cargo nextest run -p warp direct_api_page::tests`

Expected: FAIL — `no method named 'api_key_placeholder' found for enum 'ProviderType'`.

If `cargo nextest` is unavailable, fall back to: `cargo test -p warp direct_api_page::tests`.

- [ ] **Step 4: Add the two new methods to `impl ProviderType`**

Inside the existing `impl ProviderType` block in `direct_api_page.rs`, after `default_base_url` (currently ends at line 91), add:

```rust
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
```

Note: `match` arms are exhaustive — required by CLAUDE.md rule "Never use `_` wildcard in match arms".

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo nextest run -p warp direct_api_page::tests`

Expected: PASS — all three tests green.

- [ ] **Step 6: Commit**

```bash
git add app/src/settings_view/direct_api_page.rs app/src/settings_view/direct_api_page_tests.rs
git commit -m "refactor(settings): extract per-provider placeholder helpers

Adds api_key_placeholder() and base_url_placeholder() on ProviderType
with unit tests. Visibility on ProviderType raised to pub(super) so
the sibling test module can use it."
```

---

## Task 3: Make the API Key editor a password field at construction

**Files:**
- Modify: `app/src/settings_view/direct_api_page.rs:134-148` (the `api_key_editor` construction in `DirectApiSettingsPageView::new`)

- [ ] **Step 1: Set `is_password: true` on construction**

Replace the block at lines 134-148:

```rust
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
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p warp`

Expected: no new errors.

- [ ] **Step 3: Commit**

```bash
git add app/src/settings_view/direct_api_page.rs
git commit -m "feat(settings): mask API key input by default

Sets is_password: true on the Direct API page's API key editor at
construction, with an initial placeholder for the default OpenAI
provider."
```

---

## Task 4: Wrap both editors with visible chrome (`render_chromed_input` helper)

**Files:**
- Modify: `app/src/settings_view/direct_api_page.rs` (imports near line 1-19; `ApiKeyInputWidget::render` at lines 577-617; `BaseUrlInputWidget::render` at lines 529-565; add helper near top of file after constants)

The pattern is established in `paste_auth_token_modal.rs:278-295` and `teams_page.rs:3827-3856`. Replace raw `ChildView::new(&editor).finish()` with `ui_builder.text_input(editor).with_style(...)`.

- [ ] **Step 1: Update the imports at the top of `direct_api_page.rs`**

Replace the import block at lines 1-19 with:

```rust
use super::{
    settings_page::{
        Category, MatchData, PageType, SettingsPageEvent, SettingsPageMeta, SettingsPageViewHandle,
        SettingsWidget,
    },
    SettingsSection,
};
use crate::appearance::Appearance;
use crate::editor::{EditorView, SingleLineEditorOptions, TextOptions};
use crate::themes::theme::Fill as ThemeFill;
use crate::view_components::action_button::{ActionButton, NakedTheme};
use crate::view_components::{Dropdown, DropdownItem};
use ::ai::api_keys::ApiKeyManager;
use std::cell::RefCell;
use warp_core::ui::internal_colors;
use warpui::{
    elements::{Container, Element, Flex, ParentElement},
    ui_components::components::{Coords, CornerRadius, UiComponent, UiComponentStyles},
    AppContext, Entity, ModelHandle, SingletonEntity, TypedActionView, View, ViewContext,
    ViewHandle,
};
```

Note: if the build later reports `internal_colors`, `CornerRadius`, or `Coords` cannot be found at those paths, look at the live imports in `paste_auth_token_modal.rs` and `teams_page.rs` and copy the working paths. Don't guess — the symbols definitely exist, but the re-export surface may differ.

- [ ] **Step 2: Add the `render_chromed_input` helper near the top of the file**

Insert this free function immediately after the `const DROPDOWN_WIDTH` declaration (around line 23), before `pub enum DirectApiPageAction`:

```rust
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
    let border_fill = ThemeFill::Solid(theme.outline());

    appearance
        .ui_builder()
        .text_input(editor)
        .with_style(UiComponentStyles {
            background: Some(bg_fill.into()),
            border_width: Some(1.),
            border_color: Some(border_fill),
            border_radius: Some(CornerRadius::with_all(INPUT_BORDER_RADIUS)),
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
```

If `theme.outline()` doesn't compile, the right call may be `internal_colors::neutral_4(theme)` — that's what `paste_auth_token_modal.rs:234` uses for the same purpose. Adjust to match.

- [ ] **Step 3: Update `BaseUrlInputWidget::render`**

Replace the body of `BaseUrlInputWidget::render` at lines 529-564. The new body:

```rust
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
```

Drop the `use warpui::elements::ChildView;` line at the top of this function — it's no longer needed.

- [ ] **Step 4: Update `ApiKeyInputWidget::render`**

Replace the body of `ApiKeyInputWidget::render` at lines 577-616:

```rust
    fn render(
        &self,
        view: &DirectApiSettingsPageView,
        appearance: &Appearance,
        _app: &AppContext,
    ) -> Box<dyn Element> {
        let mut column = Flex::column();

        // Label
        let label = appearance
            .ui_builder()
            .span("API Key")
            .build()
            .with_margin_bottom(8.)
            .finish();
        column.add_child(label);

        // Chromed editor
        let editor = render_chromed_input(view.api_key_editor.clone(), appearance);
        column.add_child(
            Container::new(editor)
                .with_margin_bottom(ITEM_VERTICAL_SPACING)
                .finish(),
        );

        column.finish()
    }
```

Drop the `use warpui::elements::ChildView;` line — not needed.

Note: the previous Ollama-specific "(not required for Ollama)" label is removed in favor of the placeholder text approach. That's intentional per the spec; the placeholder shows `Optional` when Ollama is selected.

- [ ] **Step 5: Verify the build compiles**

Run: `cargo check -p warp`

Expected: clean compile. If `text_input(...)`, `with_style(...)`, or `internal_colors::text_main(...)` produce errors, cross-reference the working signatures in `paste_auth_token_modal.rs:278-295` — the symbols are identical.

- [ ] **Step 6: Run clippy**

Run: `cargo clippy -p warp --all-targets -- -D warnings`

Expected: zero warnings. If any are reported, fix them inline (most likely candidates: an unused import or a `format!` that should use inline args).

- [ ] **Step 7: Commit**

```bash
git add app/src/settings_view/direct_api_page.rs
git commit -m "fix(settings): give Direct API editors visible chrome

Wraps the API Key and Base URL EditorViews with the standard
text_input chrome pattern (background, border, padding) so they
actually appear as input fields instead of bare cursors."
```

---

## Task 5: Update placeholders on provider change

**Files:**
- Modify: `app/src/settings_view/direct_api_page.rs:224-237` (the `handle_select_provider` method)

- [ ] **Step 1: Update `handle_select_provider`**

Replace the method body at lines 224-237:

```rust
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
                editor.set_buffer_text(default_base_url, ctx);
                editor.set_placeholder_text(base_url_placeholder, ctx);
            });
        }

        *self.selected_provider.borrow_mut() = provider;
        *self.test_result.borrow_mut() = None;
        ctx.notify();
    }
```

Note: Custom's `default_base_url` is `""`, so setting the buffer text to empty + placeholder shows the example URL as a hint instead of pre-filling it — matching the spec's "users should not have to delete a pre-filled example" requirement.

- [ ] **Step 2: Verify build**

Run: `cargo check -p warp`

Expected: clean compile.

- [ ] **Step 3: Commit**

```bash
git add app/src/settings_view/direct_api_page.rs
git commit -m "feat(settings): show per-provider placeholders in Direct API inputs

Placeholders update on provider change to indicate expected API key
format (sk-..., sk-ant-..., AIza..., Optional, sk-or-...) and example
base URLs for Ollama, OpenRouter, and Custom."
```

---

## Task 6: Add the show/hide eye toggle for the API Key

**Files:**
- Modify: `app/src/settings_view/direct_api_page.rs` — action enum (line 25), view struct (line 94), `new` (line 109), `handle_action` (line 390), `ApiKeyInputWidget::render` (the version updated in Task 4)

- [ ] **Step 1: Add a new action variant**

Update the action enum at lines 24-30:

```rust
#[derive(Debug, Clone)]
pub enum DirectApiPageAction {
    SelectProvider(String),
    TestConnection,
    SaveApiKey,
    UpdateModelList,
    ToggleApiKeyVisibility,
}
```

- [ ] **Step 2: Add the new field on the view struct**

Update the struct at lines 94-106:

```rust
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
```

- [ ] **Step 3: Construct the toggle button in `new`**

Add the following block inside `DirectApiSettingsPageView::new` immediately after the `update_model_list_button` construction (around line 185), still before the final `Self { ... }` literal:

```rust
        // Create show/hide visibility toggle for the API Key input
        let toggle_visibility_button = ctx.add_typed_action_view(|_| {
            ActionButton::new("", NakedTheme)
                .with_icon(warp_core::ui::icons::Icon::Eye)
                .with_tooltip("Show API key")
                .on_click(|ctx| {
                    ctx.dispatch_typed_action(DirectApiPageAction::ToggleApiKeyVisibility);
                })
        });
```

Then update the struct literal at the bottom of `new` to include both new fields:

```rust
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
```

If `warp_core::ui::icons::Icon` isn't directly importable, check how `Icon::Eye` is reached in any consumer (`grep -rn "Icon::Eye" app/src`). If the resolved path differs, update both the construction line and the `use` statement accordingly.

- [ ] **Step 4: Add the action handler method**

Inside `impl DirectApiSettingsPageView`, after `handle_update_model_list` (currently ends around line 380), add:

```rust
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
            button.set_tooltip(tooltip, ctx);
        });

        ctx.notify();
    }
```

If `ActionButton` lacks runtime `set_active` and/or `set_tooltip` (verify with `grep -n "pub fn set_active\|pub fn set_tooltip" app/src/view_components/action_button.rs`), drop the `update` block — the icon will still convey state via `show_api_key` being threaded through render if needed. The masking behavior is the critical part; the tooltip update is polish.

- [ ] **Step 5: Wire the action into `handle_action`**

Update the match in `handle_action` (lines 390-405):

```rust
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
```

- [ ] **Step 6: Render the toggle button next to the API Key input**

Update `ApiKeyInputWidget::render` (the version from Task 4) to wrap the chromed editor and the toggle in a row. Replace its body with:

```rust
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
            .with_child(
                Container::new(editor)
                    .with_margin_right(8.)
                    .finish(),
            )
            .with_child(toggle)
            .finish();

        column.add_child(
            Container::new(row)
                .with_margin_bottom(ITEM_VERTICAL_SPACING)
                .finish(),
        );

        column.finish()
    }
```

- [ ] **Step 7: Verify build + clippy**

Run both in parallel:
- `cargo check -p warp`
- `cargo clippy -p warp --all-targets -- -D warnings`

Expected: clean. If `set_is_password`, `set_active`, or `set_tooltip` are reported missing, the call sites need to be reconciled with what those types actually expose — adjust calls to match available methods, falling back to the simpler "masking only, no live tooltip swap" path if needed.

- [ ] **Step 8: Run the unit tests**

Run: `cargo nextest run -p warp direct_api_page`

Expected: all three placeholder tests still pass.

- [ ] **Step 9: Commit**

```bash
git add app/src/settings_view/direct_api_page.rs
git commit -m "feat(settings): add show/hide eye toggle for Direct API key

Adds a NakedTheme ActionButton with Icon::Eye next to the API Key
input. Clicking flips EditorView::set_is_password and updates the
button's active state and tooltip."
```

---

## Task 7: Final formatting + presubmit

**Files:** none (verification only).

- [ ] **Step 1: Format**

Run: `cargo fmt`

Expected: no changes reported, or any reported changes are minor. Commit the formatting only if it changes anything (none expected if previous tasks followed the style).

- [ ] **Step 2: Run full presubmit**

Run: `./script/presubmit`

Expected: PASS — clippy clean, fmt clean, relevant tests green.

If it fails on tests in unrelated crates, that's an existing failure — note it but don't fix in this PR.

- [ ] **Step 3: Manual smoke test (record results in PR description, not in code)**

Run: `cargo run`

In the running app:
1. `Cmd+,` to open Settings.
2. Navigate to Agents → Direct API.
3. The "API Key" field shows a bordered input box with subtle background and the placeholder `sk-...`. Type a few characters — they appear masked as `•••`.
4. Click the eye icon next to the field. The masked characters reveal. Click again — re-mask.
5. Open the provider dropdown and select "Ollama". The placeholder changes to `Optional`. A new "Base URL" field appears above with `http://localhost:11434` pre-filled.
6. Select "OpenRouter". Placeholder changes to `sk-or-...`. Base URL field stays visible with `https://openrouter.ai/api/v1` pre-filled.
7. Select "Custom (OpenAI-compatible)". API Key placeholder shows `Optional`. Base URL field shows empty input with `https://api.example.com/v1` as placeholder hint.
8. Select an OpenAI / Anthropic / Google Gemini provider. Base URL field disappears. API Key placeholder updates.
9. Paste a key, click "Save Settings" — status message still appears.

- [ ] **Step 4: Commit any final formatting**

If `cargo fmt` produced changes:

```bash
git add -A
git commit -m "chore: cargo fmt after Direct API input visibility fix"
```

Otherwise skip.

---

## Spec Coverage Check

| Spec requirement | Implemented in |
|---|---|
| API Key/Base URL inputs visible with chrome | Task 4 (`render_chromed_input` helper + both widget updates) |
| Password masking on by default | Task 3 (`is_password: true` at construction) |
| Show/hide eye toggle | Task 6 (action, state, button, render, handler — needs Task 1's setter) |
| Per-provider placeholder text | Tasks 2 + 5 (`api_key_placeholder` / `base_url_placeholder` methods, applied in `handle_select_provider` and at construction) |
| Custom keeps empty buffer, shows placeholder URL | Task 5 (sets `default_base_url` which is `""` for Custom; placeholder still shows) |
| Ollama API key field stays editable, just `Optional` placeholder | Task 2 + Task 4 (no Ollama branching in `ApiKeyInputWidget::render`; placeholder is `Optional`) |
| Single-file scope + private helper | Tasks 2–6 all in `direct_api_page.rs`; `EditorView` setter is the one cross-file addition, scoped to enable the feature |
| Unit tests for placeholder logic | Task 2 (three tests in `direct_api_page_tests.rs`) |
| Manual smoke test | Task 7 step 3 |
| Runtime `is_password` setter (or fallback to recreation) | Task 1 (setter chosen — simpler than recreation) |

No spec requirements without an implementing task.

## Risk Notes

- The exact import paths for `internal_colors`, `CornerRadius`, `Coords`, and `ThemeFill` may differ from the speculative paths in Task 4 step 1. If any fail to resolve, copy the live imports from `paste_auth_token_modal.rs` (lines 17, 31) — that file uses the exact same primitives. Don't invent paths.
- `ActionButton` may not expose runtime `set_active` / `set_tooltip` setters. If absent, drop those calls from `handle_toggle_api_key_visibility` (Task 6 step 4). Masking still works; only the live tooltip swap is lost.
- `Icon` may live at a different path than `warp_core::ui::icons::Icon`. Confirm with `grep -rn "Icon::Eye" app/src crates` before settling on the import.
