# Direct API Profile Routing Design

## Summary

Add per-profile model routing to Settings -> Agents -> Profiles so a profile can use either Warp Provider models or Direct API models.

The control appears above the existing model controls:

```text
Model Routing: [ Warp Provider ] [ Direct API ]
Direct API:    [ Provider / Model ]
```

This is for `warp-oss`. Direct API configuration remains local and channel-scoped through `DirectAPISettings`, persisted in the active `settings.toml` path. For the macOS OSS build, that is `~/.warp-oss/settings.toml`.

## Goals

- Let each execution profile choose model routing independently.
- Keep the existing Warp Provider model selection working unchanged.
- Let Direct API routing use configured Direct API providers and model-list cache entries.
- Display Direct API choices as `Provider / Model`.
- Route Agent Mode locally through Direct API when the active profile selects Direct API.
- Avoid sending Direct API keys to Warp server APIs in the Direct API route.
- Preserve tool-call, cancellation, and action-confirmation semantics for agentic use.

## Non-Goals

- Do not replace all Warp Provider model infrastructure.
- Do not move Direct API secrets out of `~/.warp-oss/settings.toml`.
- Do not make Direct API the implicit default for all profiles.
- Do not add cloud sync for Direct API credentials.
- Do not solve unrelated ambient-agent, cloud task, or hosted-agent flows in this change.

## User Experience

The profile editor adds a routing row before "Base model".

When `Warp Provider` is selected:

- Existing model dropdowns remain visible.
- Existing model metadata, context-window behavior, and upgrade messaging stay unchanged.
- Agent requests continue through the current Warp provider route.

When `Direct API` is selected:

- The base model area switches to a Direct API model picker.
- Choices are shown as `Provider / Model`, for example:
  - `OpenAI / gpt-4o-mini`
  - `Anthropic / claude-3-5-sonnet-20241022`
  - `Ollama / llama3.2`
  - `OpenRouter / anthropic/claude-3.5-sonnet`
  - `Custom / local-model`
- If no Direct API provider/model is ready, show an empty or disabled state with a concise path to Settings -> Agents -> Direct API.
- Context-window controls are hidden for Direct API unless the selected model has known local capability metadata.
- Full terminal use and computer-use model controls stay Warp Provider-only for the first implementation unless Direct API support is explicitly added for those roles.

The UI should be dense and consistent with the existing profile editor. Use existing dropdown and settings row patterns. Avoid a modal or separate Direct API settings page jump for normal selection.

## Profile Data Model

Extend `AIExecutionProfile` with:

```rust
pub model_routing: ModelRouting,
pub direct_api_model: Option<DirectApiProfileModelSelection>,
```

Add serializable types near the execution profile model:

```rust
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelRouting {
    #[default]
    WarpProvider,
    DirectApi,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectApiProfileModelSelection {
    pub provider_id: ProviderId,
    pub model_id: String,
}
```

Fallback behavior:

- Missing `model_routing` deserializes to `WarpProvider`.
- `Unknown` behaves as `WarpProvider`.
- Missing `direct_api_model` keeps the Direct API route disabled until the user chooses a model or a valid default can be resolved from Direct API settings.

## Direct API Model Choices

Build model choices from existing local Direct API sources:

- `ApiKeyManager::keys(ctx)` for selected provider, configured API keys, base URLs, and selected models.
- `ModelListCache` for cached provider model lists.
- Provider defaults from `ApiKeyManager::get_selected_model_for_provider` only where the provider has a safe known default.

Choice labels use `Provider / Model`. The value stores provider ID plus raw model ID.

Providers with no cached list may still show the saved selected model as a stale/manual option so an existing working configuration remains selectable after restart.

## Request Routing

The branch point is `app/src/ai/agent/api/impl.rs::generate_multi_agent_output`.

When the active profile uses `WarpProvider`, preserve the current behavior:

- convert request params to `warp_multi_agent_api::Request`
- call `ServerApi::generate_multi_agent_output`
- stream response events back to the existing response stream model

When the active profile uses `DirectApi`, use a local Direct API route:

- resolve the active profile's Direct API provider/model
- validate that required key and base URL values are present
- construct the local provider adapter
- convert the current request input into local `ChatMessage` values
- run the local direct agent loop
- convert local agent events back into the existing `ResponseStream` event shape used by the UI
- keep cancellation wired to the same cancellation receiver

The Direct API route must not attach Direct API keys to a Warp server request.

## Agentic Tool Contract

The local route must follow the provider tool-use loop:

1. Send user/system messages plus available tool definitions to the provider.
2. Receive assistant text and tool calls.
3. Execute approved tool calls through Warp's existing action machinery.
4. Send tool results back to the provider with the correct tool-call IDs.
5. Continue until the provider returns a final response.

Implementation must preserve:

- text chunk order
- tool-call ID, name, argument, and result mapping
- cancellation
- confirmation for side-effecting or unknown tools
- no stale events after cancellation or retry

Provider references:

- OpenAI function calling: https://developers.openai.com/api/docs/guides/function-calling
- Anthropic tool use: https://platform.claude.com/docs/en/agents-and-tools/tool-use/how-tool-use-works
- Ollama OpenAI compatibility: https://docs.ollama.com/api/openai-compatibility
- Google Gemini function calling: https://docs.cloud.google.com/vertex-ai/generative-ai/docs/multimodal/function-calling

## Security and Local-First Requirements

- Direct API keys stay in `DirectAPISettings` under `settings.toml`.
- Direct API keys are never sent to Warp server APIs on the Direct API route.
- HTTP base URLs are accepted only for localhost or private LAN IPs.
- URL validation must parse URLs and IPs, not rely on string prefixes.
- Logs and telemetry must not include API keys, bearer tokens, custom model names from private endpoints, or full custom URLs where those may expose internal hostnames.
- Telemetry for Direct API routing should use provider enum values and coarse success/failure categories only.

## Error Handling

User-visible errors should be direct and actionable:

- Direct API routing selected but no provider/model configured.
- Provider requires an API key and none is saved.
- Provider requires a base URL and none is saved.
- Base URL is invalid or not allowed.
- Model list is empty or stale.
- Provider request failed due to auth, rate limit, network, or parse errors.

Do not silently fall back from Direct API to Warp Provider after the user explicitly selected Direct API. Show an error instead.

## Testing Plan

The implementation plan should include:

- Profile serialization tests for default `WarpProvider`, explicit `DirectApi`, and unknown routing values.
- Profile editor tests for routing toggle state and model dropdown contents.
- Direct API model choice tests for cached, stale/manual, and empty states.
- URL validation tests using parsed localhost, loopback IP, RFC1918 IP, and prefix-spoof hostnames.
- Provider adapter tests proving custom base URLs do not double-append `/v1`.
- Genai adapter tests proving assistant tool calls and tool results survive round trips.
- Direct loop tests for tool-call ordering, cancellation, confirmation, and malformed tool JSON.
- App compile check for OSS: `cargo check -p warp --bin warp-oss`.

## Open Decisions

- Direct API routing is per execution profile.
- Direct API controls initially apply to the base Agent Mode model.
- Warp Provider remains the default for existing and new profiles.
- Direct API does not silently fall back to Warp Provider when selected.
