# Direct API Settings — Input Field Visibility & UX

**Date:** 2026-05-11
**Author:** brainstorming session (drudduck@expansionx.com.au)
**Scope:** `app/src/settings_view/direct_api_page.rs` (single file)
**Status:** approved for planning

## Problem

The Direct API settings page (Settings → Agents → Direct API) has unusable input fields:

1. **API Key field is invisible.** Users see only a thin cursor — no input box, no border, no background. Typed characters are visible but the field itself has no chrome, so users believe nothing happened.
2. **Base URL field is invisible for the same reason.** When a provider that requires a base URL is selected (Ollama, OpenRouter, Custom), the field renders but has no visible chrome. Users report "Custom isn't showing a URL input" — it is showing, just invisibly.
3. **No password masking on the API Key.** API keys are rendered as plain text in a shared settings UI. Shoulder-surfing risk.
4. **No placeholder text.** Users have no hint about expected format (`sk-...`, `sk-ant-...`, etc.).

## Root Cause

Both `EditorView` instances in `direct_api_page.rs` are rendered via raw `ChildView::new(&editor)` calls (lines 556, 608). No `UiComponentStyles` are applied, so the editor renders without a background, border, padding, or visible text container.

The correct pattern — already in use by `paste_auth_token_modal.rs:278-295` and `teams_page.rs:3827-3856` — wraps the editor via `appearance.ui_builder().text_input(editor).with_style(UiComponentStyles { ... })`.

`SingleLineEditorOptions` already has a native `is_password: bool` field. `EditorView::set_placeholder_text(...)` already exists. The fix is to use the existing APIs correctly.

## Goals

- API Key and Base URL inputs are visually obvious as text fields.
- API Key is masked by default; an eye toggle reveals the value.
- Each provider shows a format-appropriate placeholder for both fields.
- No new crates, no schema changes, no new dependencies.

## Non-Goals

- Validating API keys against the live provider API (already a follow-up, format-only checks remain).
- Reworking the rest of the Direct API page (status display, action buttons, etc.).
- Cross-provider input components reusable elsewhere — the helper stays private to this file.

## Design

### Scope

Single-file change to `app/src/settings_view/direct_api_page.rs`. A private helper `render_chromed_input(editor, appearance, width)` is extracted to share the input styling between the two callers (API Key, Base URL). Two callers is the right time for the extraction — it's not premature, and it stops the two widgets from drifting.

### The Three Fixes

**Fix A — Visible chrome.** Both editors are wrapped via:

```rust
appearance.ui_builder()
    .text_input(editor)
    .with_style(UiComponentStyles {
        background: Some(theme.surface_2().into()),
        border_width: Some(1.),
        border_color: Some(Fill::Solid(theme.outline())),
        border_radius: Some(CornerRadius::with_all(6.)),
        font_color: Some(internal_colors::text_main(theme, theme.surface_2().into_solid())),
        padding: Some(Coords { top: 10., bottom: 10., left: 12., right: 12. }),
        ..Default::default()
    })
    .build()
    .finish()
```

Exact theme tokens may be adjusted during implementation to match neighbouring settings pages.

**Fix B — Password masking with show/hide toggle.**

- API Key editor is constructed with `SingleLineEditorOptions { is_password: true, .. }`.
- New state on `DirectApiSettingsPageView`: `show_api_key: RefCell<bool>` (default `false`), `toggle_visibility_button: ViewHandle<ActionButton>`.
- New action variant: `DirectApiPageAction::ToggleApiKeyVisibility`.
- Handler flips `show_api_key`, switches the editor's password mode, swaps the icon (eye / eye-slash), calls `ctx.notify()`.
- If `EditorView` has no runtime password-mode setter, the implementation recreates the editor while preserving buffer text (`buffer_text` → reconstruct → `set_buffer_text`) and refocuses it. If even that turns out invasive, the toggle ships as a follow-up and v1 is masked-only — the masking itself is non-negotiable.
- The toggle button sits to the right of the API Key input in a `Flex::row()`. Inline-inside-input layout is a possible future polish.

**Fix C — Placeholders per provider.**

`handle_select_provider` is extended to set placeholder text on both editors. Mappings:

| Provider | API Key placeholder | Base URL placeholder |
|---|---|---|
| OpenAI | `sk-...` | (field hidden) |
| Anthropic | `sk-ant-...` | (field hidden) |
| Google Gemini | `AIza...` | (field hidden) |
| Ollama | `Optional` | `http://localhost:11434` |
| OpenRouter | `sk-or-...` | `https://openrouter.ai/api/v1` |
| Custom | `Optional` | `https://api.example.com/v1` |

Ollama and Custom keep the API Key field visible and editable — Ollama supports optional auth, and most users leave it blank. The `Optional` placeholder communicates that.

For Custom, the base URL **buffer** stays empty (the existing `default_base_url` returns `""`); the **placeholder** is what shows the example URL. Users should not have to delete a pre-filled string.

### State Changes on `DirectApiSettingsPageView`

```rust
pub struct DirectApiSettingsPageView {
    // ... existing fields ...
    show_api_key: RefCell<bool>,              // NEW
    toggle_visibility_button: ViewHandle<ActionButton>, // NEW
}
```

### Action Enum

```rust
pub enum DirectApiPageAction {
    SelectProvider(String),
    TestConnection,
    SaveApiKey,
    UpdateModelList,
    ToggleApiKeyVisibility, // NEW
}
```

`handle_action` gets a matching arm calling a new `handle_toggle_api_key_visibility(&mut self, ctx)`.

### Render Flow

- `ApiKeyInputWidget::render` returns a `Flex::column` of (label, `Flex::row(chromed_input, toggle_button)`).
- `BaseUrlInputWidget::render` returns a `Flex::column` of (label, `chromed_input`). Visibility logic (`if !provider.needs_base_url()`) is unchanged.

### Edge Cases

- **Focus on toggle.** When the toggle swaps password mode, focus stays on the API Key input. If the implementation recreates the editor, it must call `ctx.focus(&new_editor)` after `set_buffer_text`.
- **Provider change clears test result** (already implemented at line 234) — unchanged.
- **Whitespace in pasted keys** — not addressed; same behavior as today.
- **Buffer persistence across toggles** — guaranteed by reading `buffer_text` before recreating the editor (if that path is taken).

## Testing

- **Manual smoke test (required):**
  1. Open Settings → Agents → Direct API.
  2. Verify API Key field has a visible bordered box. Type — characters appear masked.
  3. Click eye icon. Characters become visible. Click again, re-masked.
  4. Select each provider; verify the placeholder text changes and the Base URL field appears/disappears appropriately.
  5. Select Custom — Base URL field is empty with `https://api.example.com/v1` placeholder.
  6. Paste a key, click Save to Keychain — confirm the existing "saved" status message appears.

- **Unit tests (light):** Extend `direct_api_page_tests.rs` (or create it) with one test covering `handle_select_provider`'s placeholder-update logic for each `ProviderType`. Skip UI snapshot testing — not worth the harness setup for a contained fix.

## Out of Scope / Follow-ups

- Live API validation (calling the provider's `/models` endpoint).
- Inline-inside-input eye icon (vs. side-by-side button).
- Update Model List functionality (currently a placeholder action).
- Visual loading spinner during Test Connection.
- Async test runs without blocking the UI thread.

## Files Touched

- `app/src/settings_view/direct_api_page.rs` — all changes here.
- `app/src/settings_view/direct_api_page_tests.rs` — new or extended (optional, single test).
