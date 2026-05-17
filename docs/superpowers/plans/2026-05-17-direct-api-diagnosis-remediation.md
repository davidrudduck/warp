# Direct API Diagnosis And Remediation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Diagnose and remediate Warp OSS Direct API provider UX, agent engine selection state, OpenRouter 401 failures, debug logging gaps, and macOS keychain prompts.

**Architecture:** Work in two passes. First gather redacted evidence at each boundary: settings file, profile selection, route construction, provider adapter, HTTP status, and macOS signing/keychain identity. Then implement narrowly scoped remediations with tests, preserving the local-only `warp-oss` model and `~/.warp-oss/settings.toml` separation from official Warp.

**Tech Stack:** Rust workspace, WarpUI settings/profile views, `DirectAPISettings`, `ApiKeyManager`, `genai`, optional `rig-core`, `reqwest`, macOS Keychain Services through `security-framework`, `env_logger`, `cargo test`, `cargo check -p warp --bin warp-oss`.

---

## Source References

- OpenRouter authentication requires `Authorization: Bearer <API key>`: https://openrouter.ai/docs/api/reference/authentication
- Apple Keychain generic password reads prompt when the calling app is not trusted for the item: https://developer.apple.com/documentation/security/seckeychainfindgenericpassword%28_%3A_%3A_%3A_%3A_%3A_%3A_%3A_%3A%29
- Apple code signing requirements explain why changing signing identity or channel can produce new authorization prompts: https://developer.apple.com/documentation/Technotes/tn3127-inside-code-signing-requirements
- Existing Direct API user guide notes Direct API-specific log files are not wired in production builds: `docs/features/direct-api-user-guide.md`
- Existing Direct API developer guide notes `DirectApiLogger` exists but routing uses normal Warp logging unless explicitly wired: `docs/features/direct-api-developer-guide.md`

## Code Map

- Direct API settings UI: `app/src/settings_view/direct_api_page.rs`
- Direct API settings UI tests: `app/src/settings_view/direct_api_page_tests.rs`
- Execution profile editor state and rendering: `app/src/ai/execution_profiles/editor/mod.rs`, `app/src/ai/execution_profiles/editor/ui_helpers.rs`
- Execution profile editor tests: `app/src/ai/execution_profiles/editor/mod_tests.rs`
- Direct API route construction: `app/src/ai/agent/api.rs`
- Direct API route validation and stream handoff: `app/src/ai/agent/api/direct.rs`, `app/src/ai/agent/api/direct_tools.rs`, `app/src/ai/agent/api/rig_direct.rs`
- Direct API request tests: `app/src/ai/agent/api/impl_tests.rs`
- Native provider adapter: `crates/ai/src/provider/genai_adapter.rs`
- Rig provider adapter: `crates/ai/src/provider/rig_backend.rs`
- OpenRouter model listing path: `crates/ai/src/model_registry/providers/openrouter.rs`
- API key persistence: `crates/ai/src/api_keys.rs`, `crates/settings/src/direct_api.rs`
- Direct API logging helpers: `crates/ai/src/logging/mod.rs`, `crates/ai/src/logging/logger_tests.rs`
- Secure storage registration: `app/src/lib.rs`, `crates/warpui_extras/src/secure_storage/mac.rs`
- OSS app identity and channel: `app/src/bin/oss.rs`, `crates/warp_core/src/channel/state.rs`, `crates/warp_core/src/paths.rs`

## Non-Goals

- Do not migrate or read official Warp state under `~/.warp`.
- Do not make Direct API depend on a Warp server.
- Do not log API keys, bearer tokens, request bodies, prompts, tool arguments, terminal output, or file contents.
- Do not promote Rig to default. Keep Rig experimental until diagnostics prove parity.

## Task 1: Build A Redacted Evidence Report

**Files:**
- Create: `docs/superpowers/research/2026-05-17-direct-api-diagnostics.md`
- Read: `~/.warp-oss/settings.toml`
- Read: `~/Library/Logs/warp-oss.log*`
- Read: `target/debug/bundle/osx/WarpOss.app`

- [x] **Step 1: Create the research note**

```markdown
# Direct API Diagnostics - 2026-05-17

## Environment

- Record the repo path from `pwd`.
- Record the exact app path used for reproduction.
- Record build features from the launch command or bundle command.
- Record whether `~/.warp-oss/settings.toml` exists.
- Record whether `~/Library/Logs/warp-oss.log` exists.

## Settings Evidence

## Profile Evidence

## OpenRouter Auth Probe

## In-App Reproduction

## Keychain And Signing Evidence

## Root Cause Candidates

## Confirmed Root Cause
```

- [x] **Step 2: Record sanitized Direct API settings**

Run:

```bash
perl -MFile::Spec -ne '
  BEGIN { $section="" }
  if (/^\s*\[(.+)\]\s*$/) { $section=$1 }
  next unless $section =~ /^agents\.direct_api/;
  if (/^\s*([A-Za-z0-9_.-]+)\s*=\s*"(.*)"\s*$/) {
    my ($k,$v)=($1,$2);
    if ($section =~ /api_keys/ || $k =~ /key|openai|anthropic|google|open_router|custom/i) {
      my $prefix = substr($v,0,8);
      $prefix =~ s/[^A-Za-z0-9_-]/?/g;
      print "$section.$k = <redacted len=".length($v)." prefix=$prefix>\n";
    } else {
      print "$section.$k = $v\n";
    }
  } elsif (/^\s*([A-Za-z0-9_.-]+)\s*=\s*(.+?)\s*$/) {
    print "$section.$1 = $2\n" unless $1 =~ /key/i;
  }
' "$HOME/.warp-oss/settings.toml"
```

Expected: records `selected_provider`, redacted key shape, base URLs, selected models, enabled providers, and `rig_backend_enabled` without exposing secrets.

- [x] **Step 3: Probe OpenRouter key validity outside Warp**

Run:

```bash
tmp_curl_config="$(mktemp /tmp/openrouter-key.XXXXXX)"
chmod 600 "$tmp_curl_config"
perl -0ne '
  if (/open_router\s*=\s*"([^"]+)"/) {
    print "header = \"Authorization: Bearer $1\"\n";
  }
' "$HOME/.warp-oss/settings.toml" > "$tmp_curl_config"
curl -sS -D /tmp/openrouter-key.headers -o /tmp/openrouter-key.body \
  https://openrouter.ai/api/v1/key \
  --config "$tmp_curl_config"
printf 'status=%s\n' "$(awk 'NR==1 {print $2}' /tmp/openrouter-key.headers)"
perl -pe 's/sk-or-v1-[A-Za-z0-9]+/sk-or-v1-<redacted>/g; s/"hash":"[^"]+"/"hash":"<redacted>"/g' /tmp/openrouter-key.body
rm -f "$tmp_curl_config" /tmp/openrouter-key.headers /tmp/openrouter-key.body
```

Expected: `status=200` means the saved key is valid for OpenRouter. `status=401` means the currently saved key is invalid, revoked, or not recognized by OpenRouter. It does not prove the running app used the same key unless route diagnostics or in-app reproduction evidence confirm the runtime credential source.

- [x] **Step 4: Capture in-app reproduction evidence**

Run the same app build that produced the failure. In the app, choose the Direct API profile and run:

```text
/agent test
```

Record:

- selected profile name
- Direct API model label shown in the profile editor
- Agent engine shown in the profile editor
- full error message with secrets redacted
- whether `Rig Agent backend` is globally enabled in Direct API settings

- [x] **Step 5: Inspect normal app logs**

Run:

```bash
rg -n "Direct API|direct_api|backend=rig_agent|OpenRouter|openrouter|provider stream|Unauthorized|User not found|moonshot|kimi|HTTP error|Authentication failed" \
  "$HOME/Library/Logs/warp-oss.log" "$HOME/Library/Logs/warp-oss.log.old."* \
  | perl -pe 's/(Bearer\s+)[A-Za-z0-9_.-]+/${1}<redacted>/g; s/sk-[A-Za-z0-9_-]+/sk-<redacted>/g'
```

Expected: records whether routing diagnostics are already present. Current expectation is that only thin Rig debug events exist and normal Direct API route details are missing.

- [x] **Step 6: Inspect code signing identity for keychain prompts**

Run:

```bash
codesign --display --verbose=4 --requirements :- target/debug/bundle/osx/WarpOss.app 2>&1 | sed -n '1,80p'
security find-generic-password -s dev.warp.WarpOss 2>&1 | sed -n '1,40p'
```

Expected: records whether the app is unsigned, ad hoc signed, Apple Development signed, or Developer ID signed, and whether there are existing keychain items under the `dev.warp.WarpOss` service namespace.

- [x] **Step 7: Write the root-cause decision table**

Add this table to the research note:

```markdown
| Candidate | Evidence Required | Result | Verdict |
|---|---|---|---|
| Saved OpenRouter key is invalid | `/api/v1/key` returns 401 |  |  |
| Warp sends OpenRouter key to wrong endpoint | mocked provider test shows endpoint mismatch |  |  |
| Warp drops Authorization header | mocked provider test shows missing header |  |  |
| Rig OpenRouter path differs from native path | Rig and native diagnostics differ for same config |  |  |
| Profile UI selected a stale/manual model under wrong provider | profile selection provider does not match label |  |  |
| Keychain prompt is caused by unstable code identity | `codesign` DR changes across builds or app is ad hoc signed |  |  |
```

## Task 2: Add Redacted Direct API Route Diagnostics

**Files:**
- Modify: `crates/ai/src/logging/mod.rs`
- Modify: `crates/ai/src/logging/logger_tests.rs`
- Modify: `app/src/ai/agent/api/direct_tools.rs`
- Modify: `app/src/ai/agent/api/rig_direct.rs`
- Modify: `crates/ai/src/provider/genai_adapter.rs`
- Modify: `crates/ai/src/provider/rig_backend.rs`

- [x] **Step 1: Add a failing redaction test**

Add a test in `crates/ai/src/logging/logger_tests.rs`:

```rust
#[test]
fn direct_api_route_diagnostics_do_not_render_secrets() {
    let rendered = redact_direct_api_route_diagnostic(
        "RigAgent",
        "OpenRouter",
        "https://openrouter.ai/api/v1",
        "moonshotai/kimi-k2.6",
        Some("sk-or-v1-secret-secret-secret"),
        Some(401),
        Some("User not found."),
    );

    assert!(rendered.contains("backend=RigAgent"));
    assert!(rendered.contains("provider=OpenRouter"));
    assert!(rendered.contains("base_url_host=openrouter.ai"));
    assert!(rendered.contains("status=401"));
    assert!(rendered.contains("api_key_present=true"));
    assert!(!rendered.contains("sk-or-v1"));
    assert!(!rendered.contains("kimi-k2.6"));
    assert!(!rendered.contains("User not found."));
}
```

- [x] **Step 2: Run the redaction test to verify it fails**

Run:

```bash
cargo test -p ai direct_api_route_diagnostics_do_not_render_secrets -- --nocapture
```

Expected: fails because `redact_direct_api_route_diagnostic` does not exist.

- [x] **Step 3: Implement the redacted formatter**

Add to `crates/ai/src/logging/mod.rs`:

```rust
pub fn redact_direct_api_route_diagnostic(
    backend: &str,
    provider: &str,
    base_url: &str,
    model_id: &str,
    api_key: Option<&str>,
    status: Option<u16>,
    provider_message: Option<&str>,
) -> String {
    let base_url_host = reqwest::Url::parse(base_url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_string))
        .unwrap_or_else(|| "unknown".to_string());
    let status = status
        .map(|status| status.to_string())
        .unwrap_or_else(|| "none".to_string());
    let error_hash = provider_message
        .filter(|message| !message.trim().is_empty())
        .map(hash_custom_model_id)
        .unwrap_or_else(|| "none".to_string());

    format!(
        "direct_api_route backend={} provider={} base_url_host={} model_id_hash={} api_key_present={} status={} provider_error_hash={}",
        safe_log_token(backend),
        safe_log_token(provider),
        safe_log_token(&base_url_host),
        hash_custom_model_id(model_id),
        api_key.is_some_and(|key| !key.trim().is_empty()),
        safe_log_token(&status),
        error_hash,
    )
}

fn safe_log_token(value: &str) -> &str {
    if is_safe_log_value(value) {
        value
    } else {
        "unknown"
    }
}
```

Use `reqwest::Url` because `crates/ai` already depends on `reqwest` and `crates/ai/src/url_validation.rs` already parses Direct API URLs through `reqwest::Url`.

- [x] **Step 4: Wire route-start diagnostics**

In `app/src/ai/agent/api/direct_tools.rs`, log one debug line before invoking the provider:

```rust
log::debug!(
    "{}",
    ai::logging::redact_direct_api_route_diagnostic(
        "NativeGenai",
        provider_name(config.provider_id),
        config.base_url.as_deref().unwrap_or(""),
        &config.model_id,
        config.api_key.as_deref(),
        None,
        None,
    )
);
```

In `app/src/ai/agent/api/rig_direct.rs`, log the same diagnostic with `backend=RigAgent`.

- [x] **Step 5: Wire provider error diagnostics**

In `crates/ai/src/provider/genai_adapter.rs`, convert `exec_chat_stream` errors through a helper that logs status if available from the genai error string. If the concrete genai error exposes structured HTTP status, use the structured field. If not, classify with string matching and only log the numeric status and hashed provider message.

In `crates/ai/src/provider/rig_backend.rs`, extend `categorized_rig_diagnostic` with HTTP status if Rig exposes it. Preserve the existing redacted Rig diagnostic format.

- [x] **Step 6: Run logging tests**

Run:

```bash
cargo test -p ai logging::tests -- --nocapture
```

Expected: all logging redaction tests pass.

## Task 3: Prove And Fix OpenRouter Request Routing

**Files:**
- Modify: `app/src/ai/agent/api/direct_tools.rs`
- Create: `app/src/ai/agent/api/direct_tools_tests.rs`
- Modify: `app/src/ai/agent/api/impl_tests.rs`
- Modify if needed: `crates/ai/src/provider/genai_adapter.rs`
- Modify if needed: `crates/ai/src/provider/rig_backend.rs`

- [x] **Step 1: Add a direct tools test module**

Add to the bottom of `app/src/ai/agent/api/direct_tools.rs`:

```rust
#[cfg(test)]
#[path = "direct_tools_tests.rs"]
mod tests;
```

- [x] **Step 2: Add a route construction test**

Create `app/src/ai/agent/api/direct_tools_tests.rs`:

```rust
use super::*;
use crate::ai::agent::api::DirectApiRouteConfig;

#[test]
fn openrouter_provider_config_uses_openrouter_adapter_label_and_base_url() {
    let config = DirectApiRouteConfig {
        provider_id: ProviderId::OpenRouter,
        model_id: "moonshotai/kimi-k2.6".to_string(),
        api_key: Some("sk-or-v1-test".to_string()),
        base_url: Some("https://openrouter.ai/api/v1".to_string()),
    };

    let provider = provider_for_config(&config);
    assert_eq!(provider.diagnostic_provider_label(), "openrouter");
    assert_eq!(provider.diagnostic_base_url(), Some("https://openrouter.ai/api/v1/"));
}
```

- [x] **Step 3: Run the test to verify it fails**

Run:

```bash
cargo test -p warp openrouter_provider_config_uses_openrouter_adapter_label_and_base_url -- --nocapture
```

Expected: fails because debug accessors do not exist or because endpoint normalization differs.

- [x] **Step 4: Add safe diagnostic accessors**

In `crates/ai/src/provider/genai_adapter.rs`, add accessors that do not expose secrets:

```rust
impl GenaiAdapter {
    pub fn diagnostic_provider_label(&self) -> &str {
        &self.provider
    }

    pub fn diagnostic_base_url(&self) -> Option<&str> {
        self.base_url.as_deref()
    }
}
```

- [x] **Step 5: Add a live-gated OpenRouter smoke test**

Create or extend `crates/ai/tests/e2e_direct_provider.rs` with an ignored test:

```rust
#[tokio::test]
#[ignore = "requires OPENROUTER_API_KEY and network"]
async fn openrouter_key_endpoint_accepts_saved_bearer_key() {
    let key = std::env::var("OPENROUTER_API_KEY")
        .expect("OPENROUTER_API_KEY must be set for this ignored test");
    let status = reqwest::Client::new()
        .get("https://openrouter.ai/api/v1/key")
        .header("Authorization", format!("Bearer {key}"))
        .send()
        .await
        .expect("request should complete")
        .status();

    assert_eq!(status.as_u16(), 200);
}
```

- [x] **Step 6: Decide the 401 remediation from evidence**

Use the Task 1 report:

- If `/api/v1/key` returns 401, surface "OpenRouter rejected the saved API key" in UI and docs. Do not change routing.
- If `/api/v1/key` returns 200 and Warp still returns 401, fix the provider path that diverges from the external probe.
- If native succeeds and Rig fails, keep Rig disabled by default and patch `RigProviderKind::OpenRouter` first.
- If Rig and native both fail only for OpenRouter models, inspect whether the OpenAI-compatible adapter sends the wrong base URL, missing bearer auth, or incompatible body.

- [x] **Step 7: Run route tests**

Run:

```bash
cargo test -p warp direct_api -- --nocapture
cargo test -p ai openrouter -- --nocapture
```

Expected: route config, model list, and OpenRouter adapter tests pass.

## Task 4: Replace The Messy Provider Settings Layout

**Files:**
- Modify: `app/src/settings_view/direct_api_page.rs`
- Modify: `app/src/settings_view/direct_api_page_tests.rs`

- [ ] **Step 1: Add pure row-state tests**

Add tests proving labels and controls are derived from provider state:

```rust
#[test]
fn provider_row_primary_status_labels_are_short() {
    assert_eq!(ProviderType::OpenRouter.as_str(), "OpenRouter");
    assert_eq!(ProviderType::OpenRouter.api_key_placeholder(), "sk-or-...");
    assert_eq!(ProviderType::OpenRouter.default_base_url(), "https://openrouter.ai/api/v1");
}

#[test]
fn provider_rows_keep_custom_last_for_scanability() {
    assert_eq!(ProviderType::all().last(), Some(&ProviderType::Custom));
}
```

- [ ] **Step 2: Run tests before layout changes**

Run:

```bash
cargo test -p warp direct_api_page -- --nocapture
```

Expected: existing tests pass.

- [ ] **Step 3: Split provider row rendering into named helpers**

In `app/src/settings_view/direct_api_page.rs`, extract helpers:

```rust
fn render_provider_title_cell(
    provider: ProviderType,
    enabled: bool,
    appearance: &Appearance,
) -> Box<dyn Element> {
    // Provider label plus compact Enabled/Disabled status.
}

fn render_provider_key_cell(
    row: &ProviderRowState,
    appearance: &Appearance,
) -> Box<dyn Element> {
    // Key input plus eye icon.
}

fn render_provider_action_row(row: &ProviderRowState) -> Box<dyn Element> {
    // Save, Test, Enable/Disable, Refresh models.
}

fn render_provider_base_url_row(
    row: &ProviderRowState,
    appearance: &Appearance,
) -> Option<Box<dyn Element>> {
    // Base URL label and input without hard-coded left offset.
}
```

Use existing `ActionButton` themes unchanged. Do not introduce a feature-specific button theme.

- [ ] **Step 4: Use a stacked responsive row**

Replace the single wide `top_row` with this visual structure:

```text
Provider label + status
API key input + eye icon
Action row
Base URL input when applicable
Model dropdown when available
Result message
```

Desktop may use two columns, but every column must have a fixed max width and must not depend on `margin_left(182.)`.

- [ ] **Step 5: Validate visually**

Run:

```bash
cargo check -p warp --bin warp-oss
./script/macos/run --oss
```

Open Direct API settings and capture evidence:

- dark theme
- light theme if available
- width similar to the screenshot
- narrow settings pane
- OpenRouter with model list loaded
- Custom with base URL visible
- error result visible
- success result visible

Expected: no overlapping labels, inputs, buttons, or status text.

## Task 5: Add A Strong Agent Engine Selected Indicator

**Files:**
- Modify: `app/src/ai/execution_profiles/editor/ui_helpers.rs`
- Modify: `app/src/ai/execution_profiles/editor/mod_tests.rs`

- [ ] **Step 1: Add state tests**

Extend `app/src/ai/execution_profiles/editor/mod_tests.rs`:

```rust
#[test]
fn direct_api_agent_backend_state_marks_rig_selected_when_available() {
    let profile = direct_api_profile_with_backend(DirectApiAgentBackend::RigAgent);
    let state = direct_api_agent_backend_selector_state(&profile, true, true).unwrap();
    assert_eq!(state.selected_backend, DirectApiAgentBackend::RigAgent);
    assert!(state.options.iter().any(|option| {
        option.backend == DirectApiAgentBackend::RigAgent && option.enabled
    }));
}
```

- [ ] **Step 2: Run the state tests**

Run:

```bash
cargo test -p warp direct_api_agent_backend -- --nocapture
```

Expected: existing selector-state behavior is preserved.

- [ ] **Step 3: Render a non-color selected signal**

In `render_direct_api_agent_backend_button`, add a check icon or explicit selected suffix for the selected option. Prefer an existing icon:

```rust
let label = if selected {
    format!("{} selected", option.label)
} else {
    option.label.to_string()
};

let mut button = appearance
    .ui_builder()
    .button(ButtonVariant::Secondary, mouse_state)
    .with_centered_text_label(label)
    .with_style(/* existing dimensions */);
```

If the button builder supports leading icons, use `Icon::Check` for selected instead of visible "selected" copy. Keep the text fallback if icon-only state is not accessible.

- [ ] **Step 4: Validate profile UI**

Run the app and inspect the profile editor:

- Direct API route selected
- Native selected
- Rig selected
- Rig persisted but feature unavailable

Expected: selected backend is unambiguous without relying only on subtle color.

## Task 6: Make Provider Test And Save Semantics Honest

**Files:**
- Modify: `app/src/settings_view/direct_api_page.rs`
- Modify: `app/src/settings_view/direct_api_page_tests.rs`
- Modify if needed: `crates/ai/src/model_registry/providers/openrouter.rs`
- Modify if needed: `crates/ai/src/model_registry/providers/custom.rs`

- [ ] **Step 1: Add tests that reject "full test pending" as success**

Add a test around provider result text helpers after extracting them:

```rust
#[test]
fn remote_provider_test_result_is_not_reported_as_validated_until_network_probe_runs() {
    assert_eq!(
        provider_preflight_message(ProviderType::OpenRouter),
        "API key format valid. Run Refresh models to validate provider access."
    );
}
```

- [ ] **Step 2: Replace misleading test copy**

Replace "API key format valid (full test pending)" with:

```text
API key format valid. Run Refresh models to validate provider access.
```

For OpenRouter, after a successful model refresh, show:

```text
OK: OpenRouter access validated. Fetched N models.
```

For `401` or `403`, show:

```text
Error: Provider rejected the saved API key.
```

- [ ] **Step 3: Keep blank-key preservation behavior**

Preserve `api_key_or_saved` behavior for all keyed providers:

```rust
fn api_key_or_saved(
    &self,
    provider: ProviderType,
    api_key: &str,
    ctx: &ViewContext<Self>,
) -> Option<String> {
    if !api_key.trim().is_empty() {
        return Some(api_key.to_string());
    }

    let keys = self.api_key_manager.as_ref(ctx).keys(ctx);
    provider.saved_api_key(&keys)
}
```

- [ ] **Step 4: Run settings UI tests**

Run:

```bash
cargo test -p warp direct_api_page -- --nocapture
cargo test -p ai api_keys::tests -- --nocapture --test-threads=1
```

Expected: UI helper tests and persistence tests pass.

## Task 7: Diagnose And Remediate macOS Keychain Prompts

**Files:**
- Modify if needed: `script/macos/bundle`
- Modify if needed: `docs/features/direct-api-user-guide.md`
- Modify if needed: `docs/QUICK-START.md`
- Do not modify official `~/.warp`

- [ ] **Step 1: Determine whether Direct API itself needs keychain**

Run:

```bash
rg -n "secure_storage\\(|read_value\\(|write_value\\(|migrate_from_keychain|ApiKeyManager|DirectAPISettings" app/src crates/ai/src crates/warpui_extras/src
```

Expected: Direct API steady-state config uses `DirectAPISettings` and `~/.warp-oss/settings.toml`. Other app features such as MCP/OAuth may still use secure storage.

- [ ] **Step 2: Compare signing identities across builds**

Run twice, once for the app that prompts and once for any previous app build that did not:

```bash
codesign --display --verbose=4 --requirements :- /path/to/WarpOss.app 2>&1 | sed -n '1,90p'
spctl --assess --type execute --verbose=4 /path/to/WarpOss.app 2>&1 | sed -n '1,60p'
```

Expected: identifies ad hoc, unsigned, Apple Development, or Developer ID Application identity.

- [ ] **Step 3: Remediation decision**

Use this decision table:

| Evidence | Remediation |
|---|---|
| App is unsigned or ad hoc signed | Sign local bundles with a stable Apple Development identity during local testing. |
| App is signed with changing identities | Standardize on one identity for `WarpOss.app`. |
| Release build needs fewer prompts across installs | Sign with Developer ID Application and stable bundle identifier `dev.warp.WarpOss`. |
| Direct API does not require keychain but another feature prompts | Keep Direct API in settings TOML and document which feature caused secure storage access. |
| Existing keychain item was created by another signing identity | Delete the stale `dev.warp.WarpOss` keychain item or allow access once for the new signed identity. |

- [ ] **Step 4: Add signing documentation**

Document:

```markdown
Warp OSS uses its own app identity, `dev.warp.WarpOss`, and its own config path, `~/.warp-oss`.
It cannot reuse official Warp's keychain trust decision because macOS keychain access is tied to the calling app's trusted code identity.
For local builds, use a stable Apple Development signing identity.
For distributed builds, use Developer ID Application signing and keep the bundle identifier stable.
```

- [ ] **Step 5: Validate keychain prompt behavior**

After signing:

```bash
open -n target/debug/bundle/osx/WarpOss.app
```

Expected:

- first run may prompt for existing items created under another identity
- repeated runs of the same signed app should not repeatedly prompt for the same item after "Always Allow"
- Direct API settings save and model refresh should not create new keychain prompts by themselves

## Task 8: Final Validation Matrix

**Files:**
- Read: all files changed in Tasks 2 through 7

- [ ] **Step 1: Run targeted unit tests**

Run:

```bash
cargo test -p ai logging::tests -- --nocapture
cargo test -p ai openrouter -- --nocapture
cargo test -p ai api_keys::tests -- --nocapture --test-threads=1
cargo test -p warp direct_api_page -- --nocapture
cargo test -p warp direct_api_agent_backend -- --nocapture
cargo test -p warp direct_api -- --nocapture
```

Expected: all pass.

- [ ] **Step 2: Run build check**

Run:

```bash
cargo check -p warp --bin warp-oss
```

Expected: completes without errors.

- [ ] **Step 3: Run optional Rig feature check if the feature is enabled in the target build**

Run:

```bash
cargo check -p warp --bin warp-oss --features direct_api_rig_backend
```

Expected: completes without errors, or records the exact missing feature/dependency blocker.

- [ ] **Step 4: Run live OpenRouter smoke test only with explicit key**

Run:

```bash
cargo test -p ai openrouter_key_endpoint_accepts_saved_bearer_key -- --ignored --nocapture
```

Expected: passes when `OPENROUTER_API_KEY` is already set in the shell to a valid key. Fails with 401 when the supplied OpenRouter key is invalid or revoked. Do not inline the key in the command or derive it from `settings.toml` in process arguments.

- [ ] **Step 5: Manual app validation**

Validate:

- Direct API settings provider rows do not overlap at screenshot width.
- OpenRouter save, refresh models, model selection, and profile selection are understandable.
- Direct API profile clearly shows selected backend.
- `/agent test` either succeeds or returns an error that identifies provider auth failure without implying Warp cloud auth.
- Logs contain redacted diagnostics only.
- Reopening signed `WarpOss.app` does not repeatedly prompt for the same keychain item after permission is granted.

## Completion Criteria

- Root cause for the 401 is classified as saved-key invalid, native adapter bug, Rig adapter bug, profile-selection bug, or provider/body incompatibility.
- Provider settings UI is readable and non-overlapping.
- Agent engine selected state is visible without relying only on subtle color.
- Direct API diagnostics explain provider/backend/base URL host/status without secrets.
- Keychain prompt behavior is explained by concrete signing identity evidence.
- `cargo check -p warp --bin warp-oss` passes.
- Targeted tests for logging, OpenRouter, settings persistence, Direct API route config, settings UI, and profile backend state pass.
