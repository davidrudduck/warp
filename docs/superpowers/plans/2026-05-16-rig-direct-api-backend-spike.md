# Rig Direct API Backend Spike Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an experimental, UI-gated Rig.rs backend for Direct API Agent Profile routing, prove whether it can safely handle Warp's local user -> assistant -> tool -> result -> assistant loop, and keep the existing native Direct API path as the default fallback.

**Architecture:** Rig is introduced behind a compile-time feature and a runtime settings gate. The first implementation is a spike adapter, not a replacement: it converts Warp Direct API request state into Rig-compatible messages, converts Rig stream items back into Warp provider-neutral events, and stops before internal Rig tool execution so Warp remains the owner of permissions, confirmations, action execution, UI state, and persistence. Promotion from experimental to default is blocked until the adapter proves deferred tool execution, streaming parity, cancellation, redaction, and provider coverage.

**Tech Stack:** Rust 2018/2021 workspace, `rig-core = 0.37.0`, existing `genai = 0.6.0-beta.19`, WarpUI settings/profile views, `DirectAPISettings` in `~/.warp-oss/settings.toml`, `warp_multi_agent_api::ResponseEvent`, Tokio/futures streams, existing Warp action execution models.

---

## Research Summary

### Rig Fit

Rig is the best Rust-native candidate to spike. Its public README describes multi-turn streaming and prompting, 20+ model providers, vector store integrations, completion/embedding workflows, reasoning, and minimal-boilerplate app integration. It is MIT licensed, current on crates.io as `rig-core = 0.37.0`, and its own repo lists terminal/coding-agent-adjacent users such as VT Code, Con, ChatShell, nitpicker, and deepwiki-rs.

Rig also exposes hooks and streaming items around tool calls. The important detail for Warp is that Rig's default agent loop wants to call registered tools through Rig's `ToolServer`. Warp cannot let that become the main execution path. Warp must intercept tool calls, emit them into existing `ResponseEvent`/action models, wait for Warp action results, and resume the model with tool results on the next turn.

### Version Pin Recommendation

Use `rig-core = "=0.37.0"` for the first spike and commit the resulting `Cargo.lock`. Current crates.io metadata reports `rig-core = 0.37.0` as the latest available version. A local clone of the RustSec advisory database found no advisory entries matching `rig-core`, `0xPlaygrounds/rig`, or the Rig repository URL at the time this plan was written. This is not a security guarantee; it only means no known RustSec advisory was found in that database snapshot.

Do not use a git dependency for the spike. Do not use the default caret requirement (`"0.37.0"`) if the goal is to avoid upstream patch churn while implementing and testing. Revisit the pin only after the backend passes the decision gate.

### What People Like

- Rust-native type safety and async/concurrency fit.
- Provider abstraction without Python or Node runtime.
- Agent workflows, streaming, reasoning, structured outputs, and MCP support are moving quickly.
- Public ecosystem includes terminal/coding-agent-like projects, not only toy chatbots.
- Community comments around agent frameworks repeatedly value lower boilerplate and better production observability; Rig's tracing and Rust-first design directly target some of that.

### What People Dislike Or Worry About

- Rig is moving quickly and has visible breaking changes in release discussions. That is acceptable for a spike, not acceptable for an un-gated core path.
- Agent-framework sentiment is skeptical: people report easy demos but hard real-world debugging, custom handlers, state/concurrency problems, and framework lock-in.
- Some Rust community sentiment is broadly hostile to AI project announcements, so popularity signals are noisy.
- Rig's tool abstractions may fight Warp's existing action executor because Warp tools are not simple async functions; they are UI/model-bound flows with confirmation, cancellation, risk policy, and persistence.
- Rig may improve provider abstraction but not solve the real missing piece: a deferred-tool local agent loop that speaks Warp's stream contract.

### Similar Projects

No source found shows an exact `warp-oss` equivalent using Rig. The closest public examples are:

- VT Code: a Rust terminal coding agent using Rig for LLM calls/model picker.
- Con: terminal emulator with built-in AI agent harness using Rig as provider abstraction.
- ChatShell: desktop AI client on rig-core/Tauri with tools, providers, MCP, and skills.
- nitpicker: code review CLI with parallel rig-core agents.
- deepwiki-rs/Litho: codebase documentation/context generation.

These are close enough to justify a spike, but not enough to justify assuming Rig fits Warp's UI/action model.

### Alternatives Discussed In Rust AI Work

- Keep extending existing `genai`: lowest dependency churn; still requires building the missing agent loop.
- `rig-core`: best Rust-native spike candidate for provider abstraction plus agent loop.
- `rig-compose`: possible future orchestration layer; too new for this first integration.
- Swiftide: strong for RAG/data pipelines; less directly suited to Warp's terminal-agent loop.
- Kalosm: local/model framework direction; not a drop-in Direct API route.
- `llm-chain`, `async-openai`, direct provider clients: useful primitives, not a complete agent runtime.
- Full Rust agent projects such as Moltis/RayClaw: useful architectural references, too large to embed as SDKs.
- Python/TS frameworks such as Pydantic AI, OpenAI Agents SDK, LangGraph, Vercel AI SDK: good conceptual references, poor fit for a native Rust desktop app unless we accept sidecar runtime complexity.

## Source References

- Rig README: https://github.com/0xPlaygrounds/rig
- Rig ecosystem: https://github.com/0xPlaygrounds/rig/blob/main/ECOSYSTEM.md
- Rig 0.16 release discussion with breaking changes, reasoning, MCP, usage, and tool-call fixes: https://github.com/0xPlaygrounds/rig/discussions/635
- Rig current crate metadata: https://crates.io/crates/rig-core
- Rig docs: https://docs.rs/rig-core/latest/rig/
- Reddit release thread for Rig v0.31: https://www.reddit.com/r/rust/comments/1r79hxd/rig_v031_released/
- General agent-framework sentiment thread: https://www.reddit.com/r/AI_Agents/comments/1mm73hz/do_you_find_agent_frameworks_like_langchain_crew/
- Moltis Rust personal agent architecture reference: https://github.com/moltis-org/moltis
- RayClaw Rust agent runtime reference: https://github.com/rayclaw/rayclaw
- Existing Warp Direct API parity plan: `docs/superpowers/plans/2026-05-16-direct-api-agent-profile-adapter-parity.md`

## Decision Gate

Do not promote Rig beyond experimental unless all of these are true:

- Rig can be driven without letting Rig execute shell/file/MCP tools directly.
- Rig can stream text, reasoning, and tool-call deltas in display order.
- Tool-call IDs survive provider -> Rig -> Warp -> provider round trips.
- Warp can cancel a Rig run without stale UI updates or background tool execution.
- Rig supports OpenAI, Anthropic, Gemini, Ollama, OpenRouter, and Custom OpenAI-compatible routes needed by current Direct API settings, or the UI clearly limits unsupported providers.
- Redacted diagnostics prove no prompts, API keys, command output, file contents, or bearer tokens are logged by default.
- `cargo check -p warp --bin warp-oss` passes with the Rig feature enabled and disabled.

## UX Gate

Rig must be explicitly gated in two layers:

1. A Direct API settings toggle:

```toml
[agents.direct_api.experimental]
rig_backend_enabled = false
```

2. A Direct API profile selector shown only when the toggle is enabled:

```text
Agent engine: [ Native ] [ Rig Agent ]
```

Default is always `Native`. The profile selector is visible only when:

- the profile's model routing is `DirectApi`
- `rig_backend_enabled` is true
- the app was built with the Rig backend cargo feature

UI copy must stay terse:

```text
Rig Agent
Uses Rig for provider streaming. Warp still handles tools and permissions.
```

Do not put a marketing explanation in the app.

## File Structure

- Modify `Cargo.toml`
  - Add workspace dependency `rig-core`.
- Modify `crates/ai/Cargo.toml`
  - Add optional dependency `rig-core`.
  - Add feature `rig_backend = ["dep:rig-core"]`.
- Modify `app/Cargo.toml`
  - Add feature `direct_api_rig_backend = ["ai/rig_backend"]`.
- Modify `crates/settings/src/direct_api.rs`
  - Add `rig_backend_enabled` setting under `agents.direct_api.experimental`.
- Modify `crates/ai/src/provider/mod.rs`
  - Export Rig backend modules behind `#[cfg(feature = "rig_backend")]`.
- Create `crates/ai/src/provider/rig_backend.rs`
  - Own Rig provider/client construction and Rig event conversion.
- Create `crates/ai/src/provider/rig_backend_tests.rs`
  - Test provider selection, stream conversion, and tool-call interception using fakes.
- Modify `app/src/ai/execution_profiles/mod.rs`
  - Add `DirectApiAgentBackend` enum and profile field.
- Modify `app/src/ai/execution_profiles/profiles.rs`
  - Add setter for Direct API backend.
- Modify `app/src/ai/execution_profiles/profiles_tests.rs`
  - Test default/native behavior and Rig field persistence.
- Modify `app/src/ai/execution_profiles/editor/mod.rs`
  - Add backend selector action.
- Modify `app/src/ai/execution_profiles/editor/ui_helpers.rs`
  - Render backend selector only when the gate is enabled and profile route is Direct API.
- Modify `app/src/settings_view/direct_api_page.rs`
  - Add experimental Rig backend toggle.
- Modify `app/src/settings_view/direct_api_page_tests.rs`
  - Test toggle persistence and gated rendering.
- Modify `app/src/ai/agent/api.rs`
  - Add resolved backend field to `RequestParams`.
- Modify `app/src/ai/agent/api/direct_tools.rs`
  - Let `run_provider_stream` dispatch to native or Rig backend.
- Modify `app/src/ai/agent/api/direct.rs`
  - Keep validation and response stream setup; route local stream through selected backend.
- Create `app/src/ai/agent/api/rig_direct.rs`
  - Bridge Rig stream output into existing Direct API `ResponseEvent` actions, without Rig-owned tool execution.
- Modify `app/src/ai/agent/api/impl_tests.rs`
  - Add routing and parity tests.
- Modify `docs/features/direct-api-profile-routing.md`
  - Document experimental backend gate and fallback.

## Task 1: Add Cargo Feature Gate

**Files:**
- Modify `Cargo.toml`
- Modify `crates/ai/Cargo.toml`
- Modify `app/Cargo.toml`

- [x] **Step 1: Add workspace dependency**

In root `Cargo.toml`, add near workspace dependencies:

```toml
rig-core = { version = "=0.37.0", default-features = false, features = ["derive", "reqwest", "rustls"] }
```

- [x] **Step 2: Add optional AI dependency and feature**

In `crates/ai/Cargo.toml`, add:

```toml
rig_backend = ["dep:rig-core"]
```

Under dependencies add:

```toml
rig-core = { workspace = true, optional = true }
```

- [x] **Step 3: Add app feature**

In `app/Cargo.toml`, add:

```toml
direct_api_rig_backend = ["ai/rig_backend"]
```

Do not add it to `default`.

- [x] **Step 4: Verify both compile modes**

Run:

```bash
cargo check -p ai
cargo check -p ai --features rig_backend
cargo check -p warp --bin warp-oss
cargo check -p warp --bin warp-oss --features direct_api_rig_backend
```

Expected: all commands compile. If Rig introduces dependency conflicts, stop and record the blocker before adding UI.

- [x] **Step 5: Commit**

```bash
git add Cargo.toml crates/ai/Cargo.toml app/Cargo.toml
git commit -m "Add experimental Rig backend feature"
```

## Task 2: Add Runtime Settings Gate

**Files:**
- Modify `crates/settings/src/direct_api.rs`
- Modify settings tests if a direct settings test module exists; otherwise add `crates/settings/src/direct_api_tests.rs`

- [x] **Step 1: Add failing TOML-backed test**

Create or extend settings tests:

```rust
#[test]
fn direct_api_rig_backend_gate_defaults_off() {
    let prefs = InMemoryPreferences::default();
    assert_eq!(DirectAPIRigBackendEnabled::get(&prefs), false);
}

#[test]
fn direct_api_rig_backend_gate_writes_to_expected_toml_path() {
    let temp = tempfile::tempdir().unwrap();
    let prefs = TomlBackedUserPreferences::new(temp.path().join("settings.toml")).unwrap();

    DirectAPIRigBackendEnabled::set(&prefs, true).unwrap();

    let contents = std::fs::read_to_string(temp.path().join("settings.toml")).unwrap();
    assert!(contents.contains("[agents.direct_api.experimental]"));
    assert!(contents.contains("rig_backend_enabled = true"));
}
```

Use the actual helper names from existing settings tests if they differ.

- [x] **Step 2: Run failing tests**

Run:

```bash
cargo test -p settings direct_api_rig_backend_gate -- --nocapture
```

Expected: compile failure because the setting does not exist.

- [x] **Step 3: Add setting**

In `crates/settings/src/direct_api.rs`, add:

```rust
    rig_backend_enabled: DirectAPIRigBackendEnabled {
        type: bool,
        default: false,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Never,
        private: false,
        toml_path: "agents.direct_api.experimental.rig_backend_enabled",
        description: "Enable the Rig Agent backend for Direct API agent profiles",
    },
```

- [x] **Step 4: Run tests**

Run:

```bash
cargo test -p settings direct_api_rig_backend_gate -- --nocapture
```

Expected: tests pass.

- [x] **Step 5: Commit**

```bash
git add crates/settings/src/direct_api.rs crates/settings/src/*direct_api*tests.rs
git commit -m "Add Direct API Rig backend gate setting"
```

## Task 3: Add Per-Profile Backend Field

**Files:**
- Modify `app/src/ai/execution_profiles/mod.rs`
- Modify `app/src/ai/execution_profiles/profiles.rs`
- Modify `app/src/ai/execution_profiles/profiles_tests.rs`

- [x] **Step 1: Add failing profile tests**

Add:

```rust
#[test]
fn execution_profile_defaults_direct_api_backend_to_native() {
    let profile = AIExecutionProfile::default();
    assert_eq!(profile.direct_api_agent_backend, DirectApiAgentBackend::Native);
}

#[test]
fn execution_profile_roundtrips_rig_backend() {
    let profile = AIExecutionProfile {
        model_routing: ModelRouting::DirectApi,
        direct_api_agent_backend: DirectApiAgentBackend::RigExperimental,
        ..AIExecutionProfile::default()
    };

    let serialized = serde_json::to_string(&profile).unwrap();
    let decoded: AIExecutionProfile = serde_json::from_str(&serialized).unwrap();

    assert_eq!(decoded.direct_api_agent_backend, DirectApiAgentBackend::RigExperimental);
}
```

- [x] **Step 2: Run failing tests**

Run:

```bash
cargo test -p warp execution_profile_ -- --nocapture
```

Expected: compile failure because `DirectApiAgentBackend` does not exist.

- [x] **Step 3: Add enum and field**

In `app/src/ai/execution_profiles/mod.rs`, add:

```rust
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DirectApiAgentBackend {
    #[default]
    Native,
    RigExperimental,
    #[serde(other)]
    Unknown,
}

impl DirectApiAgentBackend {
    pub fn effective(self) -> Self {
        match self {
            Self::Native | Self::Unknown => Self::Native,
            Self::RigExperimental => Self::RigExperimental,
        }
    }
}
```

Add field after `direct_api_model`:

```rust
pub direct_api_agent_backend: DirectApiAgentBackend,
```

Set default:

```rust
direct_api_agent_backend: DirectApiAgentBackend::Native,
```

- [x] **Step 4: Add setter**

In `app/src/ai/execution_profiles/profiles.rs`, add:

```rust
pub fn set_direct_api_agent_backend(
    &mut self,
    profile_id: ClientProfileId,
    backend: DirectApiAgentBackend,
    ctx: &mut ModelContext<Self>,
) {
    self.update_profile(profile_id, ctx, |profile| {
        if profile.direct_api_agent_backend != backend {
            profile.direct_api_agent_backend = backend;
            true
        } else {
            false
        }
    });
}
```

If `update_profile` has a different local signature, adapt to the existing setter pattern in the same file.

- [x] **Step 5: Run tests**

Run:

```bash
cargo test -p warp execution_profile_ -- --nocapture
```

Expected: tests pass.

- [x] **Step 6: Commit**

```bash
git add app/src/ai/execution_profiles/mod.rs app/src/ai/execution_profiles/profiles.rs app/src/ai/execution_profiles/profiles_tests.rs
git commit -m "Add Direct API profile backend selection"
```

## Task 4: Add Direct API Settings Toggle UI

**Files:**
- Modify `app/src/settings_view/direct_api_page.rs`
- Modify `app/src/settings_view/direct_api_page_tests.rs`

- [x] **Step 1: Add view tests**

Add tests:

```rust
#[test]
fn rig_backend_toggle_defaults_off() {
    App::test(|ctx| {
        let view = ctx.add_window(WindowStyle::NotStealFocus, DirectApiSettingsPageView::new);
        view.update(ctx, |view, ctx| {
            assert!(!view.rig_backend_enabled(ctx));
        });
    });
}

#[test]
fn rig_backend_toggle_persists_setting() {
    App::test(|ctx| {
        let view = ctx.add_window(WindowStyle::NotStealFocus, DirectApiSettingsPageView::new);
        view.update(ctx, |view, ctx| {
            view.set_rig_backend_enabled(true, ctx);
            assert!(view.rig_backend_enabled(ctx));
        });
    });
}
```

Use the existing `App::test`/settings test helpers already used in `direct_api_page_tests.rs`.

- [x] **Step 2: Run failing tests**

Run:

```bash
cargo test -p warp direct_api_page::tests::rig_backend -- --nocapture
```

Expected: compile failure because helper methods do not exist.

- [x] **Step 3: Add action and helpers**

In `DirectApiPageAction`, add:

```rust
ToggleRigBackendEnabled(bool),
```

In `DirectApiSettingsPageView`, add helpers:

```rust
fn rig_backend_enabled(&self, ctx: &AppContext) -> bool {
    settings::DirectAPIRigBackendEnabled::get(UserPreferences::as_ref(ctx))
}

fn set_rig_backend_enabled(&self, enabled: bool, ctx: &mut ViewContext<Self>) {
    report_if_error!(settings::DirectAPIRigBackendEnabled::set(
        UserPreferences::as_ref(ctx),
        enabled,
    ));
}
```

- [x] **Step 4: Render toggle**

Add a dense settings row in the Direct API page:

```text
Rig Agent backend    [toggle]
```

Description text:

```text
Uses Rig for provider streaming. Warp still handles tools and permissions.
```

Use existing toggle/settings-row patterns in `settings_page.rs`. Do not introduce new button themes or custom colors.

- [x] **Step 5: Run tests**

Run:

```bash
cargo test -p warp direct_api_page -- --nocapture
```

Expected: tests pass.

- [x] **Step 6: Commit**

```bash
git add app/src/settings_view/direct_api_page.rs app/src/settings_view/direct_api_page_tests.rs
git commit -m "Gate Rig Direct API backend in settings"
```

## Task 5: Add Profile Editor Backend Selector

**Files:**
- Modify `app/src/ai/execution_profiles/editor/mod.rs`
- Modify `app/src/ai/execution_profiles/editor/ui_helpers.rs`
- Modify profile editor tests if present

- [x] **Step 1: Add behavior tests**

Add tests:

```rust
#[test]
fn profile_editor_hides_rig_backend_selector_when_gate_disabled() {
    let profile = direct_api_profile_with_backend(DirectApiAgentBackend::Native);
    let state = render_profile_editor_state(profile, false);
    assert!(!state.has_agent_backend_selector);
}

#[test]
fn profile_editor_shows_rig_backend_selector_for_direct_api_when_gate_enabled() {
    let profile = direct_api_profile_with_backend(DirectApiAgentBackend::Native);
    let state = render_profile_editor_state(profile, true);
    assert!(state.has_agent_backend_selector);
    assert_eq!(state.agent_backend_options, vec!["Native", "Rig Agent"]);
}
```

If no pure render-state helpers exist, add a small helper in tests rather than screenshot testing.

- [x] **Step 2: Run failing tests**

Run:

```bash
cargo test -p warp execution_profile_editor -- --nocapture
```

Expected: failure until selector state exists.

- [x] **Step 3: Add editor action**

In the editor action enum, add:

```rust
SetDirectApiAgentBackend {
    backend: DirectApiAgentBackend,
}
```

Handle it by calling:

```rust
profiles_model.set_direct_api_agent_backend(self.profile_id, *backend, ctx);
```

- [x] **Step 4: Render selector**

In `ui_helpers.rs`, render after the Direct API model picker:

```text
Agent engine: [ Native ] [ Rig Agent ]
```

Rules:

- Hide entirely for `WarpProvider`.
- Hide when settings gate is false.
- Disable `Rig Agent` if cargo feature is absent.
- If a profile stores `RigExperimental` but the gate or cargo feature is disabled, effective runtime backend is `Native` and the UI shows a small disabled-state label when visible.

- [x] **Step 5: Run tests**

Run:

```bash
cargo test -p warp execution_profile_editor -- --nocapture
cargo test -p warp execution_profile_ -- --nocapture
```

Expected: tests pass.

- [x] **Step 6: Commit**

```bash
git add app/src/ai/execution_profiles/editor/mod.rs app/src/ai/execution_profiles/editor/ui_helpers.rs app/src/ai/execution_profiles/*tests.rs
git commit -m "Add Direct API agent backend selector"
```

## Task 6: Resolve Backend Into RequestParams

**Files:**
- Modify `app/src/ai/agent/api.rs`
- Modify `app/src/ai/agent/api/impl_tests.rs`

- [x] **Step 1: Add routing tests**

Add:

```rust
#[test]
fn request_params_use_native_backend_when_rig_gate_disabled() {
    let params = request_params_for_direct_api_profile(
        DirectApiAgentBackend::RigExperimental,
        false,
    );

    assert_eq!(params.direct_api_agent_backend, DirectApiAgentBackend::Native);
}

#[test]
fn request_params_use_rig_backend_when_profile_and_gate_enable_it() {
    let params = request_params_for_direct_api_profile(
        DirectApiAgentBackend::RigExperimental,
        true,
    );

    assert_eq!(
        params.direct_api_agent_backend,
        DirectApiAgentBackend::RigExperimental
    );
}
```

- [x] **Step 2: Run failing tests**

Run:

```bash
cargo test -p warp request_params_use_rig_backend -- --nocapture
```

Expected: compile failure because `RequestParams` does not carry backend state.

- [x] **Step 3: Add field**

In `RequestParams`, add:

```rust
pub direct_api_agent_backend: DirectApiAgentBackend,
```

When building params:

```rust
let rig_gate_enabled = settings::DirectAPIRigBackendEnabled::get(UserPreferences::as_ref(app));
let direct_api_agent_backend = if requested_model_routing.is_direct_api() && rig_gate_enabled {
    profile_data.direct_api_agent_backend.effective()
} else {
    DirectApiAgentBackend::Native
};
```

If `cfg!(not(feature = "direct_api_rig_backend"))`, force `Native`.

- [x] **Step 4: Run tests**

Run:

```bash
cargo test -p warp request_params_use_rig_backend -- --nocapture
cargo test -p warp direct_api --lib -- --nocapture
```

Expected: tests pass.

- [x] **Step 5: Commit**

```bash
git add app/src/ai/agent/api.rs app/src/ai/agent/api/impl_tests.rs
git commit -m "Resolve Direct API backend into request params"
```

## Task 7: Build Rig Backend Adapter Spike

**Files:**
- Create `crates/ai/src/provider/rig_backend.rs`
- Create `crates/ai/src/provider/rig_backend_tests.rs`
- Modify `crates/ai/src/provider/mod.rs`

- [x] **Step 1: Add adapter tests**

Create `crates/ai/src/provider/rig_backend_tests.rs`:

```rust
use super::rig_backend::{RigBackendConfig, RigProviderKind};

#[test]
fn rig_backend_config_maps_openrouter() {
    let config = RigBackendConfig::new(
        RigProviderKind::OpenRouter,
        "moonshotai/kimi-k2.6",
        Some("test-key".to_string()),
        Some("https://openrouter.ai/api/v1".to_string()),
    );

    assert_eq!(config.provider_kind, RigProviderKind::OpenRouter);
    assert_eq!(config.model_id, "moonshotai/kimi-k2.6");
}

#[test]
fn rig_backend_config_rejects_missing_key_for_openrouter() {
    let err = RigBackendConfig::new(
        RigProviderKind::OpenRouter,
        "moonshotai/kimi-k2.6",
        None,
        Some("https://openrouter.ai/api/v1".to_string()),
    )
    .validate()
    .unwrap_err();

    assert!(err.to_string().contains("requires an API key"));
}
```

- [x] **Step 2: Run failing tests**

Run:

```bash
cargo test -p ai --features rig_backend rig_backend -- --nocapture
```

Expected: compile failure because `rig_backend` does not exist.

- [x] **Step 3: Add minimal config module**

Create `crates/ai/src/provider/rig_backend.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RigProviderKind {
    OpenAI,
    Anthropic,
    GoogleGemini,
    Ollama,
    OpenRouter,
    CustomOpenAICompatible,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RigBackendConfig {
    pub provider_kind: RigProviderKind,
    pub model_id: String,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
}

impl RigBackendConfig {
    pub fn new(
        provider_kind: RigProviderKind,
        model_id: impl Into<String>,
        api_key: Option<String>,
        base_url: Option<String>,
    ) -> Self {
        Self {
            provider_kind,
            model_id: model_id.into(),
            api_key,
            base_url,
        }
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        if self.model_id.trim().is_empty() {
            anyhow::bail!("Rig Direct API backend requires a model");
        }
        match self.provider_kind {
            RigProviderKind::Ollama => Ok(()),
            RigProviderKind::OpenAI
            | RigProviderKind::Anthropic
            | RigProviderKind::GoogleGemini
            | RigProviderKind::OpenRouter => {
                if self.api_key.as_deref().is_none_or(|key| key.trim().is_empty()) {
                    anyhow::bail!("Rig Direct API backend requires an API key");
                }
                Ok(())
            }
            RigProviderKind::CustomOpenAICompatible => {
                if self.base_url.as_deref().is_none_or(|url| url.trim().is_empty()) {
                    anyhow::bail!("Rig Direct API backend requires a base URL");
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
#[path = "rig_backend_tests.rs"]
mod tests;
```

In `mod.rs`, add:

```rust
#[cfg(feature = "rig_backend")]
pub mod rig_backend;
```

- [x] **Step 4: Run tests**

Run:

```bash
cargo test -p ai --features rig_backend rig_backend -- --nocapture
```

Expected: tests pass.

- [x] **Step 5: Commit**

```bash
git add crates/ai/src/provider/mod.rs crates/ai/src/provider/rig_backend.rs crates/ai/src/provider/rig_backend_tests.rs
git commit -m "Add Rig backend config adapter"
```

## Task 8: Prove Deferred Tool Execution

**Files:**
- Modify `crates/ai/src/provider/rig_backend.rs`
- Modify `crates/ai/src/provider/rig_backend_tests.rs`

- [ ] **Step 1: Add failing deferred-tool tests**

Add tests:

```rust
#[tokio::test]
async fn rig_backend_emits_tool_call_without_executing_tool() {
    let mut backend = FakeRigBackend::new()
        .with_streamed_tool_call("call_read", "ReadFiles", r#"{"files":[{"name":"Cargo.toml"}]}"#);

    let events = backend.collect_events_until_tool_call().await.unwrap();

    assert!(events.iter().any(|event| matches!(
        event,
        RigBackendEvent::ToolCallReady(call)
            if call.id == "call_read" && call.name == "ReadFiles"
    )));
    assert_eq!(backend.executed_tool_count(), 0);
}

#[tokio::test]
async fn rig_backend_can_resume_after_external_tool_result() {
    let mut backend = FakeRigBackend::new()
        .with_streamed_tool_call("call_read", "ReadFiles", r#"{"files":[{"name":"Cargo.toml"}]}"#)
        .with_final_text_after_tool_result("The package is warp.");

    let first = backend.collect_events_until_tool_call().await.unwrap();
    assert!(first.iter().any(|event| matches!(event, RigBackendEvent::ToolCallReady(_))));

    let second = backend
        .resume_with_tool_result("call_read", "Cargo.toml contents")
        .await
        .unwrap();

    assert!(second.iter().any(|event| matches!(
        event,
        RigBackendEvent::TextChunk(text) if text.contains("warp")
    )));
}
```

The `FakeRigBackend` can be an internal test-only fake over the adapter trait. The important proof is the adapter API shape, not live provider behavior.

- [ ] **Step 2: Run failing tests**

Run:

```bash
cargo test -p ai --features rig_backend rig_backend_emits_tool_call_without_executing_tool -- --nocapture
```

Expected: tests fail until deferred-tool event model exists.

- [ ] **Step 3: Add backend event model**

Add:

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum RigBackendEvent {
    Start,
    TextChunk(String),
    ReasoningChunk(String),
    ToolCallDelta {
        index: usize,
        id: String,
        name: String,
        args_fragment: String,
    },
    ToolCallReady(ToolCall),
    End {
        finish_reason: FinishReason,
        usage: Option<TokenUsage>,
    },
}
```

Map this to the existing `StreamEvent` later. Keep Rig-specific concerns out of the UI layer.

- [ ] **Step 4: Add spike adapter methods**

Add:

```rust
pub struct RigDirectBackend {
    config: RigBackendConfig,
}

impl RigDirectBackend {
    pub fn new(config: RigBackendConfig) -> anyhow::Result<Self> {
        config.validate()?;
        Ok(Self { config })
    }

    pub async fn stream_turn(
        &self,
        request: ChatRequest,
    ) -> anyhow::Result<ChatStream> {
        stream_turn_with_rig(self.config.clone(), request).await
    }
}
```

Implement `stream_turn_with_rig` with the smallest real Rig integration that compiles. If Rig cannot expose a stream before tool execution, stop and mark the spike failed.

- [ ] **Step 5: Run tests**

Run:

```bash
cargo test -p ai --features rig_backend rig_backend -- --nocapture
```

Expected: fake deferred-tool tests pass; real Rig integration compiles.

- [ ] **Step 6: Commit**

```bash
git add crates/ai/src/provider/rig_backend.rs crates/ai/src/provider/rig_backend_tests.rs
git commit -m "Spike deferred tool handling for Rig backend"
```

## Task 9: Bridge Rig Backend Into Direct API Route

**Files:**
- Modify `app/src/ai/agent/api/direct_tools.rs`
- Create `app/src/ai/agent/api/rig_direct.rs`
- Modify `app/src/ai/agent/api/mod.rs`
- Modify `app/src/ai/agent/api/direct.rs`
- Modify `app/src/ai/agent/api/impl_tests.rs`

- [ ] **Step 1: Add app routing tests**

Add:

```rust
#[test]
fn direct_api_rig_backend_uses_rig_stream_when_enabled() {
    let mut params = direct_api_request_params_for_openrouter();
    params.direct_api_agent_backend = DirectApiAgentBackend::RigExperimental;

    let backend = select_direct_api_stream_backend(&params);

    assert_eq!(backend, DirectApiStreamBackend::RigExperimental);
}

#[test]
fn direct_api_native_backend_remains_default() {
    let params = direct_api_request_params_for_openrouter();
    let backend = select_direct_api_stream_backend(&params);

    assert_eq!(backend, DirectApiStreamBackend::NativeGenai);
}
```

- [ ] **Step 2: Run failing tests**

Run:

```bash
cargo test -p warp --features direct_api_rig_backend direct_api_rig_backend_uses_rig_stream -- --nocapture
```

Expected: compile failure until selector exists.

- [ ] **Step 3: Add selector**

In `direct_tools.rs`, add:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectApiStreamBackend {
    NativeGenai,
    RigExperimental,
}

pub fn select_direct_api_stream_backend(params: &RequestParams) -> DirectApiStreamBackend {
    match params.direct_api_agent_backend.effective() {
        DirectApiAgentBackend::Native | DirectApiAgentBackend::Unknown => DirectApiStreamBackend::NativeGenai,
        DirectApiAgentBackend::RigExperimental => {
            #[cfg(feature = "direct_api_rig_backend")]
            {
                DirectApiStreamBackend::RigExperimental
            }
            #[cfg(not(feature = "direct_api_rig_backend"))]
            {
                DirectApiStreamBackend::NativeGenai
            }
        }
    }
}
```

- [ ] **Step 4: Dispatch backend**

Change `run_provider_stream`:

```rust
match select_direct_api_stream_backend(&params) {
    DirectApiStreamBackend::NativeGenai => run_native_provider_stream(params).await,
    DirectApiStreamBackend::RigExperimental => super::rig_direct::run_rig_provider_stream(params).await,
}
```

Keep the existing body in `run_native_provider_stream`.

- [ ] **Step 5: Implement `rig_direct.rs`**

Add:

```rust
#[cfg(feature = "direct_api_rig_backend")]
pub async fn run_rig_provider_stream(params: RequestParams) -> anyhow::Result<ChatStream> {
    let config = params
        .direct_api_route_config
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Direct API route config missing"))?;
    let request = super::direct_tools::build_chat_request(&params);
    let rig_config = rig_config_from_direct_api_config(config)?;
    ai::provider::rig_backend::RigDirectBackend::new(rig_config)?
        .stream_turn(request)
        .await
}

#[cfg(not(feature = "direct_api_rig_backend"))]
pub async fn run_rig_provider_stream(_params: RequestParams) -> anyhow::Result<ChatStream> {
    anyhow::bail!("Rig Direct API backend is not available in this build")
}
```

- [ ] **Step 6: Run tests**

Run:

```bash
cargo test -p warp --features direct_api_rig_backend direct_api_rig_backend -- --nocapture
cargo test -p warp direct_api --lib -- --nocapture
```

Expected: Rig feature tests pass and native Direct API tests remain green.

- [ ] **Step 7: Commit**

```bash
git add app/src/ai/agent/api/direct_tools.rs app/src/ai/agent/api/rig_direct.rs app/src/ai/agent/api/mod.rs app/src/ai/agent/api/direct.rs app/src/ai/agent/api/impl_tests.rs
git commit -m "Route Direct API through experimental Rig backend"
```

## Task 10: Add Rig Stream Parity Tests

**Files:**
- Modify `crates/ai/src/provider/rig_backend_tests.rs`
- Modify `app/src/ai/agent/api/impl_tests.rs`

- [ ] **Step 1: Add parity cases**

Cover these cases:

```rust
#[test]
fn rig_stream_preserves_text_order() {}

#[test]
fn rig_stream_preserves_reasoning_chunks() {}

#[test]
fn rig_stream_assembles_tool_arguments_from_deltas() {}

#[test]
fn rig_stream_preserves_tool_call_ids() {}

#[test]
fn rig_stream_maps_usage_on_end() {}

#[test]
fn rig_stream_empty_success_is_error() {}

#[test]
fn rig_stream_cancellation_stops_events() {}
```

Each test should assert exact event order and tool-call IDs.

- [ ] **Step 2: Run tests**

Run:

```bash
cargo test -p ai --features rig_backend rig_stream -- --nocapture
cargo test -p warp --features direct_api_rig_backend rig_stream -- --nocapture
```

Expected: all parity tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/ai/src/provider/rig_backend_tests.rs app/src/ai/agent/api/impl_tests.rs
git commit -m "Add Rig Direct API stream parity tests"
```

## Task 11: Add Redaction And Diagnostics

**Files:**
- Modify `crates/ai/src/logging/mod.rs`
- Modify `crates/ai/src/logging/logger_tests.rs`
- Modify `crates/ai/src/provider/rig_backend.rs`

- [ ] **Step 1: Add redaction tests**

Add:

```rust
#[test]
fn rig_backend_diagnostics_redact_api_keys_and_tool_args() {
    let event = RigDiagnosticEvent {
        provider: "OpenRouter".to_string(),
        model_id: "moonshotai/kimi-k2.6".to_string(),
        api_key: Some("sk-or-v1-secret".to_string()),
        tool_args: Some(r#"{"command":"cat ~/.ssh/id_rsa"}"#.to_string()),
    };

    let rendered = redact_rig_diagnostic_event(&event);

    assert!(!rendered.contains("sk-or-v1-secret"));
    assert!(!rendered.contains("id_rsa"));
    assert!(rendered.contains("<redacted>"));
}
```

- [ ] **Step 2: Add safe diagnostics**

Log only:

- backend name
- provider enum
- public model ID or hashed custom model ID
- event counts
- tool-call count
- finish reason
- error category

Do not log prompts, file contents, shell output, tool args, API keys, headers, or full custom URLs.

- [ ] **Step 3: Run tests**

Run:

```bash
cargo test -p ai --features rig_backend logging rig_backend -- --nocapture
```

Expected: tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/ai/src/logging/mod.rs crates/ai/src/logging/logger_tests.rs crates/ai/src/provider/rig_backend.rs
git commit -m "Redact Rig backend diagnostics"
```

## Task 12: Manual UI And Runtime Validation

**Files:**
- Modify `docs/features/direct-api-profile-routing.md`
- Modify `docs/QUICK-START.md`

- [ ] **Step 1: Document the setting**

Add:

```markdown
### Experimental Rig backend

The Direct API settings page can expose an experimental Rig backend. It is off by default:

```toml
[agents.direct_api.experimental]
rig_backend_enabled = false
```

When enabled, Direct API profiles show `Agent engine: Native / Rig Agent`.
Use `Native` unless testing Rig provider streaming.
```
```

- [ ] **Step 2: Validate settings UI**

Run:

```bash
cargo run -p warp --bin warp-oss --features direct_api_rig_backend
```

Manual checks:

- Settings -> Agents -> Direct API shows the Rig Agent toggle.
- Toggle state survives restart in `~/.warp-oss/settings.toml`.
- Direct API profile editor shows backend selector only when the toggle is on.
- `Native` remains selected by default.

- [ ] **Step 3: Validate native fallback**

Manual checks:

- Set profile to Direct API + Native.
- Prompt: `Say hello in one sentence.`
- Confirm output streams.
- Confirm logs show native Direct API backend, not Rig.

- [ ] **Step 4: Validate Rig text stream**

Manual checks:

- Set profile to Direct API + Rig Agent.
- Prompt: `Say hello in one sentence.`
- Confirm output streams.
- Confirm no empty success.
- Confirm no GraphQL task status warnings caused by local Direct API run.

- [ ] **Step 5: Validate Rig tool flow**

Manual checks:

- Prompt: `Read Cargo.toml and summarize the package name.`
- Confirm a tool call appears in Warp UI.
- Confirm Warp permission UI controls execution.
- Approve the read.
- Confirm the follow-up provider turn includes the tool result and produces final text.

- [ ] **Step 6: Run final validation**

Run:

```bash
cargo fmt --check
git diff --check
cargo test -p settings direct_api_rig_backend -- --nocapture
cargo test -p ai --features rig_backend rig_backend -- --nocapture
cargo test -p warp --features direct_api_rig_backend direct_api_rig_backend -- --nocapture
cargo test -p warp direct_api --lib -- --nocapture
cargo check -p warp --bin warp-oss
cargo check -p warp --bin warp-oss --features direct_api_rig_backend
```

Expected: all pass.

- [ ] **Step 7: Commit docs**

```bash
git add docs/features/direct-api-profile-routing.md docs/QUICK-START.md
git commit -m "Document experimental Rig Direct API backend"
```

## Adversarial Review

### Critical Issues To Prove Before Implementation

1. Rig may not solve the missing piece.
   - Existing Warp already has `genai` provider abstraction.
   - The missing piece is deferred tool execution and Warp-compatible stream lifecycle.
   - If Rig cannot defer tool execution cleanly, it should not be integrated past Task 8.

2. Rig's internal tool server is the wrong owner for Warp tools.
   - Warp tools require UI confirmation, profile permissions, command risk handling, model context, cancellation, and persistence.
   - Directly registering `RunShellCommand` or file tools in Rig would bypass core Warp safety behavior.

3. API churn is a real dependency risk.
   - Rig release notes show breaking changes in streaming usage, tool result serialization, client builders, MCP integrations, and vector APIs.
   - Pin exact versions and keep the backend behind a cargo feature.

4. Provider parity may regress.
   - Current settings include Custom OpenAI-compatible base URLs and OpenRouter defaults.
   - Rig's provider support may not exactly match Warp's existing base URL normalization and validation.
   - The plan must test every configured provider path before calling Rig "better."

5. Observability can leak sensitive local data.
   - Rig uses tracing spans around tool calls.
   - Warp must not log prompt content, tool args, command output, file contents, API keys, or custom internal URLs by default.

6. The UI gate can create false confidence.
   - "Rig Agent" must be paired with clear disabled/default behavior so it does not read as the recommended option before validation.
   - Native must remain the default until the decision gate passes.

7. Dependency cost may be high.
   - Compile time, binary size, transitive TLS/reqwest features, and duplicate provider code need measurement.
   - If the feature adds too much cost, keep it out of default builds.

8. Cancellation and stale stream updates are likely failure points.
   - Rig's stream task must obey Warp cancellation and generation checks.
   - No background tool calls can continue after user cancellation.

### Kill Criteria

Stop the Rig path and continue with native `genai` if any of these are true:

- Rig cannot emit tool calls before executing tools.
- Rig requires tool execution inside its own `ToolServer` for multi-turn continuation.
- Rig cannot support OpenRouter and Custom OpenAI-compatible base URLs without invasive patches.
- Rig stream events cannot preserve tool-call IDs or reasoning signatures for provider follow-up.
- Rig causes unacceptable binary size or compile-time regression for `warp-oss`.
- Redaction requires forking Rig internals.

### If The Spike Succeeds

If all gates pass, follow with a separate plan to:

- promote Rig backend from experimental to preview
- remove duplicate native provider code only where Rig proves parity
- keep Native backend available as a fallback for at least one release cycle
- add one integration test that exercises text, tool call, tool result, and final answer in the same conversation

## Self-Review

- Spec coverage: plan includes Rig research, sentiment, similar projects, alternatives, UI gating, compile/runtime gates, implementation steps, tests, docs, and adversarial review.
- Placeholder scan: no task depends on unspecified implementation without a stop condition; spike unknowns have explicit kill criteria.
- Type consistency: `DirectApiAgentBackend`, `RigBackendConfig`, `RigBackendEvent`, and `DirectApiStreamBackend` are introduced before downstream use.
- Local-first check: all Direct API settings remain under `~/.warp-oss/settings.toml`; Warp remains owner of tools and permissions.
