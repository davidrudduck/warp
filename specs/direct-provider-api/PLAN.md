# Direct Provider API — Implementation Plan

**Status**: Revised after third adversarial review (Opus/Codex/Gemini)  
**Date**: 2026-05-08  
**Scope**: Add client-side direct-to-provider LLM API support to the Warp terminal, bypassing the Warp backend for AI requests when configured.

---

## Adversarial Review History

### First Review (Opus/Codex/Gemini — Round 1)

| Rating | Count | Examples |
|---|---|---|
| PLAN_IS_WRONG | 6 | Dispatch branch location, `genai` availability, tool reusability, settings nav path, `LLMPreferences`, tool reuse |
| CRITICAL | 4 | `genai` beta/unmaintained risk, char/4 tokenization, privacy/legal gate missing, undefined types |
| BLOCKS_COMPILATION | 3 | `ProviderError` undefined, `UserContent`/`AssistantContent` undefined, `&dyn` wrong ownership shape |
| PLAN_MISSED | 5 | Existing `SecretStore` infra, `SoloUserByok` side effects, feature parity gap, `kSecAttrAccessGroup` moot |
| MAJOR | 8 | Phase ordering, cancellation contract, tool-pair trimming, DB connection pool, two-loop maintenance tax |
| PLAN_UNDERESTIMATES | 3 | Timeline (7→14 wks), Diesel coupling, settings UI complexity |

### Second Review (Opus/Codex/Gemini — Round 2)

| Rating | Count | Key Findings |
|---|---|---|
| PLAN_IS_WRONG / CRITICAL | 2 | `LLMPreferences` exists at `llms.rs:547`; dispatch must branch before protobuf, not at line 139 |
| BLOCKS_COMPILATION | 2 | `BlocklistAIActionExecutor::try_to_execute_action` is sync/requires `&mut ModelContext` — `.await?` pseudocode impossible; `futures::channel::oneshot::Receiver` cannot be polled in multi-iteration loop as written |
| PLAN_IS_WRONG | 4 | Cannot `select!` on `ChatStream` directly; no `SqlitePool` in `crates/persistence`; `api/mod.rs` doesn't exist (file is `api.rs`); `AIAgentActionResultType` at line 966 is a wrapper struct |
| CRITICAL | 2 | Dogfood without rate-limit for 4 weeks risks user accounts; conversation history decision affects Phase 1–3 types |
| PLAN_MISSED | 5 | Tool adapter layer; parallel tool dispatch; `count_tokens` RTT; prompt caching cost gap; `secrecy` crate not in Cargo.lock |
| MAJOR | 6 | No convergence plan for two loops; wiremock TLS handwave; `genai` gate lacks criteria; profile-toggle granularity; Phase 3/4 interlock; MCP boundary |

**Revised timeline: 17–20 weeks** (was 14–16).

### Third Review (Opus/Codex/Gemini — Round 3)

| Rating | Count | Key Findings |
|---|---|---|
| BLOCKS_COMPILATION | 3 | `AsyncAppContext` does not exist in codebase — entire ToolDispatcher pattern is wrong; `AgentEventSender`/`FusedCancelSignal`/`TurnResult`/`TransportError` undefined; `AIAgentActionResultType` enum absent from codebase |
| PLAN_IS_WRONG | 5 | `LLMPreferences::available_llms` does not exist; `try_to_execute_action` return value ignored (hangs on `NeedsConfirmation`); `tracing::` violates CLAUDE.md §5; `AISubpage` variant names wrong; `ApiKeys::get_*` methods don't exist (only `keys()`) |
| CRITICAL | 3 | `NeedsConfirmation` + `FuturesUnordered` breaks Anthropic transcript (orphaned tool-use IDs); `SoloUserByok` Cargo feature × `DirectApiCalls` runtime flag needs explicit decision matrix and build guard; `conversation_id` missing from `direct_loop::run()` signature |
| MAJOR | 3 | `AgentTransport` trait needs `#[async_trait]`; two-loop drift needs enforcement mechanism; profile blast-radius needs per-session override |

**Revised timeline: 18–21 weeks** (+1 week for ToolDispatcher redesign, Phase 0 research tasks added).

---

## 1. Context & Motivation

Warp currently routes all AI agent requests through `https://app.warp.dev/ai/multi-agent` as a protobuf payload with SSE response. The client never contacts OpenAI, Anthropic, Google, or Ollama directly.

This creates three user problems:
1. **BYOK is plan-gated**: `SoloUserByok` feature flag (`crates/warp_features/src/lib.rs:813`) already exists and is wired to the BYOK gate in `user_workspaces.rs:490`, but is not enabled in `DOGFOOD_FLAGS`. Enabling it currently causes API keys to be forwarded to the Warp server inside protobuf — it does not produce client-side dispatch.
2. **Local LLMs are impossible**: Ollama, llama.cpp, LM Studio, and other `localhost` providers cannot be backend-routed.
3. **Provider choice is Warp-controlled**: Users cannot use providers, models, or endpoints not approved by Warp's server-side model list.

---

## 2. Goals

1. Users can configure OpenAI, Anthropic, Google, Ollama, and any OpenAI-compatible endpoint (OpenRouter, LiteLLM, llama.cpp, vLLM, LM Studio)
2. AI agent requests can be dispatched directly from the client to the configured provider
3. Model lists are always current — fetched dynamically from each provider's API on first AI request (not on launch), with lifecycle tracking
4. API key storage builds on existing `ApiKeyManager` + `warpui_extras::secure_storage` infrastructure — no new parallel storage system
5. The full agent loop works correctly in direct-dispatch mode, with explicit feature parity documentation
6. The feature is gated behind `FeatureFlag::DirectApiCalls`, rolled out through dogfood → preview → stable
7. Secret redaction and privacy safeguards are in place **before** any dogfood release
8. All logic is covered by tests written before implementation (TDD — with correct phase ordering per §7)

## 3. Non-Goals (V1)

- Replacing Warp's backend-routed path — both paths coexist; user selects
- Feature parity with backend path on day one — see §4.3 compatibility matrix
- Multi-tier secret storage UI (OS keychain + env var only in V1)
- Multiple simultaneous active providers (single active provider per task type)
- WASM target support for direct dispatch

---

## 4. Pre-Implementation Gates

These must be resolved before Phase 1 begins.

### 4.1 Privacy & Legal Review (BLOCKING)

Direct dispatch sends user terminal context and conversation content to third-party provider APIs without routing through Warp's data pipeline. The existing `should_redact_secrets` flag (`app/src/ai/agent/api.rs:235`) and `EnterpriseSecretRegex` (`app/src/workspaces/workspace.rs`) apply to the backend-routed path only.

Required before Phase 1:
- Privacy team sign-off on direct egress to provider APIs
- Determination of whether Warp ToS needs update for "facilitating" PII transfer to third-party processors
- Explicit policy on what gets logged/redacted on the direct path
- Enterprise customer notification plan for `EnterpriseSecretRegex` gap

### 4.2 Provider Client Library Evaluation (BLOCKING)

The `genai` crate (`0.6.0-beta.19`) is the proposed primary adapter but:
- **Not in `Cargo.lock`** — must be added; may conflict with Warp's pinned `reqwest = "0.12.28"` with `default-features = false` and custom TLS flags
- Beta version with single maintainer
- No license review performed
- No security audit performed

**Pass/fail criteria (all must pass to adopt `genai`):**
- License: MIT / Apache-2.0 / BSD only — copyleft = reject
- Anthropic tool-result fidelity: 100% round-trip on a fixture set of 20 real Anthropic responses with multi-block tool calls and parallel tool use
- Abandonment contingency: estimated replacement cost ≤ 4 person-weeks; otherwise hand-roll from the start
- reqwest TLS feature compatibility: must not introduce duplicate `rustls` or `ring`/`aws-lc-rs` conflicts into the workspace dep graph

**Decision default:** If any criterion fails, hand-roll OpenAI + Anthropic adapters on the existing `reqwest` client (estimated 3–4 person-weeks; covers ~80% of users). Add `genai` only if all criteria pass.

**Alternative framing:** Tests use plain-HTTP wiremock servers; production providers use HTTPS base URLs. The `LlmProvider` trait exposes `with_base_url(url: &str)` so test suites inject mock URLs without any TLS dependency. This means the "TLS conflict" between wiremock and reqwest is a category error — tests never need TLS for the mock layer.

### 4.3 Feature Compatibility Matrix

The existing backend-routed path carries ~30 parameters in `RequestParams` (`app/src/ai/agent/api.rs:157–337`). Direct dispatch silently drops or breaks all of them unless explicitly ported. This matrix must be approved before Phase 1:

| Feature | Backend path | Direct path V1 | Notes |
|---|---|---|---|
| MCP servers | ✓ | ✗ V1 | `CallMCPToolExecutor` already inside `BlocklistAIActionExecutor`; interface boundary committed in Phase 4, V2 delivery |
| Computer Use | ✓ | ✗ | Requires image tool results (Anthropic-only format) |
| Orchestration / multi-agent | ✓ | ✗ | `parent_agent_id`, `orchestration_enabled` |
| Conversation forking | ✓ | ✗ | `forked_from_conversation_token` |
| Drive context | ✓ | ✗ | `warp_drive_context_enabled` |
| Ambient agents | ✓ | ✗ | `ambient_agent_task_id` |
| Memory | ✓ | ✗ | `is_memory_enabled` |
| Web search | ✓ | ✗ | `web_search_enabled` |
| Research agent | ✓ | ✗ | `research_agent_enabled` |
| Prompt caching | ✓ (Anthropic) | Phase 4 | 5-10× cost difference — must ship in Phase 4 or require explicit UI cost warning |
| Autonomy levels | ✓ | ✗ | `autonomy_level`, `isolation_level` |
| Shell execution | ✓ | Partial | Via `BlocklistAIActionExecutor` bridge (§5.4) |
| File reads | ✓ | Partial | Same |
| Conversation history | Server-managed | In-memory V1 | See §4.5 — no persistence across restarts in V1 |
| Telemetry / analytics | ✓ | Partial | Provider kind + model ID + token counts only |
| Credit tracking | ✓ | ✗ | Provider bills user directly |
| Sentry error reporting | ✓ | Partial | Must not include API keys or terminal content |

The UI must communicate unsupported features clearly when direct mode is active. Direct mode is intentionally a second-class citizen for the feature set above; see §12 for the convergence roadmap.

### 4.4 Threat Model

Required document before Phase 1 (scoped threat model, not a full security audit):
- What does the in-process API key store protect against? (disk theft, remote code via deps, malicious MCP servers, `task_for_pid`)
- What does it explicitly NOT protect against? (plain `String` in `ApiKeys` struct — keys are not zeroed on drop in current implementation; `secrecy` crate is **not in `Cargo.lock`** and must not be assumed; if zeroize-on-drop is required, add `secrecy` explicitly with a dependency evaluation)
- What is the attacker model for a Warp user running untrusted MCP tools?

### 4.5 Conversation Persistence Decision (BLOCKING)

This decision affects Phase 1 type design, Phase 2 schema, Phase 3 UI, and Phase 4 dispatch. Deferring to Phase 4 forces rework across all prior phases.

The server path uses `conversation_token: Option<ServerConversationToken>` — the server holds canonical history. The direct path has no server-side memory.

**Options:**

| Path | Durable | Schema impact | V1 recommendation |
|---|---|---|---|
| A: In-memory only | No — lost on close/crash | None | ✓ Recommended for V1 |
| B: Client-side SQLite | Yes | New conversation + message tables; 1.5–2 wk migration | V2 |
| C: Hybrid | Fragmented | Worst of both | Reject |

**Decision: Path A (in-memory) for V1 dogfood.** The UI must display a persistent non-modal banner when direct mode is active: *"Direct-mode conversations are not saved across app restarts in V1."* Path B is the V2 target; its Diesel schema must not be preemptively built in Phase 2.

---

## 5. Architecture

### 5.1 Dispatch Path

**File path correction:** `RequestParams` is defined in `app/src/ai/agent/api.rs` (a flat file). There is no `api/mod.rs`. All references to `api/mod.rs` in earlier plan versions were wrong.

**Correct branch location:** The dispatch decision must be made at the **top** of `generate_multi_agent_output` in `app/src/ai/agent/api/impl.rs`, **before** the protobuf `api::Request` is constructed. By line 63 in that function, `params.input` has already been converted via `convert_input(params.input)?` into `api::request::Input`, and by line 139, the full protobuf `api::Request` is assembled. Branching at line 139 means the direct path receives a protobuf struct it cannot use.

The direct path must consume `params: RequestParams` directly (it contains `input: Vec<AIAgentInput>`, `model: LLMId`, `mcp_context`, `api_keys`, etc. already typed).

```text
agent/api/impl.rs::generate_multi_agent_output(params: RequestParams, ...)
    │
    ├── [BEFORE protobuf construction, ~line 53]
    │
    ├── FeatureFlag::DirectApiCalls.is_enabled()
    │   && provider_config.is_some()
    │       └── direct_loop::run(
    │               provider,
    │               messages_from_params(&params),  // convert input to ChatMessage
    │               tools_from_params(&params),      // derive tool list
    │               conversation_id,                 // AIConversationId — passed in
    │               tx,
    │               dispatcher,                      // ToolDispatcher — created by caller
    │               cancellation_rx,
    │           )
    │
    └── (default) build api::Request from params → server_api.generate_multi_agent_output(&request)
                  [unchanged]
```

### 5.2 Provider Abstraction

**`async_trait` requirement:** `LlmProvider` uses `async fn` in a trait and is used as `Arc<dyn LlmProvider>`. Native `async fn` in traits is not dyn-compatible in Rust 1.92 without `async-trait`. The workspace already declares `async-trait = "0.1.89"` (`Cargo.toml:109`) — use it.

```rust
// crates/ai/src/provider/mod.rs

/// Owned Arc so the provider can be moved into spawned async tasks.
/// Send + Sync + 'static required by Warp's ctx.spawn(future, callback) pattern.
#[async_trait]
pub trait LlmProvider: Send + Sync + 'static {
    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse, ProviderError>;
    async fn chat_stream(&self, req: ChatRequest) -> Result<ChatStream, ProviderError>;
    fn capabilities(&self) -> &ModelCapabilities;
    fn provider_kind(&self) -> ProviderKind;
    /// Injects a base URL override — used in tests to point at HTTP mock servers.
    /// No TLS configuration needed for test environments.
    fn with_base_url(self, url: &str) -> Self where Self: Sized;
}

pub type SharedProvider = Arc<dyn LlmProvider>;
```

Note: `with_base_url` has `where Self: Sized` and is excluded from the vtable — calling it through `dyn LlmProvider` is not possible and not needed. Tests call it on the concrete type before boxing.

```rust
pub enum ProviderKind {
    OpenAI,
    Anthropic,
    Google,
    Ollama,
    OpenAICompatible { label: String, base_url: String },
}
```

Content block types (required for Anthropic multimodal and tool call correctness):

```rust
pub enum ContentBlock {
    Text(String),
    Image { media_type: ImageMediaType, data: String }, // base64
    ToolUse { id: String, name: String, input: serde_json::Value },
    ToolResult { tool_use_id: String, content: ToolResultContent, is_error: bool },
}

pub enum ToolResultContent {
    Text(String),
    Blocks(Vec<ContentBlock>), // for image results (Computer Use)
}

pub enum ChatMessage {
    System(String),
    User(Vec<ContentBlock>),
    Assistant { text: Option<String>, tool_calls: Vec<ToolCall> },
    // ToolResult is encoded as User(vec![ContentBlock::ToolResult{...}]) for Anthropic
    // and as separate role-"tool" messages for OpenAI — the adapter handles wire format
}
```

Streaming events:

```rust
pub enum StreamEvent {
    Start,
    TextChunk(String),
    // args_fragment accumulates per index; adapter merges before emitting ToolCallReady
    ToolCallChunk { index: usize, id: String, name: String, args_fragment: String },
    ToolCallReady(ToolCall), // emitted when args JSON is complete
    End { finish_reason: FinishReason, usage: Option<TokenUsage> },
}

pub type ChatStream = Pin<Box<dyn Stream<Item = Result<StreamEvent, ProviderError>> + Send>>;
```

Error type (required before any other code compiles):

```rust
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("authentication failed: {0}")]
    Auth(String),
    #[error("HTTP {status}: {body}")]
    Http { status: u16, body: String },
    /// For errors embedded in SSE stream after a 200 OK response.
    /// Many providers send errors in-band (Anthropic `error` events, OpenAI `response.failed`).
    #[error("provider stream error: {message}")]
    Remote { provider: String, code: Option<String>, message: String },
    #[error("rate limited; retry after {retry_after_secs:?}s")]
    RateLimited { retry_after_secs: Option<u64> },
    #[error("service unavailable")]
    ServiceUnavailable,
    #[error("context length exceeded")]
    ContextLengthExceeded,
    #[error("transport error: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("stream parse error: {0}")]
    StreamParse(String),
    #[error("cancelled")]
    Cancelled,
    #[error("model not supported: {0}")]
    UnsupportedModel(String),
}
```

### 5.3 Secret Storage

**Do not create a new `SecretStore`.** Extend the existing infrastructure:

- `warpui_extras::secure_storage` (`crates/warpui_extras/src/secure_storage/`) — already handles macOS Keychain via `security_framework`, Linux, and Windows
- `ApiKeyManager` (`crates/ai/src/api_keys.rs`) — already stores Google/Anthropic/OpenAI/OpenRouter keys under `"AiApiKeys"` keychain entry, already emits `ApiKeyManagerEvent::KeysUpdated`

The existing `SecKeychain::default()` lookup uses the service name, not the binary hash — it already survives app updates without re-prompting.

**Actual `ApiKeys` struct** (verified at `crates/ai/src/api_keys.rs:20`):
```rust
pub struct ApiKeys {
    pub google: Option<String>,
    pub anthropic: Option<String>,
    pub openai: Option<String>,
    pub open_router: Option<String>,
}
```
Setter pattern: `set_google_key(key: Option<String>, ctx: &mut ModelContext<Self>)` per field. Getter: `keys() -> &ApiKeys` returns the whole struct — there are no individual `get_*` methods. New provider fields follow the same pattern.

**What to add**: New fields on `ApiKeys` struct (one per new provider) and corresponding `set_*` methods on `ApiKeyManager`. For V1: `ollama_url: Option<String>`, `openai_compatible_endpoint: Option<OpenAICompatibleEndpoint>`.

**Storage tiers for V1**: OS keychain (existing) + env var override (already supported). No app passphrase tier, no plaintext tier — defer to V2 if users request.

**Key access pattern for async dispatch**: Extract owned `String` values from `ApiKeyManager` before entering the background spawn future. Never hold a reference across an async boundary. Note: `secrecy::Secret<String>` (zeroize-on-drop) is **not currently in `Cargo.lock`** — if zeroize-on-drop semantics are required, `secrecy` must be explicitly evaluated and added as a dependency (see §4.4 threat model).

### 5.4 Tool Dispatch

**The existing tool implementations are not directly reusable as async functions.** `BlocklistAIActionExecutor::try_to_execute_action` (`app/src/ai/blocklist/action_model/execute.rs:543`) is:
- **Synchronous** — no `async`/`await`
- Requires `&mut self` AND `&mut ModelContext<Self>` — must run on the WarpUI app thread
- Returns `TryExecuteResult { ExecutedSync | ExecutedAsync | NotExecuted{..} | NeedsConfirmation }`, not a `Result<String, ProviderError>`
- Async results arrive via `BlocklistAIActionExecutorEvent::FinishedAction` event emission (`execute.rs:589`)

**`AsyncAppContext` does not exist.** The plan previously used `ctx: &AsyncAppContext` and `ctx.update(|ctx| { ... })` — this type is absent from the codebase (zero grep hits). WarpUI's actual concurrency model is `ctx.spawn(future, callback)` where the future runs in background (`'static + Send`) and the callback runs on the main thread with `&mut ModelContext<T>`.

**Corrected architecture — channel bridge pattern:**

The direct loop runs as a background task with NO entity access. Tool dispatch crosses the app-thread boundary via an `mpsc` channel:

```text
Background future (direct_loop_future)
    │
    │  Sends: ToolDispatchRequest { action, result_tx: oneshot::Sender<ToolResult> }
    ▼
mpsc::Sender<ToolDispatchRequest>  ──→  mpsc::Receiver<ToolDispatchRequest>
                                                │
                                                ▼ (main-thread, inside WarpUI model)
                                    DirectLoopModel::process_tool_request()
                                        ├── executor.update(ctx, |executor, ctx| {
                                        │       register result_tx keyed by action.id
                                        │       try_to_execute_action(action, ...)
                                        │   })
                                        └── on FinishedAction event → routes to result_tx
                                                │
                                    oneshot::Sender<ToolResult>  ──→  result_rx.await
                                                                         │
                                                                    Background future
                                                                    (continues loop)
```

**`DirectLoopModel`** — a new thin WarpUI model that:
1. Holds `mpsc::Receiver<ToolDispatchRequest>` and `ModelHandle<BlocklistAIActionExecutor>`
2. Processes incoming tool requests by calling `try_to_execute_action` on the executor
3. Subscribes to `BlocklistAIActionExecutorEvent::FinishedAction` to route results to the correct `oneshot::Sender`
4. Is spawned by `generate_multi_agent_output` when the direct path is chosen

**`try_to_execute_action` return value — must not be ignored:**

```rust
match executor.try_to_execute_action(action, conversation_id, false, ctx) {
    TryExecuteResult::ExecutedSync => {
        // FinishedAction will fire as a deferred effect — result_tx already registered
    }
    TryExecuteResult::ExecutedAsync => {
        // FinishedAction will fire later — result_tx already registered
    }
    TryExecuteResult::NeedsConfirmation => {
        // See NeedsConfirmation handling below — result_tx awaits confirmation result
    }
    TryExecuteResult::NotExecuted { reason, action } => {
        // Remove result_tx and send Err immediately
        let _ = result_tx.send(Err(DispatchError::NotExecuted(reason)));
    }
}
```

**`NeedsConfirmation` rule — serialize all tool dispatch when any tool needs confirmation:**

Anthropic and OpenAI both require ALL tool-use IDs from an assistant turn to be answered with corresponding tool-result blocks before the next turn. Partial completion (some tools done, one awaiting confirmation) is not legal at the wire format level.

Rule: **Before dispatching any tool calls concurrently, pre-classify the batch.** If any call would require confirmation, the entire batch is paused and dispatched sequentially behind the confirmation modal. Only after all confirmations are resolved does the batch run.

```rust
// Before FuturesUnordered dispatch:
let (needs_confirm, can_dispatch): (Vec<_>, Vec<_>) = tool_calls
    .iter()
    .partition(|tc| requires_confirmation(&tc.name));

if !needs_confirm.is_empty() {
    // Serialize: prompt for each, then dispatch in order
    for tc in needs_confirm.iter().chain(can_dispatch.iter()) {
        let result = dispatch_with_confirmation(tc, &dispatcher, &tool_req_tx).await?;
        results.push(result);
    }
} else {
    // Safe to dispatch concurrently
    // ... FuturesUnordered path (see §5.5)
}
```

`requires_confirmation` is a static function mapping tool names to confirmation requirements — maintained alongside the tool adapter layer (§5.4.1).

**Phase 0 research required (blocking Phase 4 design):**
- Find the WarpUI mechanism for a model to receive mpsc messages from a background task and process them on the app thread. Study `app/src/ai/agent_sdk/mod.rs:344-452` and `app/src/ai/agent_tips.rs:527` as canonical examples of `ctx.spawn(future, callback)` in existing AI code.
- Find the actual `FinishedAction` variant definition and its fields to determine the concrete result type. Grep: `grep -rn "FinishedAction" app/src/ai/blocklist/ --include="*.rs"`
- Determine whether `register_pending_result` should be a new method on `BlocklistAIActionExecutor` or whether results are routed via the existing event subscription mechanism alone.

### 5.4.1 Tool Adapter Layer

The provider wire format uses `ToolCall { id: String, name: String, input: serde_json::Value }`. The executor uses `AIAgentAction` (a typed enum). A bidirectional adapter must be defined:

**`ToolCall` → `AIAgentAction`** (inbound, called before dispatch):
- Pattern-match on `name` string
- Parse `input: serde_json::Value` into the typed variant's fields
- Return `Err` for unknown tool names

**Result → `ContentBlock::ToolResult`** (outbound, called after dispatch):
- The actual result type is determined by the `FinishedAction` event's fields — find this via Phase 0 grep before writing the adapter.
- **`AIAgentActionResultType` does not exist** — this type name appears nowhere in the codebase. Do not reference it. Use the actual type found in `FinishedAction`.
- Map each result variant to `ToolResultContent::Text(formatted_string)` or `ToolResultContent::Blocks(...)` for image-bearing results.

**V1 supported tools** (must be enumerated before Phase 4; minimum set for dogfood):
- `RequestCommandOutput` (shell execution)
- `ReadFiles` (file reads)
- `AskUserQuestion` (user confirmation)
- `SearchCodebase`, `Grep`, `FileGlob` (search tools)
- `ApplyFileDiffs` (file writes — requires confirmation)

MCP tools (`CallMCPToolExecutor`) are wired to the existing executor. The adapter layer must reserve the tool-name namespace for MCP tools so V2 can route them without conflicts.

**`requires_confirmation` mapping** (maintained here, alongside the adapter):

| Tool name | Requires confirmation |
|---|---|
| `ApplyFileDiffs` | Yes |
| `RequestCommandOutput` | Yes (if not in autonomy level that permits shell execution) |
| `AskUserQuestion` | Yes (by definition) |
| `ReadFiles`, `SearchCodebase`, `Grep`, `FileGlob` | No |

### 5.5 Direct Agent Loop

**New types required (must be defined before Phase 1 code compiles):**

```rust
// Type alias for the channel used to emit agent events to the UI
pub type AgentEventSender = mpsc::Sender<AgentEvent>;

// Defined in crates/ai/src/agent_events/mod.rs (new)
pub enum AgentEvent {
    TextChunk(String),
    TokenUsage(TokenUsage),
    Error(String),
    Done,
}
```

Note: `FusedCancelSignal` in §12's `AgentTransport` trait is shorthand for `futures::future::Fuse<futures::channel::oneshot::Receiver<()>>`. Define a type alias in `direct_loop.rs`:
```rust
type FusedCancel = futures::future::Fuse<futures::channel::oneshot::Receiver<()>>;
```

**Cancellation:** The existing cancellation signal is `futures::channel::oneshot::Receiver<()>` (not `tokio::sync::oneshot`). A `futures::channel::oneshot::Receiver` implements `Future` and can be fused for multi-poll use. The direct loop converts it once to a fused future at entry and reuses it across iterations.

**Stream selection:** `ChatStream` is a `Stream`, not a `Future`. You cannot `select!` directly on a stream object. Use `stream.next().fuse()` inside each iteration.

**Tool call parallelism:** Tool calls without confirmation requirements must be dispatched concurrently. Use `FuturesUnordered`. Use `.into_iter()` on an owned `Vec<ToolCall>` — do NOT use `.iter()` (borrowed iterator references cannot be captured in `async move` blocks for independently polled futures).

```rust
// app/src/ai/agent/direct_loop.rs

use futures::{FutureExt, StreamExt, stream::FuturesUnordered};

/// The tool dispatch channel crosses the background/main-thread boundary.
/// Background future sends requests; DirectLoopModel processes them on the app thread.
pub struct ToolDispatchRequest {
    pub action: AIAgentAction,
    pub conversation_id: AIConversationId,
    pub result_tx: futures::channel::oneshot::Sender<Result<ToolResult, DispatchError>>,
}

pub async fn run(
    provider: SharedProvider,
    initial_messages: Vec<ChatMessage>,
    tools: Vec<Tool>,
    conversation_id: AIConversationId,        // Required — not in prior plan versions
    tx: AgentEventSender,
    tool_req_tx: mpsc::Sender<ToolDispatchRequest>,  // Channel to main-thread executor
    cancellation_rx: futures::channel::oneshot::Receiver<()>,
) -> Result<(), ProviderError> {
    let mut history = initial_messages;
    // Fuse the receiver so it can be polled multiple times without being consumed
    let mut cancel: FusedCancel = cancellation_rx.fuse();

    loop {
        let request = ChatRequest {
            messages: trim_to_context_window(&history, &provider.capabilities()),
            tools: tools.clone(),
            options: ChatOptions::default(),
        };

        // Select between cancellation and stream acquisition
        let stream = futures::select! {
            _ = cancel => return Ok(()),
            result = provider.chat_stream(request).fuse() => result?,
        };

        let (text, tool_calls) = collect_and_emit_stream(stream, &tx, &mut cancel).await?;

        history.push(ChatMessage::Assistant {
            text: text.clone(),
            tool_calls: tool_calls.clone(),
        });

        if tool_calls.is_empty() {
            break;
        }

        // Enforce per-session safety cap before dispatching tools
        if history.len() > MAX_DIRECT_LOOP_TURNS {
            let _ = tx.try_send(AgentEvent::Error(
                "Direct-mode agent reached the maximum turn limit.".into()
            ));
            break;
        }

        // Pre-classify tool calls: if any require confirmation, serialize all
        let (confirm_calls, parallel_calls): (Vec<_>, Vec<_>) = tool_calls
            .into_iter()
            .partition(|tc| adapter::requires_confirmation(&tc.name));

        let mut results: Vec<(usize, ContentBlock)> = Vec::new();

        if !confirm_calls.is_empty() {
            // Serialize all calls (confirmation batch must be sequential)
            for (i, tc) in confirm_calls.into_iter().chain(parallel_calls).enumerate() {
                let block = dispatch_one(tc, i, conversation_id, &tool_req_tx, &mut cancel).await?;
                results.push(block);
            }
        } else {
            // Safe to dispatch concurrently with FuturesUnordered
            let mut pending: FuturesUnordered<_> = parallel_calls
                .into_iter()   // owned iteration — avoids lifetime issues with async move
                .enumerate()
                .map(|(i, tc)| {
                    let tool_req_tx = tool_req_tx.clone();
                    async move {
                        dispatch_one(tc, i, conversation_id, &tool_req_tx, &mut /* cancel */ ...).await
                    }
                })
                .collect();
            // Note: cancel signal cannot be passed into concurrent futures simultaneously.
            // Use a separate broadcast/watch channel for cancellation in the concurrent path,
            // or poll cancel in the outer collection loop (see below).

            loop {
                futures::select! {
                    _ = cancel => return Ok(()),
                    item = pending.next().fuse() => match item {
                        Some(Ok(pair)) => results.push(pair),
                        Some(Err(e)) => return Err(e),
                        None => break,
                    }
                }
            }
        }

        // Sort back to original call order
        results.sort_by_key(|(i, _)| *i);
        let result_blocks: Vec<ContentBlock> = results.into_iter().map(|(_, b)| b).collect();

        // Single User message containing all tool results
        history.push(ChatMessage::User(result_blocks));
    }

    Ok(())
}

/// Safety cap: prevent runaway agent loops from exhausting provider quota.
const MAX_DIRECT_LOOP_TURNS: usize = 50;
```

**Cancellation in concurrent dispatch (open design question):** The `cancel` signal is a `&mut FusedCancel` — it cannot be shared across `FuturesUnordered` futures simultaneously. Options for the concurrent path:
- Use a `tokio::sync::watch::Receiver<bool>` as a broadcast cancellation channel that can be cloned into each future (requires `tokio` dep, already present)
- Move the cancel check to the outer `pending.next()` collect loop only (simpler; slightly delayed cancellation response)

The plan recommends Option 2 (outer-loop cancel check only) for V1 simplicity. Add a Phase 0 task to confirm this matches WarpUI's cancellation expectations.

`collect_and_emit_stream` — correct stream/cancel select pattern:

```rust
async fn collect_and_emit_stream(
    mut stream: ChatStream,
    tx: &AgentEventSender,
    cancel: &mut FusedCancel,
) -> Result<(Option<String>, Vec<ToolCall>), ProviderError> {
    let mut text = String::new();
    let mut tool_calls = Vec::new();
    loop {
        futures::select! {
            _ = cancel => return Err(ProviderError::Cancelled),
            item = stream.next().fuse() => match item {
                Some(Ok(StreamEvent::TextChunk(chunk))) => {
                    text.push_str(&chunk);
                    let _ = tx.try_send(AgentEvent::TextChunk(chunk));
                }
                Some(Ok(StreamEvent::ToolCallReady(tc))) => tool_calls.push(tc),
                Some(Ok(StreamEvent::End { usage, .. })) => {
                    if let Some(u) = usage {
                        let _ = tx.try_send(AgentEvent::TokenUsage(u));
                    }
                    break;
                }
                Some(Ok(_)) => {} // Start, ToolCallChunk — handled internally
                Some(Err(e)) => return Err(e),
                None => return Err(ProviderError::StreamParse(
                    "stream ended without End event".into()
                )),
            }
        }
    }
    Ok((if text.is_empty() { None } else { Some(text) }, tool_calls))
}
```

**Context window trimming** — trim by atomic units, accounting for batched results:

```rust
fn trim_to_context_window(
    history: &[ChatMessage],
    capabilities: &ModelCapabilities,
) -> Vec<ChatMessage> {
    // Always preserve index 0 (System prompt).
    // Group remaining messages into atomic "turn" units:
    //   - Standalone User or Assistant(text-only): unit of 1
    //   - Assistant(tool_calls) + following User(tool_results): unit of 2, INDIVISIBLE
    //     NOTE: A single User(tool_results) message may contain MANY ContentBlock::ToolResult
    //     entries (one per batched tool call). Do NOT split individual results out of this message.
    //     If the batch itself is too large, summarize or truncate individual results before
    //     history insertion — do not make the User message itself partially trimmable.
    // Remove oldest complete units until estimated token count is within budget.
}
```

**Token estimation:**
- `tiktoken-rs` (cl100k_base encoding) as the **cross-provider estimator** for trim decisions — use for OpenAI, Anthropic, and Google. Must be gated behind `#[cfg(not(target_arch = "wasm32"))]` in `crates/ai`. Verify binary size delta and wasm-compilation impact before landing (add as a Phase 0 sub-task).
- Pull `input_tokens` + `output_tokens` from `StreamEvent::End { usage }` for accurate **post-hoc cost accounting**. Do NOT call Anthropic's `/v1/messages/count_tokens` on the hot path — it adds a network RTT before every request and fails when the endpoint is unavailable.
- `char / 3` as a conservative Ollama fallback only.

**Context-length error recovery**: On `ProviderError::ContextLengthExceeded`, retry once with system prompt + last 4 messages only. Surface a user-visible warning via `tx`.

### 5.6 Model Registry

**Fetch on first AI request, not on launch** — eliminates unauthorized launch-time egress.

**Diesel connection pool correction:** `crates/persistence` has **no connection pool** — that crate exports only schema types, model types, and embedded migrations. It contains no `r2d2::Pool`, no `SqlitePool`, and no connection management. Synchronous Diesel operations called from async Tokio tasks must go inside `tokio::task::spawn_blocking`. The actual database connection management in the app is located in `app/src/` — find it via `grep -r "SqliteConnection\|r2d2\|diesel::Connection" app/src/ --include="*.rs" -l` before Phase 2 begins and use the same pattern.

```rust
// crates/ai/src/model_registry/mod.rs

pub struct ModelRegistry {
    // Diesel operations wrapped in spawn_blocking — no SqlitePool assumed.
    // Pattern: tokio::task::spawn_blocking(move || { conn.execute(...) }).await?
    db_path: PathBuf,
    cache: Arc<Mutex<HashMap<ProviderKind, (Vec<ModelRecord>, Instant)>>>,
    ttl: Duration, // 24h
}
```

Diesel migration: new timestamped folder in `crates/persistence/migrations/` (currently 134 migrations, verified), new table added to `schema.rs` (regenerated via `diesel migration run`), new types in `crates/persistence/src/model.rs`. Use `Timestamp` not `DateTime<Utc>` — matches existing schema column type.

**Lifecycle tracking (V1):**
- Present in latest fetch → `last_seen = now`, `removed = false`
- Absent from latest fetch → `removed = true`, surface warning if it was user's selected model
- No `deprecated_at`, `first_seen` tracking in V1

**Model filtering:** Use the **static capability registry** (`crates/ai/src/model_registry/known_capabilities.rs`) as the source of truth for what appears in the chat model picker. Do NOT ship a denylist (`whisper-*`, `dall-e-*`, etc.) — OpenAI model naming is unstable and prefix heuristics will misclassify future models. Unknown model IDs from `/v1/models` are hidden by default with a "show unverified models" toggle as an escape hatch. Manual model ID entry is always available.

**Capability population:** Most `/v1/models` responses don't return capability flags. Overlay static registry with any capabilities returned by the API; allow user override. OpenRouter's `/api/v1/models` is the richest source — use it as the primary capability reference.

### 5.7 Settings UI — Correct Navigation Path

The settings navigation path "Settings → Agents → Warp Agent → Providers" does not exist. The correct location:

- Add `AgentProviders` variant to `SettingsSection` enum (`app/src/settings_view/mod.rs:188`)
- Add `AgentProviders` variant to `AISubpage` enum (`app/src/settings_view/ai_page.rs:94`)

**Verified `AISubpage` variants** (actual, as of current codebase): `WarpAgent`, `Profiles`, `Knowledge`, `ThirdPartyCLIAgents`. There is no existing `ApiKeys` or `BYO` variant. The new `AgentProviders` variant is an addition, not an extension of an existing one.

- Wire navigation in `mod.rs:1218+`
- Extend `render_api_keys_section` (`ai_page.rs:6427`) or add a sibling `render_providers_section`

**Model picker integration — corrected chain:**

`LLMPreferences` (`app/src/ai/llms.rs:547`) is a singleton entity holding the catalog of available LLMs. It fires `UpdatedAvailableLLMs` events when the catalog changes. `AIExecutionProfilesModel` (at `app/src/ai/execution_profiles/profiles.rs`) reads from this catalog for per-profile model selection.

**`LLMPreferences::available_llms` does not exist** — this method/field name has zero occurrences in the codebase. The only related hit is `should_refresh_available_llms_on_stream_finish: bool`, a boolean flag. The actual API for registering new models with `LLMPreferences` must be determined in Phase 0 by reading `llms.rs` in full. Do not assume this method exists.

The correct integration chain once the API is identified:
```text
LLMPreferences (catalog — must add new provider models via the actual API)
    → fires UpdatedAvailableLLMs
    → AIExecutionProfilesModel (per-profile model selection — RequestParams.model: LLMId)
    → RequestParams.model: LLMId (set at api.rs:314)
```

Direct-provider models must be registered into **both** `LLMPreferences` (so the picker shows them) AND be consumable by profiles via `AIExecutionProfilesModel`. This is two integration points, not one. Budget +1 week for Phase 3 vs. the original estimate.

The recent commit `9c1df06` ("Support third party harness model selection") added per-harness model paths to `app/src/ai/agent_sdk/mod.rs` — direct-provider model injection must not conflict with this. Co-ordinate with the author before Phase 3.

**Profile granularity for provider toggle:**
- Profile = transport mode (server / direct) + provider + model + system prompt
- Per-profile direct/server toggle; the default profile remains server-routed
- The UI must show a confirmation when converting the default profile to direct mode (blast radius: all terminals using that profile switch simultaneously)
- No per-session override mechanism exists in V1 — the toggle is global per-profile. Add a Phase 0 task to determine if per-session override is needed before Phase 3.
- Fallback rule: if the active session type rejects direct dispatch (e.g., WASM, certain SSH contexts), silently fall back to server with a single non-modal toast notification
- Telemetry: record `profile_changed_at` timestamp when profile toggles to enable attribution of downstream errors

**Phase 3/4 interlock:** The `DirectApiCalls` flag must NOT be flipped on while Phase 4 dispatch is incomplete. Use a separate `DirectApiCallsUI` sub-flag for the settings UI during Weeks 5–7, keeping `DirectApiCalls` off until Phase 4c (dispatch fully wired, retry/circuit-breaker in place). Clarify this split in the flag rollout (§6).

**"Settings → Privacy → API Key Storage"** section does not exist. V1 does not add a storage tier UI — defer to V2.

---

## 6. Feature Flag Rollout

```bash
FeatureFlag::DirectApiCallsUI   → DOGFOOD_FLAGS in Phase 3 (settings UI visible, dispatch not yet wired)
FeatureFlag::DirectApiCalls     → DOGFOOD_FLAGS in Phase 4c (full dispatch + retry + circuit-breaker ready)
                                → PREVIEW_FLAGS after dogfood validation
                                → RELEASE_FLAGS after preview stabilisation

FeatureFlag::SoloUserByok       → Activated via Cargo feature `solo_user_byok`
                                  (app/Cargo.toml:925, app/src/lib.rs:2882)
                                → The runtime flag's enum VARIANT only exists when the
                                  Cargo feature is compiled in. Build without the feature
                                  = the flag cannot be checked at runtime.
```

**`SoloUserByok` × `DirectApiCalls` decision matrix:**

| `SoloUserByok` (runtime) | `DirectApiCalls` (runtime) | Behavior |
|---|---|---|
| Off | Off | All requests via Warp server, no BYOK |
| On | Off | Server-side BYOK (today's behavior — keys forwarded to Warp server) |
| Off | On | Direct mode unreachable — no key entry UI exposed |
| On | On | Direct mode active for solo users with keys — keys NOT forwarded to Warp server |

**Build-time guard required:** Add a `compile_error!` or Cargo feature dependency in `app/Cargo.toml` to ensure `direct_api_calls` Cargo feature (if created) requires `solo_user_byok`. Alternatively, document explicitly that `FeatureFlag::DirectApiCalls` is runtime-only and can only be meaningfully enabled in builds that also compile `solo_user_byok`. Add a runtime assertion:

```rust
if FeatureFlag::DirectApiCalls.is_enabled() {
    debug_assert!(
        cfg!(feature = "solo_user_byok"),
        "DirectApiCalls enabled in a build without solo_user_byok Cargo feature"
    );
}
```

---

## 7. Testing Strategy (TDD — Correctly Ordered)

**Revised TDD ordering for streaming protocol work:**

1. **Spike** (Week 1, before Phase 1): Send one real message to OpenAI and Anthropic. Capture raw SSE byte sequences to fixture files (with keys stripped). Capture multi-block parallel tool call responses specifically. This produces evidence for test writing.
2. **Write parser tests against fixtures** (Phase 1): Test SSE parsers against captured real responses, not synthetic byte sequences.
3. **Extract trait and mock** after parser tests pass.
4. **Write agent loop tests** against `MockLlmProvider`.

### 7.1 Test Layers

```bash
┌─────────────────────────────────────────────────┐
│ Unit tests (no network)                          │
│ • ProviderError variants + user message mapping  │
│ • ContentBlock serialization (OpenAI ↔ Anthropic)│
│ • In-stream error decoding (200 OK + error event)│
│ • Parallel tool call concurrent dispatch         │
│ • NeedsConfirmation serialization (batch rule)   │
│ • Tool-batch-preserving context trim             │
│ • ToolCall → AIAgentAction adapter mapping       │
│ • FinishedAction result → ContentBlock mapping   │
│ • Retry backoff calculation + Retry-After parsing│
│ • ApiKeys serialization round-trip               │
│ • ModelRecord lifecycle state transitions        │
│ • Capability flag static registry lookups        │
└─────────────────────────────────────────────────┘
         ↓
┌─────────────────────────────────────────────────┐
│ Fixture-based parser tests                       │
│ • OpenAI SSE parser (real captured responses)    │
│ • Anthropic SSE parser (real captured responses) │
│ • Anthropic parallel tool call multi-block       │
│ • Ollama NDJSON parser                           │
│ • Tool call fragment accumulation (real deltas)  │
└─────────────────────────────────────────────────┘
         ↓
┌─────────────────────────────────────────────────┐
│ Integration tests (wiremock-rs HTTP mock)        │
│ NOTE: Tests use plain HTTP mock servers.         │
│ The LlmProvider trait's with_base_url() method   │
│ injects the mock URL. No TLS configuration is    │
│ needed — production uses HTTPS base URLs,        │
│ tests use http://localhost:<port>.               │
│ Add wiremock as dev-dependency; no TLS conflict. │
│ • Chat round-trip per provider format            │
│ • Streaming round-trip                           │
│ • Multi-turn with tool calls                     │
│ • 429 retry + Retry-After header                 │
│ • ContextLengthExceeded recovery                 │
│ • In-stream error (200 OK + error event)         │
│ • Model list fetch + lifecycle update            │
│ • NeedsConfirmation pause/resume                 │
│ • Concurrent tool dispatch + result ordering     │
└─────────────────────────────────────────────────┘
         ↓
┌─────────────────────────────────────────────────┐
│ Smoke tests (real API, env-var gated)            │
│ • One chat round-trip per provider               │
│ • Skipped in CI without $PROVIDER_API_KEY        │
└─────────────────────────────────────────────────┘
```

### 7.2 MockLlmProvider

```rust
pub struct MockLlmProvider {
    // Queue of responses — consumed in order, errors included
    responses: VecDeque<Result<ChatResponse, ProviderError>>,
    stream_sequences: VecDeque<Vec<StreamEvent>>,
    // Inspector for verifying what was sent
    requests_received: Arc<Mutex<Vec<ChatRequest>>>,
    capabilities: ModelCapabilities,
    base_url: Option<String>,
}
```

---

## 8. Observability & Redaction

**Secret scrubbing must be in Phase 1, not Phase 5.** Logging unredacted API keys during dogfood is a security incident.

**Use `log::` macros only** — CLAUDE.md §5 mandates this. Do NOT use `tracing::info_span!` or any `tracing::` macro in AI module code. The `tracing` crate is technically in the workspace but the project coding rules prohibit it in new code. Use structured `log::info!` with key=value fields:

```rust
log::info!("direct_api: request start provider={provider} model={model}");
log::info!("direct_api: request end input_tokens={input} output_tokens={output} duration_ms={ms}");
log::warn!("direct_api: rate limited provider={provider} retry_after_secs={secs:?}");
log::error!("direct_api: auth failed provider={provider}");  // never include key value
```

Add a log filter that strips any field matching `*api_key*`, `*Authorization*`, `*token*` before emission. Truncate prompt content in log fields to 1,000 chars, completions to 2,000 chars. Do not log terminal command output or file contents routed through tool calls. `ProviderError::Auth` variants must not include the key value, only provider name.

**What to collect on direct path:**
- Provider kind and model ID (for product analytics)
- Token usage counts from `StreamEvent::End { usage }` — displayed as "~N tokens this session"
- Error categories without content (for reliability monitoring)
- Do NOT collect: prompt content, tool outputs, conversation content

**Token cost display:** "~N tokens" without a pricing table informs but cannot translate to dollars. V1 ships `TokenUsage` display with a static pricing table `HashMap<(ProviderKind, ModelId), Pricing { input_usd_per_mtok, output_usd_per_mtok }>` — this is 1 day of work and prevents a foreseeable angry-user incident. Include Anthropic prompt cache tier pricing (`cache_read_usd_per_mtok`).

---

## 9. Implementation Phases

### Phase 0 — Pre-flight (Week 1)
- [ ] Privacy/legal review initiated (gate on sign-off before Phase 1 code)
- [ ] `genai` evaluation memo with pass/fail criteria from §4.2 — decision by end of week
- [ ] Feature compatibility matrix approved (§4.3)
- [ ] Threat model document drafted (§4.4)
- [ ] Conversation persistence decision signed off (§4.5 — Path A: in-memory V1)
- [ ] 1-week spike: capture real SSE fixture files from OpenAI and Anthropic, including parallel tool call responses
- [ ] Secret scrubbing log filter implemented (blocks Phase 1 dogfood)
- [ ] `tiktoken-rs` binary size delta measured; wasm compilation impact verified
- [ ] Identify actual database connection pattern in `app/src/` for Phase 2 ModelRegistry design
- [ ] Find actual `FinishedAction` event variant definition and its result type fields — determines §5.4.1 adapter output type
- [ ] Find WarpUI mechanism for background mpsc receiver → main-thread callback (study `agent_sdk/mod.rs`, `agent_tips.rs:527` as canonical `ctx.spawn` examples)
- [ ] Read `app/src/ai/llms.rs` in full to find the actual API for registering new models with `LLMPreferences` (not `available_llms` — that doesn't exist)
- [ ] Determine if per-session profile override is needed before Phase 3 (§5.7 profile blast-radius)
- [ ] Confirm cancellation approach for concurrent FuturesUnordered dispatch (§5.5 open question)
- [ ] Confirm `SoloUserByok` × `DirectApiCalls` decision matrix and add build-time guard (§6)

### Phase 1 — Foundation (Week 2–5)
- [ ] `AgentEvent` enum + `AgentEventSender` type alias defined
- [ ] `ProviderError` defined with all variants including `Remote` (in-stream errors after 200 OK)
- [ ] `ContentBlock`, `ChatMessage`, `ChatRequest`, `StreamEvent`, `ChatStream` types defined
- [ ] `LlmProvider` trait with `#[async_trait]` + `with_base_url()` + `MockLlmProvider` implemented
- [ ] Fixture-based parser tests written (OpenAI SSE, Anthropic SSE, Anthropic parallel tool calls, Ollama NDJSON)
- [ ] Tool call fragment accumulation tests (real deltas from fixture)
- [ ] Tool batch context trim tests (batch is indivisible unit)
- [ ] `tiktoken-rs` (cl100k_base) integrated behind `#[cfg(not(target_arch = "wasm32"))]`
- [ ] `ApiKeys` extended with new provider fields (via `ApiKeyManager`, not new struct)
- [ ] `genai` OR hand-rolled adapters wired behind `LlmProvider` trait (per evaluation memo decision)

### Phase 2 — Model Registry (Week 5–7)
- [ ] Diesel migration (in `crates/persistence/migrations/`) + `schema.rs` regeneration + model types
- [ ] Per-provider model fetch using `spawn_blocking` wrapping synchronous Diesel operations
- [ ] Static capability registry for known models (`known_capabilities.rs`)
- [ ] Background refresh on first AI request (not launch)
- [ ] Lifecycle tracking (present/absent in V1; warn on removed selected model)
- [ ] Model picker: static registry as source of truth; unknown models hidden by default
- [ ] Tests: fetch, lifecycle transitions, stale cache fallback, unknown-model hiding

### Phase 3 — Settings UI (Week 7–9)
- [ ] `FeatureFlag::DirectApiCallsUI` added to `DOGFOOD_FLAGS` (UI visible, dispatch not yet wired)
- [ ] `AgentProviders` added to `SettingsSection` + `AISubpage` enums (after correct variant names confirmed)
- [ ] Provider configuration UI (provider selector, model picker, key input, test-connection button)
- [ ] Model picker: new provider models registered in `LLMPreferences` AND `AIExecutionProfilesModel` (via API identified in Phase 0)
- [ ] Profile granularity: per-profile direct/server toggle with confirmation dialog for default profile
- [ ] Removed model indicator in model picker
- [ ] In-memory-only banner ("conversations not saved in direct mode V1")
- [ ] Prompt caching cost warning for Anthropic direct mode (until prompt caching ships in Phase 4)
- [ ] Static pricing table for token cost display

### Phase 4 — Direct Dispatch Path (Week 9–15)
- [ ] **Phase 4a (Weeks 9–12):** `DirectLoopModel` implemented with mpsc channel bridge
- [ ] `BlocklistAIActionExecutor` extended with result-routing mechanism (per Phase 0 design)
- [ ] Tool adapter layer: `ToolCall → AIAgentAction` + `FinishedAction result → ContentBlock` (full V1 tool set)
- [ ] `requires_confirmation` static mapping implemented
- [ ] `NeedsConfirmation` pre-classification and serialization flow
- [ ] `direct_loop::run` implemented with `FuturesUnordered` concurrent tool dispatch (parallel calls only)
- [ ] `collect_and_emit_stream` with `stream.next().fuse()` + fused cancel signal
- [ ] Cancellation: fused `futures::channel::oneshot::Receiver`, in-flight tool cancellation propagated
- [ ] `conversation_id: AIConversationId` in `run()` signature
- [ ] All agent loop tests passing against `MockLlmProvider`
- [ ] **Phase 4b (Weeks 12–13):** Retry + exponential backoff with jitter + `Retry-After` header respect
- [ ] Circuit breaker per endpoint
- [ ] `MAX_DIRECT_LOOP_TURNS` cap (50) with user-visible error
- [ ] `ContextLengthExceeded` recovery (retry with trimmed history)
- [ ] Prompt caching for Anthropic adapter (`cache_control: { type: "ephemeral" }` on system blocks)
- [ ] **Phase 4c (Week 13):** `FeatureFlag::DirectApiCalls` added to `DOGFOOD_FLAGS`
- [ ] Branch in `api/impl.rs` (before protobuf construction) selecting backend vs direct path
- [ ] wiremock integration tests (plain HTTP mock via `with_base_url()`, no TLS configuration)
- [ ] Explicit UI messaging for unsupported features in direct mode (§4.3 matrix)
- [ ] MCP interface boundary stub committed for V2 (reserve tool-name namespace, document call path)

### Phase 5 — Hardening (Week 15–17)
- [ ] Token usage + cost display in UI ("~N tokens / ~$X this session")
- [ ] Real-API smoke tests (env-var gated)
- [ ] `SoloUserByok` enabled in `DOGFOOD_FLAGS` (only safe once `DirectApiCalls` also active, per §6 matrix)
- [ ] Profile-toggle blast-radius confirmation UX hardening
- [ ] `SoloUserByok` × `DirectApiCalls` runtime assertion added

### Phase 6 — Rollout (Week 17–21)
- [ ] `DirectApiCalls` → `PREVIEW_FLAGS` after dogfood validation
- [ ] `DirectApiCalls` → `RELEASE_FLAGS` after preview stabilisation
- [ ] Remove `DirectApiCalls` + `DirectApiCallsUI` flags, clean dead branches

---

## 10. Key Risks

| Risk | Mitigation |
|---|---|
| `genai` beta/abandoned | Pass/fail criteria in §4.2; hand-roll 2 providers as default fallback |
| `AsyncAppContext` redesign more complex than estimated | Phase 0 research + canonical examples from `agent_sdk/mod.rs`; early prototype in Phase 4a |
| `FinishedAction` result routing more complex than a HashMap | Phase 0 task to read execute.rs and event dispatch flow before Phase 4a |
| `LLMPreferences` model registration API unknown | Phase 0 task to read `llms.rs` fully before Phase 3 UI work |
| `NeedsConfirmation` partial batch breaks Anthropic transcript | Pre-classification + serialization rule in §5.4; unit tested |
| `BlocklistAIActionExecutor` bridge more complex than estimated | Scoped to `DirectLoopModel` channel pattern in §5.4; early prototype in Phase 4a |
| `LLMPreferences` + `AIExecutionProfilesModel` two-point integration | Both integration points explicitly named in §5.7; +1 wk Phase 3 budget |
| Privacy/legal blocks Phase 1 | Phase 0 gate; plan does not proceed without sign-off |
| tiktoken-rs wasm compat or binary size | Phase 0 measurement gate before adoption |
| No `SqlitePool` in `crates/persistence` | Phase 0 sub-task: identify real DB connection pattern; use `spawn_blocking` |
| In-stream errors not caught (200 OK + error SSE) | `ProviderError::Remote` variant + fixture-based parser tests |
| `SoloUserByok` (Cargo feature) ≠ `DirectApiCalls` (runtime) | Decision matrix in §6; build-time assertion added |
| `SoloUserByok` + dogfood = keys sent to Warp server | Enable only after `DirectApiCalls` is also active and tested (Phase 4c) |
| Model registry fetch = launch-time egress | Deferred to first AI request only |
| Commit `9c1df06` (harness model selection) conflicts | Co-ordinate with author before Phase 3 |
| Parallel tool calls break Anthropic transcript | `FuturesUnordered` dispatch + original-order sort + single User message; unit tested |
| Concurrent cancellation not propagated to parallel futures | Phase 0 decision: outer-loop cancel only (V1); watch channel in V2 |
| Prompt caching omission → 5-10× Anthropic bill | Prompt caching in Phase 4b; cost warning in Phase 3 UI until it ships |
| User account quota exhausted by runaway loop | `MAX_DIRECT_LOOP_TURNS = 50` cap in loop; circuit breaker in Phase 4b |
| `tracing::` used in log calls | All §8 observability uses `log::` macros per CLAUDE.md §5 |
| Two agent loops → permanent feature parity debt | See §12 convergence roadmap |

---

## 11. Open Questions

1. **`genai` or hand-rolled?** Decision by end of Phase 0 via evaluation memo criteria.
2. **Which tools ship in direct-mode V1?** §5.4.1 minimum set; must be locked before Phase 4a.
3. **Conversation history persistence:** Resolved — Path A (in-memory V1), Path B (SQLite V2). See §4.5.
4. **Provider toggle granularity:** Resolved — per-profile toggle with default-profile confirmation. See §5.7.
5. **MCP servers in direct mode?** V2. Interface boundary stub committed in Phase 4c so V2 can add without rework.
6. **Token cost visibility?** V1 ships token count + static pricing table. See §8.
7. **`secrecy` crate (zeroize-on-drop)?** Not currently in `Cargo.lock`. If required by threat model (§4.4), evaluate and add explicitly — do not assume it exists.
8. **WarpUI mpsc-to-main-thread mechanism?** Phase 0 research required. See §5.4.
9. **`LLMPreferences` model registration API?** Phase 0 research required. See §5.7.
10. **Per-session profile override?** Phase 0 decision required. See §5.7.
11. **Concurrent FuturesUnordered cancellation approach?** V1 uses outer-loop cancel check; V2 may use watch channel. See §5.5.

---

## 12. Two-Loop Convergence Roadmap

The plan acknowledges two agent loops will coexist: the server-routed path (30+ active features, protobuf transport) and the direct path (V1 subset, HTTP/SSE transport). Without an explicit convergence plan, the direct path is a permanent second-class citizen whose feature gap grows with every new server-path feature.

**Declared position for V1:** Direct mode is intentionally second-class for the feature set in §4.3. The UI must communicate this explicitly. Users who need MCP, orchestration, Drive, or memory must use the server-routed path.

**Convergence target (V3, ~month 9–12):** Introduce a `Transport` trait that abstracts both paths:

```rust
// Requires #[async_trait] — workspace already declares async-trait = "0.1.89"
#[async_trait]
pub trait AgentTransport: Send + Sync + 'static {
    async fn send_turn(
        &self,
        input: &[AIAgentInput],
        history: &[ChatMessage],
        tools: &[Tool],
        tx: &AgentEventSender,
        cancel: &mut futures::future::Fuse<futures::channel::oneshot::Receiver<()>>,
    ) -> Result<TurnResult, TransportError>;
}
```

Note: `TurnResult` and `TransportError` must be defined as new types in `crates/ai` — they do not currently exist. Both `ServerTransport` and `DirectTransport` implement this trait. The agent loop (`impl.rs`) becomes transport-agnostic. Features added to the agent loop (autonomy, orchestration, MCP) apply to both transports automatically. This is the V3 milestone.

**Convergence enforcement (V2 requirement):**
- Add `.github/DIRECT_PATH_PENDING.md` — a tracked list of server-path features not yet ported to the direct path.
- Each PR touching `app/src/ai/server_loop/` or equivalent server-path code must include one of: (a) a matching direct-path port, or (b) a new entry in `DIRECT_PATH_PENDING.md` with a milestone label.
- A CI check (`script/check_direct_path_pending.sh`) fails if a file in the server-path feature set is modified without a corresponding `DIRECT_PATH_PENDING.md` update.
- Assign an owner for the V3 milestone before V2 ships.

**Tracking:** Each new server-path feature shipped in V2 must include a "direct-path compatibility: [N/A | V2-planned | V3-planned]" annotation in its PR description. This prevents silent drift accumulation.
