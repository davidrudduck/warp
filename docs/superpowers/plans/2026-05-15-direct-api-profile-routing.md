# Direct API Profile Routing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add per-profile model routing so Warp OSS Agent Mode can use either Warp Provider models or locally configured Direct API providers.

**Architecture:** Store routing on `AIExecutionProfile`, render a compact routing selector in the existing profile editor, and carry a resolved Direct API route config through `RequestParams`. The Warp Provider route keeps the current `ServerApi::generate_multi_agent_output` path; the Direct API route builds a local provider, emits existing `warp_multi_agent_api::ResponseEvent` shapes, and never attaches Direct API keys to server requests.

**Tech Stack:** Rust 2018, WarpUI entity/view model, `serde`, `warp_multi_agent_api`, `genai`, `reqwest`, `tokio`, `futures`, local `DirectAPISettings` in `~/.warp-oss/settings.toml`.

---

## Source References

- Design spec: `docs/superpowers/specs/2026-05-15-direct-api-profile-routing-design.md`
- OpenAI function calling: https://developers.openai.com/api/docs/guides/function-calling
- Anthropic tool use: https://platform.claude.com/docs/en/agents-and-tools/tool-use/how-tool-use-works
- Ollama OpenAI compatibility: https://docs.ollama.com/api/openai-compatibility
- Google Gemini function calling: https://docs.cloud.google.com/vertex-ai/generative-ai/docs/multimodal/function-calling

## File Structure

- Modify `app/src/ai/execution_profiles/mod.rs`
  - Add `ModelRouting` and `DirectApiProfileModelSelection`.
  - Add fields to `AIExecutionProfile` with backwards-compatible defaults.
- Modify `app/src/ai/execution_profiles/profiles.rs`
  - Add setters for routing and Direct API profile model selection.
  - Emit existing `ProfileUpdated` events.
- Modify `app/src/ai/execution_profiles/profiles_tests.rs`
  - Prove profile defaults and setters.
- Create `app/src/ai/execution_profiles/direct_api_model_choices.rs`
  - Build `Provider / Model` choices from `ApiKeyManager`, cached model lists, and saved selections.
- Modify `app/src/ai/execution_profiles/editor/mod.rs`
  - Add routing dropdown and Direct API model dropdown.
  - Refresh Direct API choices when profile, model cache, or API keys change.
- Modify `app/src/ai/execution_profiles/editor/ui_helpers.rs`
  - Render `Model Routing` before the base model row.
  - Swap the base model row for the Direct API model row when Direct API routing is selected.
- Modify `app/src/ai/execution_profiles/model_menu_items.rs`
  - Keep Warp Provider choices untouched.
- Modify `app/src/ai/agent/api.rs`
  - Add `DirectApiRouteConfig` and profile routing fields to `RequestParams`.
- Modify `app/src/ai/agent/api/impl.rs`
  - Branch to a local Direct API response stream when routing is `DirectApi`.
  - Keep server routing unchanged for `WarpProvider` and `Unknown`.
- Create `app/src/ai/agent/api/direct.rs`
  - Convert `RequestParams` into provider messages.
  - Build local provider adapter.
  - Convert local provider events into `warp_multi_agent_api::ResponseEvent`.
- Create `app/src/ai/agent/api/direct_tools.rs`
  - Define the local tool schema subset and map provider tool calls to proto tool calls.
- Modify `crates/ai/src/provider/genai_adapter.rs`
  - Preserve assistant tool calls and user tool results.
  - Support custom base URLs where the provider supports them.
- Modify `crates/ai/src/model_registry/providers/custom.rs`
  - Normalize base URLs before adding `/models`.
- Modify `app/src/settings_view/direct_api_page.rs`
  - Replace string-prefix HTTP checks with shared parsed URL validation.
- Create `crates/ai/src/url_validation.rs`
  - Shared URL validation for model-list and route setup.
- Modify `crates/ai/src/lib.rs`
  - Export `url_validation`.
- Modify tests under `crates/ai/src/provider/`, `crates/ai/src/model_registry/providers/`, and `app/src/ai/agent/api/`.

## Task 1: Profile Routing Schema

**Files:**
- Modify `app/src/ai/execution_profiles/mod.rs`
- Modify `app/src/ai/execution_profiles/profiles_tests.rs`

- [x] **Step 1: Add failing serialization/default tests**

Add these tests to `app/src/ai/execution_profiles/profiles_tests.rs`:

```rust
use crate::ai::execution_profiles::{
    AIExecutionProfile, DirectApiProfileModelSelection, ModelRouting,
};
use ai::model_registry::ProviderId;

#[test]
fn execution_profile_defaults_to_warp_provider_routing() {
    let profile = AIExecutionProfile::default();

    assert_eq!(profile.model_routing, ModelRouting::WarpProvider);
    assert_eq!(profile.direct_api_model, None);
}

#[test]
fn execution_profile_deserializes_missing_routing_as_warp_provider() {
    let profile: AIExecutionProfile = serde_json::from_str(
        r#"{
            "name": "Default",
            "is_default_profile": true
        }"#,
    )
    .expect("profile should deserialize");

    assert_eq!(profile.model_routing, ModelRouting::WarpProvider);
    assert_eq!(profile.direct_api_model, None);
}

#[test]
fn execution_profile_roundtrips_direct_api_selection() {
    let profile = AIExecutionProfile {
        model_routing: ModelRouting::DirectApi,
        direct_api_model: Some(DirectApiProfileModelSelection {
            provider_id: ProviderId::OpenAI,
            model_id: "gpt-4o-mini".to_string(),
        }),
        ..AIExecutionProfile::default()
    };

    let serialized = serde_json::to_string(&profile).expect("profile should serialize");
    let decoded: AIExecutionProfile =
        serde_json::from_str(&serialized).expect("profile should deserialize");

    assert_eq!(decoded.model_routing, ModelRouting::DirectApi);
    assert_eq!(
        decoded.direct_api_model,
        Some(DirectApiProfileModelSelection {
            provider_id: ProviderId::OpenAI,
            model_id: "gpt-4o-mini".to_string(),
        })
    );
}
```

- [x] **Step 2: Run test to verify failure**

Run:

```bash
cargo test -p warp execution_profile_defaults_to_warp_provider_routing -- --nocapture
```

Expected: compile failure because `ModelRouting` and `DirectApiProfileModelSelection` do not exist.

- [x] **Step 3: Add profile routing types and fields**

In `app/src/ai/execution_profiles/mod.rs`, add the import:

```rust
use ai::model_registry::ProviderId;
```

Add the types before `AIExecutionProfile`:

```rust
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelRouting {
    #[default]
    WarpProvider,
    DirectApi,
    #[serde(other)]
    Unknown,
}

impl ModelRouting {
    pub fn is_direct_api(self) -> bool {
        matches!(self, Self::DirectApi)
    }

    pub fn effective(self) -> Self {
        match self {
            Self::WarpProvider | Self::Unknown => Self::WarpProvider,
            Self::DirectApi => Self::DirectApi,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectApiProfileModelSelection {
    pub provider_id: ProviderId,
    pub model_id: String,
}

impl DirectApiProfileModelSelection {
    pub fn label(&self) -> String {
        format!("{} / {}", self.provider_id.display_name(), self.model_id)
    }
}
```

Add fields to `AIExecutionProfile` after `base_model`:

```rust
    pub base_model: Option<LLMId>,
    pub model_routing: ModelRouting,
    pub direct_api_model: Option<DirectApiProfileModelSelection>,
    pub coding_model: Option<LLMId>,
```

Set defaults in every `AIExecutionProfile` construction that lists model fields explicitly:

```rust
            base_model: None,
            model_routing: ModelRouting::WarpProvider,
            direct_api_model: None,
            coding_model: None,
```

- [x] **Step 4: Run tests**

Run:

```bash
cargo test -p warp execution_profile_ -- --nocapture
```

Expected: all three tests pass.

- [x] **Step 5: Commit**

```bash
git add app/src/ai/execution_profiles/mod.rs app/src/ai/execution_profiles/profiles_tests.rs
git commit -m "Add profile model routing state"
```

## Task 2: Profile Setters and Request Route Config

**Files:**
- Modify `app/src/ai/execution_profiles/profiles.rs`
- Modify `app/src/ai/execution_profiles/profiles_tests.rs`
- Modify `app/src/ai/agent/api.rs`

- [ ] **Step 1: Add failing setter tests**

Add tests to `app/src/ai/execution_profiles/profiles_tests.rs`:

```rust
use warpui::App;

#[test]
fn profile_setters_preserve_other_route_selection() {
    App::test(|ctx| {
        let handle = AIExecutionProfilesModel::handle(ctx);
        let profile_id = handle.as_ref(ctx).default_profile_id();

        handle.update(ctx, |model, ctx| {
            model.set_model_routing(profile_id, ModelRouting::DirectApi, ctx);
            model.set_direct_api_model(
                profile_id,
                Some(DirectApiProfileModelSelection {
                    provider_id: ProviderId::Anthropic,
                    model_id: "claude-3-5-sonnet-20241022".to_string(),
                }),
                ctx,
            );
        });

        let profile = handle.as_ref(ctx).profile(profile_id).unwrap().data();
        assert_eq!(profile.model_routing, ModelRouting::DirectApi);
        assert_eq!(
            profile.direct_api_model.as_ref().map(|selection| selection.label()),
            Some("Anthropic / claude-3-5-sonnet-20241022".to_string())
        );
    });
}
```

- [ ] **Step 2: Run test to verify failure**

Run:

```bash
cargo test -p warp profile_setters_preserve_other_route_selection -- --nocapture
```

Expected: compile failure for missing setter methods.

- [ ] **Step 3: Add setters**

Add to `impl AIExecutionProfilesModel` in `app/src/ai/execution_profiles/profiles.rs` near `set_base_model`:

```rust
    pub fn set_model_routing(
        &mut self,
        profile_id: ClientProfileId,
        routing: ModelRouting,
        ctx: &mut ModelContext<Self>,
    ) {
        self.edit_profile_internal(
            profile_id,
            |profile| {
                if profile.model_routing != routing {
                    profile.model_routing = routing;
                    return true;
                }
                false
            },
            ctx,
        );
    }

    pub fn set_direct_api_model(
        &mut self,
        profile_id: ClientProfileId,
        selection: Option<DirectApiProfileModelSelection>,
        ctx: &mut ModelContext<Self>,
    ) {
        self.edit_profile_internal(
            profile_id,
            |profile| {
                if profile.direct_api_model != selection {
                    profile.direct_api_model = selection.clone();
                    return true;
                }
                false
            },
            ctx,
        );
    }
```

Add imports at the top if they are not in scope:

```rust
use super::{DirectApiProfileModelSelection, ModelRouting};
```

- [ ] **Step 4: Add route config to request params**

In `app/src/ai/agent/api.rs`, add imports:

```rust
use ai::model_registry::ProviderId;
use crate::ai::execution_profiles::{DirectApiProfileModelSelection, ModelRouting};
```

Add this struct near `RequestParams`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectApiRouteConfig {
    pub provider_id: ProviderId,
    pub model_id: String,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
}

impl DirectApiRouteConfig {
    pub fn from_selection(
        selection: &DirectApiProfileModelSelection,
        keys: &ai::api_keys::ApiKeys,
    ) -> Option<Self> {
        let api_key = match selection.provider_id {
            ProviderId::OpenAI => keys.openai.clone(),
            ProviderId::Anthropic => keys.anthropic.clone(),
            ProviderId::GoogleGemini => keys.google.clone(),
            ProviderId::OpenRouter => keys.open_router.clone(),
            ProviderId::Custom => keys.custom.clone(),
            ProviderId::Ollama => None,
        };
        let base_url = match selection.provider_id {
            ProviderId::OpenRouter => keys.openrouter_base_url.clone(),
            ProviderId::Ollama => keys.ollama_base_url.clone(),
            ProviderId::Custom => keys.custom_base_url.clone(),
            ProviderId::OpenAI | ProviderId::Anthropic | ProviderId::GoogleGemini => None,
        };

        Some(Self {
            provider_id: selection.provider_id,
            model_id: selection.model_id.clone(),
            api_key,
            base_url,
        })
    }
}
```

Add fields to `RequestParams`:

```rust
    pub model_routing: ModelRouting,
    pub direct_api_route_config: Option<DirectApiRouteConfig>,
```

In `RequestParams::new`, reuse the active profile data already read for context window. Compute:

```rust
        let profile_data = AIExecutionProfilesModel::as_ref(app)
            .active_profile(terminal_view_id, app)
            .data()
            .clone();
        let model_routing = profile_data.model_routing.effective();
        let direct_api_route_config = profile_data
            .direct_api_model
            .as_ref()
            .and_then(|selection| {
                let keys = ApiKeyManager::as_ref(app).keys(app);
                DirectApiRouteConfig::from_selection(selection, &keys)
            });
```

Keep the existing context-window clamping, but use this `profile_data` instead of fetching the profile a second time.

Set the new fields in `Self`:

```rust
            model_routing,
            direct_api_route_config,
```

- [ ] **Step 5: Run tests**

Run:

```bash
cargo test -p warp profile_setters_preserve_other_route_selection -- --nocapture
```

Expected: test passes.

- [ ] **Step 6: Commit**

```bash
git add app/src/ai/execution_profiles/profiles.rs app/src/ai/execution_profiles/profiles_tests.rs app/src/ai/agent/api.rs
git commit -m "Resolve Direct API profile route config"
```

## Task 3: Direct API Model Choice Helper

**Files:**
- Create `app/src/ai/execution_profiles/direct_api_model_choices.rs`
- Modify `app/src/ai/execution_profiles/mod.rs`

- [ ] **Step 1: Add helper module with tests**

Create `app/src/ai/execution_profiles/direct_api_model_choices.rs`:

```rust
use std::collections::BTreeSet;
use std::time::Duration;

use ai::api_keys::ApiKeys;
use ai::model_registry::{ModelListCache, ProviderId};

use super::DirectApiProfileModelSelection;

const MODEL_CACHE_MAX_AGE: Duration = Duration::from_secs(60 * 60 * 24 * 7);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectApiModelChoice {
    pub selection: DirectApiProfileModelSelection,
    pub label: String,
    pub is_stale_or_manual: bool,
}

pub fn direct_api_model_choices_from_parts(
    keys: &ApiKeys,
    cache: Option<&ModelListCache>,
) -> Vec<DirectApiModelChoice> {
    let mut choices = Vec::new();
    let mut seen = BTreeSet::new();

    for provider_id in direct_api_providers_with_possible_models(keys) {
        if let Some(cache) = cache {
            if let Some(entry) = cache.get(provider_id, MODEL_CACHE_MAX_AGE) {
                for model in entry.models {
                    push_choice(&mut choices, &mut seen, provider_id, model.id, false);
                }
            }
        }

        if let Some(model_id) = keys.selected_models.get(&provider_id) {
            push_choice(&mut choices, &mut seen, provider_id, model_id.clone(), true);
        }
    }

    choices
}

fn direct_api_providers_with_possible_models(keys: &ApiKeys) -> Vec<ProviderId> {
    let mut providers = Vec::new();
    if keys.openai.as_ref().is_some_and(|key| !key.is_empty()) {
        providers.push(ProviderId::OpenAI);
    }
    if keys.anthropic.as_ref().is_some_and(|key| !key.is_empty()) {
        providers.push(ProviderId::Anthropic);
    }
    if keys.google.as_ref().is_some_and(|key| !key.is_empty()) {
        providers.push(ProviderId::GoogleGemini);
    }
    if keys.open_router.as_ref().is_some_and(|key| !key.is_empty()) {
        providers.push(ProviderId::OpenRouter);
    }
    if keys.custom_base_url.as_ref().is_some_and(|url| !url.is_empty()) {
        providers.push(ProviderId::Custom);
    }
    if keys.ollama_base_url.as_ref().is_some_and(|url| !url.is_empty()) {
        providers.push(ProviderId::Ollama);
    }
    providers
}

fn push_choice(
    choices: &mut Vec<DirectApiModelChoice>,
    seen: &mut BTreeSet<(ProviderId, String)>,
    provider_id: ProviderId,
    model_id: String,
    is_stale_or_manual: bool,
) {
    let model_id = model_id.trim().to_string();
    if model_id.is_empty() || !seen.insert((provider_id, model_id.clone())) {
        return;
    }

    let selection = DirectApiProfileModelSelection {
        provider_id,
        model_id,
    };
    choices.push(DirectApiModelChoice {
        label: selection.label(),
        selection,
        is_stale_or_manual,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_api_choices_include_saved_manual_model_without_cache() {
        let mut keys = ApiKeys::default();
        keys.openai = Some("sk-test".to_string());
        keys.selected_models
            .insert(ProviderId::OpenAI, "gpt-4o-mini".to_string());

        let choices = direct_api_model_choices_from_parts(&keys, None);

        assert_eq!(choices.len(), 1);
        assert_eq!(choices[0].label, "OpenAI / gpt-4o-mini");
        assert!(choices[0].is_stale_or_manual);
    }

    #[test]
    fn direct_api_choices_ignore_providers_without_key_or_base_url() {
        let keys = ApiKeys::default();

        let choices = direct_api_model_choices_from_parts(&keys, None);

        assert!(choices.is_empty());
    }
}
```

- [ ] **Step 2: Wire module**

Add to `app/src/ai/execution_profiles/mod.rs`:

```rust
pub mod direct_api_model_choices;
```

- [ ] **Step 3: Run tests**

Run:

```bash
cargo test -p warp direct_api_choices_ -- --nocapture
```

Expected: both helper tests pass.

- [ ] **Step 4: Commit**

```bash
git add app/src/ai/execution_profiles/direct_api_model_choices.rs app/src/ai/execution_profiles/mod.rs
git commit -m "Build Direct API model choices for profiles"
```

## Task 4: Profile Editor Routing UI

**Files:**
- Modify `app/src/ai/execution_profiles/editor/mod.rs`
- Modify `app/src/ai/execution_profiles/editor/ui_helpers.rs`

- [ ] **Step 1: Add editor actions and fields**

In `app/src/ai/execution_profiles/editor/mod.rs`, extend imports:

```rust
use crate::ai::execution_profiles::{
    direct_api_model_choices::{direct_api_model_choices_from_parts, DirectApiModelChoice},
    DirectApiProfileModelSelection, ModelRouting,
};
```

Add actions after `SetBaseModel`:

```rust
    SetModelRouting {
        routing: ModelRouting,
    },
    SetDirectApiModel {
        selection: DirectApiProfileModelSelection,
    },
```

Add fields to `ExecutionProfileEditorView` after `base_model_dropdown`:

```rust
    model_routing_dropdown: ViewHandle<Dropdown<ExecutionProfileEditorViewAction>>,
    direct_api_model_dropdown: ViewHandle<FilterableDropdown<ExecutionProfileEditorViewAction>>,
```

- [ ] **Step 2: Create dropdown views**

In `ExecutionProfileEditorView::new`, create these before `base_model_dropdown`:

```rust
        let model_routing_dropdown = ctx.add_typed_action_view(|ctx| {
            let mut dropdown = Dropdown::new(ctx);
            dropdown.set_items(
                vec![
                    DropdownItem::new("Warp Provider")
                        .with_on_select_action(
                            ExecutionProfileEditorViewAction::SetModelRouting {
                                routing: ModelRouting::WarpProvider,
                            }
                            .into(),
                        ),
                    DropdownItem::new("Direct API")
                        .with_on_select_action(
                            ExecutionProfileEditorViewAction::SetModelRouting {
                                routing: ModelRouting::DirectApi,
                            }
                            .into(),
                        ),
                ],
                0,
                ctx,
            );
            dropdown
        });

        let direct_api_model_dropdown = ctx.add_typed_action_view(|ctx| {
            let mut dropdown = FilterableDropdown::new(ctx);
            dropdown.set_placeholder_text("Select Direct API model", ctx);
            dropdown
        });
```

Store both fields in `Self`.

- [ ] **Step 3: Add refresh helpers**

Add methods to `impl ExecutionProfileEditorView`:

```rust
    fn refresh_model_routing_dropdown(&self, profile: &AIExecutionProfile, ctx: &mut ViewContext<Self>) {
        let selected = match profile.model_routing.effective() {
            ModelRouting::WarpProvider | ModelRouting::Unknown => 0,
            ModelRouting::DirectApi => 1,
        };
        self.model_routing_dropdown.update(ctx, |dropdown, ctx| {
            dropdown.set_selected_index(selected, ctx);
        });
    }

    fn refresh_direct_api_model_dropdown(&self, app: &mut AppContext) {
        let keys = ApiKeyManager::as_ref(app).keys(app);
        let cache = ModelListCache::new().ok();
        let choices = direct_api_model_choices_from_parts(&keys, cache.as_ref());
        let current_profile = AIExecutionProfilesModel::as_ref(app)
            .profile(self.profile_id)
            .map(|profile| profile.data().clone());
        let selected = current_profile
            .as_ref()
            .and_then(|profile| profile.direct_api_model.as_ref())
            .and_then(|current| choices.iter().position(|choice| &choice.selection == current))
            .unwrap_or(0);

        self.direct_api_model_dropdown.update(app, |dropdown, ctx| {
            if choices.is_empty() {
                dropdown.set_items(
                    vec![DropdownItem::new("Configure Direct API provider in Agents settings")],
                    0,
                    ctx,
                );
                dropdown.set_enabled(false, ctx);
                return;
            }

            let items = choices
                .into_iter()
                .map(|DirectApiModelChoice { selection, label, .. }| {
                    DropdownItem::new(label).with_on_select_action(
                        ExecutionProfileEditorViewAction::SetDirectApiModel { selection }.into(),
                    )
                })
                .collect();
            dropdown.set_enabled(true, ctx);
            dropdown.set_items(items, selected, ctx);
        });
    }
```

The required dropdown methods are the same methods already used by `refresh_model_selection_dropdown` in `app/src/ai/execution_profiles/editor/mod.rs`: update the item list, selected index, enabled state, and placeholder through the dropdown view handles.

- [ ] **Step 4: Handle actions**

Add cases to `handle_action`:

```rust
            ExecutionProfileEditorViewAction::SetModelRouting { routing } => {
                AIExecutionProfilesModel::handle(ctx).update(ctx, |profiles_model, ctx| {
                    profiles_model.set_model_routing(self.profile_id, *routing, ctx);
                });
                self.refresh(ctx);
            }
            ExecutionProfileEditorViewAction::SetDirectApiModel { selection } => {
                AIExecutionProfilesModel::handle(ctx).update(ctx, |profiles_model, ctx| {
                    profiles_model.set_direct_api_model(
                        self.profile_id,
                        Some(selection.clone()),
                        ctx,
                    );
                });
                self.refresh(ctx);
            }
```

- [ ] **Step 5: Render routing rows**

In `app/src/ai/execution_profiles/editor/ui_helpers.rs`, update `render_models_section`:

```rust
    let profile = AIExecutionProfilesModel::as_ref(app)
        .profile(view.profile_id)
        .map(|profile| profile.data().clone())
        .unwrap_or_default();
    let routing = profile.model_routing.effective();

    let mut column = Flex::column()
        .with_child(render_separator(appearance))
        .with_child(render_section_label("MODELS", appearance))
        .with_child(render_dropdown_row(
            appearance,
            "Model Routing",
            "Choose whether this profile uses Warp Provider models or locally configured Direct API models.",
            &view.model_routing_dropdown,
        ));

    if routing.is_direct_api() {
        column = column.with_child(render_filterable_dropdown_row(
            appearance,
            "Direct API model",
            "The provider and model used by this profile for Agent Mode.",
            &view.direct_api_model_dropdown,
        ));
    } else {
        column = column.with_child(render_filterable_dropdown_row(
            appearance,
            "Base model",
            "This model serves as the primary engine behind the agent. It powers most interactions and invokes other models for tasks like planning or code generation when necessary. Warp may automatically switch to alternate models based on model availability or for auxiliary tasks such as conversation summarization.",
            &view.base_model_dropdown,
        ));

        if let Some(row) = render_context_window_row(appearance, view, app) {
            column.add_child(row);
        }

        column = column.with_child(render_filterable_dropdown_row(
            appearance,
            "Full terminal use model",
            "The model used when the agent operates inside interactive terminal applications like database shells, debuggers, REPLs, or dev servers--reading live output and writing commands to the PTY.",
            &view.full_terminal_use_model_dropdown,
        ));

        if FeatureFlag::LocalComputerUse.is_enabled() {
            column.add_child(render_filterable_dropdown_row(
                appearance,
                "Computer use model",
                "The model used when the agent takes control of your computer to interact with graphical applications through mouse movements, clicks, and keyboard input.",
                &view.computer_use_model_dropdown,
            ));
        }
    }
```

- [ ] **Step 6: Run compile check**

Run:

```bash
cargo check -p warp --bin warp-oss
```

Expected: compile succeeds.

- [ ] **Step 7: Commit**

```bash
git add app/src/ai/execution_profiles/editor/mod.rs app/src/ai/execution_profiles/editor/ui_helpers.rs
git commit -m "Add Direct API routing controls to profiles"
```

## Task 5: Shared URL Validation and Model-List Normalization

**Files:**
- Create `crates/ai/src/url_validation.rs`
- Modify `crates/ai/src/lib.rs`
- Modify `app/src/settings_view/direct_api_page.rs`
- Modify `crates/ai/src/model_registry/providers/custom.rs`
- Modify `crates/ai/src/model_registry/providers/custom_tests.rs`

- [ ] **Step 1: Add URL validation module**

Create `crates/ai/src/url_validation.rs`:

```rust
use std::net::IpAddr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BaseUrlValidationError {
    InvalidUrl,
    HttpNotLocalOrPrivate,
}

pub fn validate_direct_api_base_url(url: &str) -> Result<(), BaseUrlValidationError> {
    let parsed = reqwest::Url::parse(url).map_err(|_| BaseUrlValidationError::InvalidUrl)?;
    match parsed.scheme() {
        "https" => Ok(()),
        "http" if host_is_local_or_private(&parsed) => Ok(()),
        "http" => Err(BaseUrlValidationError::HttpNotLocalOrPrivate),
        _ => Err(BaseUrlValidationError::InvalidUrl),
    }
}

pub fn normalize_openai_compatible_base_url(url: &str) -> Result<String, BaseUrlValidationError> {
    validate_direct_api_base_url(url)?;
    let trimmed = url.trim_end_matches('/');
    Ok(trimmed.strip_suffix("/v1").unwrap_or(trimmed).to_string())
}

fn host_is_local_or_private(url: &reqwest::Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    let Ok(addr) = host.parse::<IpAddr>() else {
        return false;
    };
    match addr {
        IpAddr::V4(addr) => {
            addr.is_loopback()
                || addr.is_private()
                || addr.octets()[0] == 169 && addr.octets()[1] == 254
        }
        IpAddr::V6(addr) => addr.is_loopback() || addr.is_unique_local(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_https_urls() {
        assert_eq!(validate_direct_api_base_url("https://api.openai.com/v1"), Ok(()));
    }

    #[test]
    fn allows_http_loopback_and_private_lan() {
        assert_eq!(validate_direct_api_base_url("http://localhost:11434"), Ok(()));
        assert_eq!(validate_direct_api_base_url("http://127.0.0.1:11434"), Ok(()));
        assert_eq!(validate_direct_api_base_url("http://192.168.1.10:8080"), Ok(()));
        assert_eq!(validate_direct_api_base_url("http://10.0.0.5:8080"), Ok(()));
        assert_eq!(validate_direct_api_base_url("http://172.16.0.5:8080"), Ok(()));
    }

    #[test]
    fn rejects_prefix_spoof_hosts() {
        assert_eq!(
            validate_direct_api_base_url("http://localhost.evil.test:11434"),
            Err(BaseUrlValidationError::HttpNotLocalOrPrivate)
        );
        assert_eq!(
            validate_direct_api_base_url("http://127.0.0.1.evil.test:11434"),
            Err(BaseUrlValidationError::HttpNotLocalOrPrivate)
        );
    }

    #[test]
    fn normalizes_openai_compatible_base_url_once() {
        assert_eq!(
            normalize_openai_compatible_base_url("https://example.test/v1").unwrap(),
            "https://example.test"
        );
        assert_eq!(
            normalize_openai_compatible_base_url("https://example.test/").unwrap(),
            "https://example.test"
        );
    }
}
```

- [ ] **Step 2: Export module**

Add to `crates/ai/src/lib.rs`:

```rust
pub mod url_validation;
```

- [ ] **Step 3: Replace settings page validation**

In `app/src/settings_view/direct_api_page.rs`, replace the local `is_safe_http_url` function with:

```rust
use ai::url_validation::validate_direct_api_base_url;
```

Replace call sites:

```rust
if validate_direct_api_base_url(&url).is_err() {
    self.set_error(
        "Base URL must use https://, except http:// localhost or private LAN addresses",
        ctx,
    );
    return;
}
```

- [ ] **Step 4: Normalize custom model list URL**

In `crates/ai/src/model_registry/providers/custom.rs`, import:

```rust
use crate::url_validation::normalize_openai_compatible_base_url;
```

Change `CustomListProvider::new`:

```rust
    pub fn new(base_url: String, api_key: Option<String>) -> Self {
        let base_url = normalize_openai_compatible_base_url(&base_url)
            .unwrap_or_else(|_| base_url.trim_end_matches('/').to_string());
        Self {
            api_key,
            base_url,
            client: Client::new(),
        }
    }
```

Keep `list_models` as:

```rust
let url = format!("{}/v1/models", self.base_url);
```

- [ ] **Step 5: Run URL tests**

Run:

```bash
cargo test -p ai url_validation -- --nocapture
```

Expected: all URL validation tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/ai/src/url_validation.rs crates/ai/src/lib.rs app/src/settings_view/direct_api_page.rs crates/ai/src/model_registry/providers/custom.rs crates/ai/src/model_registry/providers/custom_tests.rs
git commit -m "Validate Direct API base URLs by parsing"
```

## Task 6: Provider Adapter Tool and Base URL Hardening

**Files:**
- Modify `crates/ai/src/provider/genai_adapter.rs`
- Modify `crates/ai/src/provider/genai_adapter_tests.rs`

- [ ] **Step 1: Add conversion tests**

Add tests to `crates/ai/src/provider/genai_adapter_tests.rs`:

```rust
#[test]
fn converts_assistant_tool_calls_to_genai_messages() {
    let req = ChatRequest {
        messages: vec![ChatMessage::Assistant {
            text: Some("I need a file".to_string()),
            tool_calls: vec![ToolCall {
                id: "call-1".to_string(),
                name: "ReadFiles".to_string(),
                input: serde_json::json!({"files":[{"name":"Cargo.toml"}]}),
            }],
        }],
        tools: Vec::new(),
        options: ChatOptions::default(),
    };

    let genai_req = convert_to_genai_request(req);

    assert_eq!(genai_req.messages.len(), 1);
}

#[test]
fn converts_user_tool_results_without_dropping_text() {
    let req = ChatRequest {
        messages: vec![ChatMessage::User(vec![
            ContentBlock::ToolResult {
                tool_use_id: "call-1".to_string(),
                content: ToolResultContent::Text("file contents".to_string()),
                is_error: false,
            },
            ContentBlock::Text("continue".to_string()),
        ])],
        tools: Vec::new(),
        options: ChatOptions::default(),
    };

    let genai_req = convert_to_genai_request(req);

    assert_eq!(genai_req.messages.len(), 1);
}
```

If `genai::chat::ChatRequest` does not expose message internals, keep these as compile-proving tests and add a lower-level pure helper:

```rust
fn content_blocks_to_text(blocks: &[ContentBlock]) -> String
```

Then assert that helper output includes both tool result text and user text.

- [ ] **Step 2: Run tests to verify failure or gap**

Run:

```bash
cargo test -p ai genai_adapter -- --nocapture
```

Expected: tests expose the current dropped assistant tool-call and tool-result behavior, or compile forces the helper seam.

- [ ] **Step 3: Preserve tool content in conversions**

In `crates/ai/src/provider/genai_adapter.rs`, change the user message branch to include tool results:

```rust
fn content_blocks_to_text(blocks: &[ContentBlock]) -> String {
    blocks
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text(text) => Some(text.clone()),
            ContentBlock::ToolResult {
                tool_use_id,
                content,
                is_error,
            } => {
                let status = if *is_error { "error" } else { "success" };
                Some(format!(
                    "Tool result for {tool_use_id} ({status}): {}",
                    tool_result_content_to_text(content)
                ))
            }
            ContentBlock::ToolUse { id, name, input } => {
                Some(format!("Tool call {id} {name}: {input}"))
            }
            ContentBlock::Image { .. } => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn tool_result_content_to_text(content: &ToolResultContent) -> String {
    match content {
        ToolResultContent::Text(text) => text.clone(),
        ToolResultContent::Blocks(blocks) => content_blocks_to_text(blocks),
    }
}
```

Use it in `ChatMessage::User`:

```rust
ChatMessage::User(blocks) => {
    genai_messages.push(GenaiChatMessage::user(content_blocks_to_text(&blocks)));
}
```

For `ChatMessage::Assistant { text, tool_calls }`, preserve the call metadata as text until genai exposes a stable cross-provider assistant tool-call message constructor:

```rust
ChatMessage::Assistant { text, tool_calls } => {
    let mut parts = Vec::new();
    if let Some(text) = text {
        parts.push(text);
    }
    for tool_call in tool_calls {
        parts.push(format!(
            "Tool call {} {}: {}",
            tool_call.id, tool_call.name, tool_call.input
        ));
    }
    if !parts.is_empty() {
        genai_messages.push(GenaiChatMessage::assistant(parts.join("\n")));
    }
}
```

- [ ] **Step 4: Run provider tests**

Run:

```bash
cargo test -p ai genai_adapter -- --nocapture
```

Expected: tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/ai/src/provider/genai_adapter.rs crates/ai/src/provider/genai_adapter_tests.rs
git commit -m "Preserve Direct API tool history conversion"
```

## Task 7: Local Direct API Response Stream

**Files:**
- Create `app/src/ai/agent/api/direct.rs`
- Create `app/src/ai/agent/api/direct_tools.rs`
- Modify `app/src/ai/agent/api.rs`
- Modify `app/src/ai/agent/api/impl.rs`
- Modify `app/src/ai/agent/api/impl_tests.rs`

- [ ] **Step 1: Add direct module declarations**

In `app/src/ai/agent/api.rs`, add:

```rust
mod direct;
mod direct_tools;
```

- [ ] **Step 2: Add branch tests**

In `app/src/ai/agent/api/impl_tests.rs`, add tests that prove routing selection:

```rust
#[test]
fn direct_api_routing_requires_route_config() {
    let mut params = minimal_request_params_for_test();
    params.model_routing = ModelRouting::DirectApi;
    params.direct_api_route_config = None;

    let err = super::direct::validate_direct_route(&params).unwrap_err();

    assert_eq!(err.to_string(), "Direct API routing is selected but no Direct API model is configured");
}

#[test]
fn warp_provider_routing_keeps_server_request_path() {
    let mut params = minimal_request_params_for_test();
    params.model_routing = ModelRouting::WarpProvider;
    params.direct_api_route_config = None;

    assert!(!params.model_routing.is_direct_api());
}
```

Extend the existing `request_params_with_ask_user_question_enabled` helper in `app/src/ai/agent/api/impl_tests.rs` with `model_routing: ModelRouting::WarpProvider` and `direct_api_route_config: None`.

- [ ] **Step 3: Implement Direct route validation**

Create `app/src/ai/agent/api/direct.rs`:

```rust
use std::sync::Arc;

use anyhow::anyhow;
use futures::{FutureExt, StreamExt};
use uuid::Uuid;
use warp_multi_agent_api as api;

use super::{Event, RequestParams, ResponseStream};
use crate::ai::agent::api::ConvertToAPITypeError;
use crate::server::server_api::AIApiError;

pub fn validate_direct_route(params: &RequestParams) -> anyhow::Result<()> {
    let Some(config) = &params.direct_api_route_config else {
        anyhow::bail!("Direct API routing is selected but no Direct API model is configured");
    };
    if config.model_id.trim().is_empty() {
        anyhow::bail!("Direct API routing is selected but the selected model is empty");
    }
    match config.provider_id {
        ai::model_registry::ProviderId::Ollama => {}
        ai::model_registry::ProviderId::OpenAI
        | ai::model_registry::ProviderId::Anthropic
        | ai::model_registry::ProviderId::GoogleGemini
        | ai::model_registry::ProviderId::OpenRouter
        | ai::model_registry::ProviderId::Custom => {
            if config.api_key.as_ref().is_none_or(|key| key.trim().is_empty()) {
                anyhow::bail!("Direct API provider requires an API key");
            }
        }
    }
    Ok(())
}

pub async fn generate_direct_api_output(
    params: RequestParams,
    cancellation_rx: futures::channel::oneshot::Receiver<()>,
) -> Result<ResponseStream, ConvertToAPITypeError> {
    if let Err(err) = validate_direct_route(&params) {
        return Ok(single_error_stream(err.to_string()));
    }

    let (tx, rx) = async_channel::unbounded::<Event>();
    let request_id = Uuid::new_v4().to_string();
    let conversation_id = params
        .conversation_token
        .as_ref()
        .map(|token| token.as_str().to_string())
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    tx.send(Ok(api::ResponseEvent {
        r#type: Some(api::response_event::Type::Init(
            api::response_event::StreamInit {
                conversation_id: conversation_id.clone(),
                request_id: request_id.clone(),
                run_id: conversation_id.clone(),
            },
        )),
    }))
    .await
    .ok();

    tokio::spawn(async move {
        let mut cancellation_rx = cancellation_rx.fuse();
        futures::select! {
            _ = cancellation_rx => {
                send_finished(&tx, api::response_event::stream_finished::Reason::Other(
                    api::response_event::stream_finished::Other {},
                )).await;
            }
            result = run_direct_text_stream(params, request_id.clone()).fuse() => {
                match result {
                    Ok(events) => {
                        for event in events {
                            if tx.send(Ok(event)).await.is_err() {
                                return;
                            }
                        }
                        send_finished(&tx, api::response_event::stream_finished::Reason::Done(
                            api::response_event::stream_finished::Done {},
                        )).await;
                    }
                    Err(err) => {
                        let _ = tx
                            .send(Err(Arc::new(AIApiError::Other(anyhow!(err)))))
                            .await;
                    }
                }
            }
        }
    });

    Ok(Box::pin(rx))
}

async fn run_direct_text_stream(
    params: RequestParams,
    request_id: String,
) -> anyhow::Result<Vec<api::ResponseEvent>> {
    let task_id = params
        .tasks
        .first()
        .map(|task| task.id.clone())
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let message_id = Uuid::new_v4().to_string();
    let text = super::direct_tools::run_provider_once(params).await?;

    Ok(vec![api::ResponseEvent {
        r#type: Some(api::response_event::Type::ClientActions(
            api::response_event::ClientActions {
                actions: vec![api::ClientAction {
                    action: Some(api::client_action::Action::AddMessagesToTask(
                        api::client_action::AddMessagesToTask {
                            task_id: task_id.clone(),
                            messages: vec![api::Message {
                                id: message_id,
                                task_id,
                                request_id,
                                timestamp: None,
                                server_message_data: String::new(),
                                citations: Vec::new(),
                                message: Some(api::message::Message::AgentOutput(
                                    api::message::AgentOutput { text },
                                )),
                            }],
                        },
                    )),
                }],
            },
        )),
    }])
}

async fn send_finished(
    tx: &async_channel::Sender<Event>,
    reason: api::response_event::stream_finished::Reason,
) {
    let _ = tx
        .send(Ok(api::ResponseEvent {
            r#type: Some(api::response_event::Type::Finished(
                api::response_event::StreamFinished {
                    reason: Some(reason),
                    token_usage: Vec::new(),
                    should_refresh_model_config: false,
                    request_cost: None,
                    conversation_usage_metadata: None,
                },
            )),
        }))
        .await;
}

fn single_error_stream(message: String) -> ResponseStream {
    let (tx, rx) = async_channel::unbounded();
    tokio::spawn(async move {
        let _ = tx
            .send(Err(Arc::new(AIApiError::Other(anyhow!(message)))))
            .await;
    });
    Box::pin(rx)
}
```

- [ ] **Step 4: Implement provider call helper**

Create `app/src/ai/agent/api/direct_tools.rs`:

```rust
use ai::provider::{ChatMessage, ChatOptions, ChatRequest, ContentBlock, GenaiAdapter, LlmProvider};
use ai::model_registry::ProviderId;

use super::RequestParams;

pub async fn run_provider_once(params: RequestParams) -> anyhow::Result<String> {
    let config = params
        .direct_api_route_config
        .clone()
        .ok_or_else(|| anyhow::anyhow!("Direct API route config missing"))?;
    let provider_name = provider_name(config.provider_id);
    let api_key = config.api_key.unwrap_or_default();
    let adapter = GenaiAdapter::new(provider_name, &api_key, &config.model_id);
    let adapter = if let Some(base_url) = config.base_url.as_deref() {
        adapter.with_base_url(base_url)
    } else {
        adapter
    };

    let prompt = params
        .input
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    let response = adapter
        .chat(ChatRequest {
            messages: vec![ChatMessage::User(vec![ContentBlock::Text(prompt)])],
            tools: Vec::new(),
            options: ChatOptions::default(),
        })
        .await?;

    Ok(response.text.unwrap_or_default())
}

fn provider_name(provider_id: ProviderId) -> &'static str {
    match provider_id {
        ProviderId::OpenAI => "openai",
        ProviderId::Anthropic => "anthropic",
        ProviderId::GoogleGemini => "gemini",
        ProviderId::Ollama => "ollama",
        ProviderId::OpenRouter | ProviderId::Custom => "openai",
    }
}
```

This establishes the local route. Task 8 expands it from text-only provider calls into agentic tool events using the same `ResponseEvent` stream shape.

- [ ] **Step 5: Branch in `generate_multi_agent_output`**

In `app/src/ai/agent/api/impl.rs`, insert after secret redaction and before building `api::Request`:

```rust
    if params.model_routing.is_direct_api() {
        params.api_keys = None;
        return super::direct::generate_direct_api_output(params, cancellation_rx).await;
    }
```

- [ ] **Step 6: Run targeted compile**

Run:

```bash
cargo check -p warp --bin warp-oss
```

Expected: compile succeeds after matching actual enum constructors and imports.

- [ ] **Step 7: Commit**

```bash
git add app/src/ai/agent/api.rs app/src/ai/agent/api/impl.rs app/src/ai/agent/api/direct.rs app/src/ai/agent/api/direct_tools.rs app/src/ai/agent/api/impl_tests.rs
git commit -m "Route Direct API profiles locally"
```

## Task 8: Agentic Tool Event Bridge

**Files:**
- Modify `app/src/ai/agent/api/direct.rs`
- Modify `app/src/ai/agent/api/direct_tools.rs`
- Modify `app/src/ai/agent/api/impl_tests.rs`
- Modify `crates/ai/src/direct_loop/mod.rs`
- Modify `crates/ai/src/direct_loop/run_tests.rs`

- [ ] **Step 1: Add direct tool mapping tests**

Add tests in `app/src/ai/agent/api/impl_tests.rs`:

```rust
#[test]
fn maps_direct_read_files_tool_call_to_proto_tool_call() {
    let tool_call = ai::provider::ToolCall {
        id: "call-read".to_string(),
        name: "ReadFiles".to_string(),
        input: serde_json::json!({
            "files": [{"name": "Cargo.toml"}]
        }),
    };

    let proto = super::direct_tools::provider_tool_call_to_proto(tool_call)
        .expect("tool call should map");

    assert_eq!(proto.tool_call_id, "call-read");
    assert!(matches!(
        proto.tool,
        Some(warp_multi_agent_api::message::tool_call::Tool::ReadFiles(_))
    ));
}

#[test]
fn unknown_direct_tool_call_maps_to_error() {
    let tool_call = ai::provider::ToolCall {
        id: "call-unknown".to_string(),
        name: "UnknownTool".to_string(),
        input: serde_json::json!({}),
    };

    let err = super::direct_tools::provider_tool_call_to_proto(tool_call).unwrap_err();

    assert!(err.to_string().contains("Unsupported Direct API tool"));
}
```

- [ ] **Step 2: Implement provider-to-proto tool mapping**

In `app/src/ai/agent/api/direct_tools.rs`, add:

```rust
pub fn provider_tool_call_to_proto(
    tool_call: ai::provider::ToolCall,
) -> anyhow::Result<warp_multi_agent_api::message::ToolCall> {
    let tool = match tool_call.name.as_str() {
        "ReadFiles" => {
            let files = tool_call
                .input
                .get("files")
                .and_then(|value| value.as_array())
                .into_iter()
                .flatten()
                .filter_map(|file| {
                    file.get("name")
                        .and_then(|name| name.as_str())
                        .map(|name| warp_multi_agent_api::message::tool_call::read_files::File {
                            name: name.to_string(),
                            line_ranges: Vec::new(),
                        })
                })
                .collect();
            warp_multi_agent_api::message::tool_call::Tool::ReadFiles(
                warp_multi_agent_api::message::tool_call::ReadFiles { files },
            )
        }
        "Grep" => {
            let queries = tool_call
                .input
                .get("queries")
                .and_then(|value| value.as_array())
                .into_iter()
                .flatten()
                .filter_map(|query| query.as_str().map(str::to_string))
                .collect();
            let path = tool_call
                .input
                .get("path")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_string();
            warp_multi_agent_api::message::tool_call::Tool::Grep(
                warp_multi_agent_api::message::tool_call::Grep { queries, path },
            )
        }
        "RunShellCommand" => {
            let command = tool_call
                .input
                .get("command")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_string();
            warp_multi_agent_api::message::tool_call::Tool::RunShellCommand(
                warp_multi_agent_api::message::tool_call::RunShellCommand {
                    command,
                    is_read_only: false,
                    uses_pager: false,
                    citations: Vec::new(),
                    is_risky: true,
                    wait_until_complete_value: None,
                    risk_category: 0,
                },
            )
        }
        other => anyhow::bail!("Unsupported Direct API tool: {other}"),
    };

    Ok(warp_multi_agent_api::message::ToolCall {
        tool_call_id: tool_call.id,
        tool: Some(tool),
    })
}
```

- [ ] **Step 3: Emit tool calls as existing client actions**

Update `run_direct_text_stream` in `app/src/ai/agent/api/direct.rs` to collect both assistant text and tool calls from a new helper:

```rust
let output = super::direct_tools::run_provider_turn(params).await?;
let mut messages = Vec::new();
if !output.text.is_empty() {
    messages.push(api::Message {
        id: Uuid::new_v4().to_string(),
        task_id: task_id.clone(),
        request_id: request_id.clone(),
        timestamp: None,
        server_message_data: String::new(),
        citations: Vec::new(),
        message: Some(api::message::Message::AgentOutput(
            api::message::AgentOutput { text: output.text },
        )),
    });
}
for tool_call in output.tool_calls {
    messages.push(api::Message {
        id: Uuid::new_v4().to_string(),
        task_id: task_id.clone(),
        request_id: request_id.clone(),
        timestamp: None,
        server_message_data: String::new(),
        citations: Vec::new(),
        message: Some(api::message::Message::ToolCall(tool_call)),
    });
}
```

Return the same `AddMessagesToTask` action with `messages`.

- [ ] **Step 4: Return provider turn output**

In `direct_tools.rs`, add:

```rust
pub struct DirectProviderTurnOutput {
    pub text: String,
    pub tool_calls: Vec<warp_multi_agent_api::message::ToolCall>,
}

pub async fn run_provider_turn(params: RequestParams) -> anyhow::Result<DirectProviderTurnOutput> {
    let config = params
        .direct_api_route_config
        .clone()
        .ok_or_else(|| anyhow::anyhow!("Direct API route config missing"))?;
    let provider_name = provider_name(config.provider_id);
    let api_key = config.api_key.unwrap_or_default();
    let adapter = GenaiAdapter::new(provider_name, &api_key, &config.model_id);
    let adapter = if let Some(base_url) = config.base_url.as_deref() {
        adapter.with_base_url(base_url)
    } else {
        adapter
    };
    let prompt = params
        .input
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    let response = adapter
        .chat(ChatRequest {
            messages: vec![ChatMessage::User(vec![ContentBlock::Text(prompt)])],
            tools: direct_tool_definitions(),
            options: ChatOptions::default(),
        })
        .await?;
    let tool_calls = response
        .tool_calls
        .into_iter()
        .map(provider_tool_call_to_proto)
        .collect::<anyhow::Result<Vec<_>>>()?;

    Ok(DirectProviderTurnOutput {
        text: response.text.unwrap_or_default(),
        tool_calls,
    })
}

fn direct_tool_definitions() -> Vec<ai::provider::Tool> {
    vec![
        ai::provider::Tool {
            name: "ReadFiles".to_string(),
            description: "Read one or more files from the current workspace.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "files": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "name": {"type": "string"}
                            },
                            "required": ["name"]
                        }
                    }
                },
                "required": ["files"]
            }),
        },
        ai::provider::Tool {
            name: "Grep".to_string(),
            description: "Search files for text or regex patterns.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "queries": {"type": "array", "items": {"type": "string"}},
                    "path": {"type": "string"}
                },
                "required": ["queries"]
            }),
        },
        ai::provider::Tool {
            name: "RunShellCommand".to_string(),
            description: "Request execution of a shell command after Warp permission checks.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {"type": "string"}
                },
                "required": ["command"]
            }),
        },
    ]
}
```

- [ ] **Step 5: Fix direct loop ordering**

In `crates/ai/src/direct_loop/mod.rs`, replace the partitioning branch with order-preserving classification:

```rust
        let requires_serial = batch_requires_confirmation(&tool_calls);
        let mut results: Vec<(usize, ContentBlock)> = Vec::new();

        if requires_serial {
            for (i, tc) in tool_calls.into_iter().enumerate() {
                let block = dispatch_one(tc, i, conversation_id, &tool_req_tx).await?;
                results.push(block);
            }
        } else {
            use futures::stream::FuturesUnordered;

            let mut pending: FuturesUnordered<_> = tool_calls
                .into_iter()
                .enumerate()
                .map(|(i, tc)| {
                    let tool_req_tx = tool_req_tx.clone();
                    async move { dispatch_one(tc, i, conversation_id, &tool_req_tx).await }
                })
                .collect();
```

- [ ] **Step 6: Run tool tests**

Run:

```bash
cargo test -p warp maps_direct_read_files_tool_call_to_proto unknown_direct_tool_call_maps_to_error -- --nocapture
cargo test -p ai direct_loop -- --nocapture
```

Expected: tests pass and direct loop preserves original tool-call order.

- [ ] **Step 7: Commit**

```bash
git add app/src/ai/agent/api/direct.rs app/src/ai/agent/api/direct_tools.rs app/src/ai/agent/api/impl_tests.rs crates/ai/src/direct_loop/mod.rs crates/ai/src/direct_loop/run_tests.rs
git commit -m "Bridge Direct API tool calls into agent events"
```

## Task 9: Final Validation and Documentation

**Files:**
- Modify `docs/superpowers/specs/2026-05-15-direct-api-profile-routing-design.md` if implementation behavior differs.
- Modify `docs/features/direct-api-*.md` if they mention profile routing.

- [ ] **Step 1: Run formatting and whitespace checks**

Run:

```bash
cargo fmt --check
git diff --check
```

Expected: both pass.

- [ ] **Step 2: Run targeted tests**

Run:

```bash
cargo test -p ai api_keys::tests -- --nocapture --test-threads=1
cargo test -p ai url_validation -- --nocapture
cargo test -p ai genai_adapter -- --nocapture
cargo test -p ai direct_loop -- --nocapture
cargo test -p warp direct_api_choices_ -- --nocapture
cargo test -p warp execution_profile_ -- --nocapture
```

Expected: all targeted tests pass.

- [ ] **Step 3: Run OSS compile**

Run:

```bash
cargo check -p warp --bin warp-oss
```

Expected: compile passes for the OSS binary.

- [ ] **Step 4: Manual UI validation**

Run the OSS app:

```bash
cargo run -p warp --bin warp-oss
```

Manual checks:

- Open Settings -> Agents -> Profiles.
- Confirm `Model Routing` appears above the base model controls.
- Select `Warp Provider`; confirm existing base model, context window, full terminal use, and computer use controls remain.
- Select `Direct API`; confirm the Direct API model picker appears as `Provider / Model`.
- Select a Direct API model; close and reopen the profile editor; confirm the selection persists.
- Confirm `~/.warp-oss/settings.toml` is used for Direct API configuration and official `~/.warp` is not modified.

- [ ] **Step 5: Security review search**

Run:

```bash
rg -n "api_key|Authorization|Bearer|direct_api_route_config|log::|telemetry" app/src crates/ai/src crates/settings/src
```

Expected:

- No Direct API key values are logged.
- Direct API branch sets `params.api_keys = None` before local routing.
- Direct API settings continue through `DirectAPISettings`.

- [ ] **Step 6: Commit validation/doc updates**

```bash
git add docs app/src crates/ai/src crates/settings/src
git commit -m "Validate Direct API profile routing"
```

If there are no doc changes after validation, skip this commit.

## Self-Review

- Spec coverage:
  - Per-profile routing is covered by Tasks 1, 2, and 4.
  - `Provider / Model` Direct API choices are covered by Tasks 3 and 4.
  - Local Direct API branch and no server key forwarding are covered by Task 7.
  - Tool-call, ordering, and cancellation coverage are covered by Tasks 7 and 8.
  - URL parsing and custom `/v1` normalization are covered by Task 5.
  - Provider conversion hardening is covered by Task 6.
  - OSS settings and validation are covered by Task 9.
- Placeholder scan:
  - The plan avoids open-ended implementation wording and gives exact files, snippets, commands, and expected results.
- Type consistency:
  - `ModelRouting`, `DirectApiProfileModelSelection`, `DirectApiRouteConfig`, and `DirectApiModelChoice` names are consistent across tasks.
  - `ProviderId` remains the canonical provider identifier.
  - Direct API route fields live on `RequestParams` so `generate_multi_agent_output` can branch without needing `AppContext`.
