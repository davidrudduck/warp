# TECH.md — Refactor AgentInputFooter: Shared Model + Separate View Instances

## Context

`AgentInputFooter` (`app/src/ai/blocklist/agent_view/agent_input_footer/mod.rs`) is a ~2400-line view that serves as the control bar at the bottom of the agent input. It renders in two modes: **Agent View mode** (model selector, NLD toggle, display chips) and **CLI agent mode** (rich input toggle, file explorer, voice, plugin install chip, etc.).

A single `ViewHandle<AgentInputFooter>` is created by `Input` and then shared with `UseAgentToolbar` via `ViewHandle::clone()`:

- `app/src/terminal/input.rs:2226–2237` — `Input::new` creates the footer
- `app/src/terminal/view.rs:4058–4066` — `TerminalView::new` passes the same handle to `UseAgentToolbar`

Both parents subscribe to the same view's events. WarpUI delivers events to all subscribers of a view, so every chip click fires both the `Input` subscriber and the `UseAgentToolbar` subscriber. This caused the bug fixed in the parent branch: the `Input` subscriber was emitting `Event::Escape` for `OpenRichInput`/`HideRichInput`, which in cloud mode exited the agent view. Even with the bug fix (making `Input`'s handler a no-op for these events), the dual-subscriber pattern remains a foot-gun for future changes.

### Relevant state in `AgentInputFooter`

The view holds two categories of state:

**Shared state** (must be consistent across both render sites):
- `plugin_chip_ready: bool` — debounce timer for install chip visibility
- `plugin_operation_in_progress: bool` — install/update in flight
- `render_ftu_callout: bool` — first-time-use model callout
- `display_chip_config: DisplayChipConfig` — session context, repo path, etc.
- `left_display_chips`, `right_display_chips`, `cli_display_chips` — chip view handles
- Voice input state (`cli_voice_input_state`, `cli_transcription_handle`)

**View handles** (child views that could be created per-instance):
- ~15 `ActionButton` handles (nld, mic, file, rich input, settings, plugin install/update/dismiss, fast forward, handoff, file explorer, remote control start/stop, context window, ftu close)
- `model_selector: ViewHandle<ProfileModelSelector>`
- `environment_selector: Option<ViewHandle<EnvironmentSelector>>`
- `prompt_alert: ViewHandle<PromptAlertView>`
- `v2_model_selector: Option<ViewHandle<ModelSelector>>`

### Model subscriptions in `AgentInputFooter::new`

The constructor sets up ~12 model subscriptions (CLIAgentSessions, AISettings, SessionSettings, AIExecutionProfiles, BlocklistAIHistory, NetworkStatus, UserWorkspaces, AIRequestUsage, AuthManager, DisplayChipConfig.model_events, PromptType, and AmbientAgentViewModel). These drive state updates that must be visible to both render sites.

## Proposed changes

Extract shared state into `AgentInputFooterModel` (a WarpUI `Entity` with events). Each parent creates its own `AgentInputFooter` view instance backed by the same model. Events from each view flow only to the owning parent's subscriber.

### 1. Create `AgentInputFooterModel`

New file: `app/src/ai/blocklist/agent_view/agent_input_footer/model.rs`

```rust
pub struct AgentInputFooterModel {
    terminal_view_id: EntityId,
    plugin_chip_ready: bool,
    plugin_operation_in_progress: bool,
    render_ftu_callout: bool,
    display_chip_config: DisplayChipConfig,
    #[cfg(feature = "voice_input")]
    cli_voice_input_state: CLIVoiceInputState,
    #[cfg(feature = "voice_input")]
    cli_transcription_handle: Option<SpawnedFutureHandle>,
}
```

The model holds all shared state and owns the ~12 singleton model subscriptions that currently live in `AgentInputFooter::new`. It emits events (`AgentInputFooterModelEvent`) when shared state changes so view instances can re-render.

The model is created once per terminal view in `TerminalView::new` (alongside the existing `CLIAgentSessionsModel`, `AIInputModel`, etc.) and passed to both the `Input`-owned and `UseAgentToolbar`-owned footer views.

### 2. Make `AgentInputFooter` per-instance

Each `AgentInputFooter` view instance:
- Takes a `ModelHandle<AgentInputFooterModel>` instead of owning shared state directly
- Creates its own `ActionButton` handles (these are cheap; buttons are stateless renderers)
- Subscribes to `AgentInputFooterModel` for re-render notifications
- Emits `AgentInputFooterEvent` only to its owning parent's subscriber

Construction changes:
- `Input::new` creates its own `AgentInputFooter` (as it does today)
- `UseAgentToolbar::new` creates a *separate* `AgentInputFooter` instead of receiving a cloned handle
- Both pass the same `ModelHandle<AgentInputFooterModel>`

### 3. Wire up in `TerminalView::new`

```
let agent_input_footer_model = ctx.add_model(|ctx| {
    AgentInputFooterModel::new(terminal_view_id, display_chip_config, ctx)
});
// Input creates its own footer view internally, using agent_input_footer_model
// UseAgentToolbar creates its own footer view internally, using agent_input_footer_model
```

The `UseAgentToolbar::new` signature changes from taking `ViewHandle<AgentInputFooter>` to taking `ModelHandle<AgentInputFooterModel>`.

### 4. Remove dual-subscriber no-op

With separate view instances, the `Input` subscriber's no-op arms for `OpenRichInput`/`HideRichInput`/`WriteToPty`/etc. are no longer needed — those events only fire from the `UseAgentToolbar`'s footer instance. The `Input`'s footer instance emits events that only `Input` handles.

### Migration strategy

This is a large refactor (~2400 lines touched). Recommended sequencing:

1. **PR 1**: Extract `AgentInputFooterModel` with the shared state fields and model subscriptions. `AgentInputFooter` reads from the model instead of owning the state. Still one shared view instance — validates the model extraction compiles and behaves correctly.

2. **PR 2**: Create separate `AgentInputFooter` instances per parent. Update `UseAgentToolbar::new` to take the model handle and construct its own footer view. Remove the no-op event arms from `Input`'s subscriber.

## Testing and validation

- **Existing behavior**: The rich input chip in both terminal mode (non-cloud) and cloud mode with a 3p agent (Codex, Claude Code) must open/close correctly without exiting cloud mode. This was the original bug; the fix is on the parent branch and this refactor must not regress it.
- **Plugin chip**: Install/update/dismiss plugin chip state must stay consistent — when the debounce fires or a listener connects in one render context, the chip should appear/disappear correctly in whichever context is currently rendering it.
- **Voice input**: CLI voice input start/stop/transcribe must work from the footer regardless of which parent is currently rendering it.
- **Display chips**: Chip menus (directory, git stats, etc.) must open and close correctly. `has_open_chip_menu()` must reflect the state of whichever footer instance is currently visible.
- **Model/environment selectors**: Both V1 and V2 model selectors and the environment selector must open, close, and propagate selections correctly.
- **Manual test**: Run `claude` in terminal mode → click Rich Input chip to show/hide → verify works. Enter cloud mode with Codex → click Rich Input chip → verify opens/closes without exiting cloud mode.

## Risks and mitigations

- **Button identity**: Some external code may read button state through the shared `ViewHandle<AgentInputFooter>` (e.g. `agent_input_footer.as_ref(ctx).has_open_chip_menu(ctx)`). After the split, callers need to know which instance to query. Mitigate by putting query methods on the model (e.g. `AgentInputFooterModel::has_open_chip_menu`) instead of the view.
- **Render divergence**: Two view instances could theoretically render different chip sets if they get out of sync. Mitigate by having the model own the chip selection logic and emit events; views just render what the model says.
- **Size of change**: PR 1 (model extraction) is the riskiest part. Keep it mechanical — move fields and subscriptions without changing behavior.
