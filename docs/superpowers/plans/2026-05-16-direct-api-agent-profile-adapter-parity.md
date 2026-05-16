# Direct API Agent Profile Adapter Parity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the Agent Profile `DirectApi` routing option a complete alternative to the existing Warp Provider LLM route for local `warp-oss` Agent Mode, while preserving the legacy route unchanged.

**Architecture:** Keep `app/src/ai/agent/api/impl.rs::generate_multi_agent_output` as the branch point. Introduce a local Direct API run engine that normalizes provider requests, provider stream events, tool calls, tool results, usage, cancellation, and errors into the existing `warp_multi_agent_api::ResponseEvent` contract. The Warp Provider branch remains the source of truth for legacy server-backed behavior; Direct API becomes an explicit per-profile local backend with its own capability matrix and local-first guardrails.

**Tech Stack:** Rust 2018, WarpUI entity/model system, `warp_multi_agent_api`, `genai`, `tokio`, `futures`, `serde_json`, local `DirectAPISettings` in `~/.warp-oss/settings.toml`, existing Agent Profile model routing.

---

## Scope

This plan is for the follow-up parity work after `docs/superpowers/plans/2026-05-15-direct-api-profile-routing.md`.

It intentionally does not replace the existing Warp Provider path. It adds the missing Direct API backend behavior so selecting `Direct API` in an Agent Profile is not a partial or silent-success path.

## Source References

### Repo Sources

- `app/src/ai/agent/api/impl.rs:54-56` already branches to `generate_direct_api_output` when `params.model_routing.is_direct_api()`.
- `app/src/ai/agent/api/direct.rs:156-189` currently emits text chunks and tool-call messages, but drops tool-call chunks and ignores end metadata.
- `crates/ai/src/provider/genai_adapter.rs:351-403` currently maps genai stream events, drops `ToolCallChunk`, `ReasoningChunk`, and `ThoughtSignatureChunk`, and only emits captured tool calls on `End`.
- `app/src/ai/agent/api/direct_tools.rs:149-190` currently exposes only `ReadFiles`, `Grep`, and `RunShellCommand` to Direct API providers.
- `crates/settings/src/direct_api.rs:1-121` stores Direct API settings under `[agents.direct_api]` with `SyncToCloud::Never`.
- `app/src/lib.rs:1070-1072` and `app/src/lib.rs:1892-1894` currently force noop secure storage for `Channel::Oss`, which makes auth persistence unreliable.
- `app/src/ai/blocklist/task_status_sync_model.rs:33-46` is registered unconditionally and reports task status to Warp server APIs.

### External Provider Docs

- OpenAI streaming uses typed semantic events and has dedicated streaming tool-call events: https://developers.openai.com/api/docs/guides/streaming-responses
- OpenAI function calling is a multi-step application-managed loop: https://developers.openai.com/api/docs/guides/function-calling
- Anthropic Messages streaming uses named SSE event types and supports tool-use streaming: https://platform.claude.com/docs/en/build-with-claude/streaming
- Google Gemini function calling can continue a tool loop through SDK helpers; manual REST implementations must preserve conversation state and tool metadata: https://ai.google.dev/gemini-api/docs/function-calling
- OpenRouter Responses tool calling documents parallel tool calls, tool-call responses, conversation tool responses, streaming tool calls, validation, and best practices: https://openrouter.ai/docs/api/reference/responses/tool-calling
- Ollama REST streaming emits content, thinking, and tool-call fields and requires accumulating streamed fields for conversation history: https://docs.ollama.com/capabilities/streaming
- Ollama chat API accepts `messages`, optional `tools`, and streaming by default: https://docs.ollama.com/api/chat

## Industry Best-Practice Constraints

- Normalize provider differences at one adapter boundary. Do not let OpenAI, Anthropic, Gemini, OpenRouter, Ollama, and custom OpenAI-compatible event shapes leak into app UI, task storage, or action execution.
- Treat tool use as a loop, not a single event. The app sends tools, receives tool calls, executes approved calls, sends tool outputs back with matching IDs, and repeats until the provider returns final text or a terminal error.
- Preserve provider IDs, tool-call IDs, names, argument fragments, and result IDs in order. Tool-call streaming APIs commonly deliver partial arguments before a final assembled call.
- Accumulate streamed text, reasoning, thinking, and tool-call fragments before persistence decisions. Streaming chunks are not complete JSON documents.
- Do not report success for an empty provider stream unless the final reason explains a valid no-text outcome and the UI displays that state.
- Keep provider capability metadata explicit. Tool support, vision support, reasoning/thinking support, parallel tool support, max context, JSON mode, and web search vary by provider and by model.
- Keep user control around side-effecting tools. Provider tool calls are requests from the model; Warp must still enforce profile permissions, action confirmation, command risk handling, and cancellation.
- Use structured error categories. Auth failures, rate limits, network failures, model-not-found, unsupported tool mode, malformed tool JSON, provider parse errors, and cancellation should produce distinct local errors.
- Avoid secret leakage. API keys and bearer tokens must not appear in logs, telemetry, debug formatting, errors, request snapshots, or test output.

## Feature Parity Matrix

| Area | Warp Provider path | Direct API target | Notes |
|---|---|---|---|
| Profile selection | Existing Agent Profile model fields | `model_routing=DirectApi` plus `direct_api_model` | No silent fallback after explicit Direct API selection. |
| Basic chat | Server stream | Local provider stream | Must emit visible text or a user-facing no-output error. |
| Streaming text | Server `ResponseEvent` | Provider stream -> normalized `ResponseEvent` | Preserve order and cancellation generation. |
| Streaming tool calls | Server handles provider details | Direct adapter assembles fragments | `ToolCallChunk` cannot be dropped. |
| Tool execution | Existing action model | Existing action model | Direct API should reuse the same permission and confirmation machinery. |
| Tool result continuation | Server orchestrates | Local Direct API run engine resubmits results | Required for real agent workflows. |
| Parallel tools | Server supported | Capability-gated local support | If unsupported, serialize safely and disclose limits. |
| Read files | Existing tool | Existing local tool subset | Already mapped; needs loop continuation and result mapping. |
| Grep/search | Existing tool | Existing local tool subset | `SearchCodebase` can remain unsupported until mapped. |
| Shell commands | Existing risk/permission flow | Existing risk/permission flow | Preserve profile permissions and action confirmation. |
| Apply file diffs | Existing tool | Add only after local mapping and tests | Do not advertise until supported. |
| MCP tools | Existing tool | Capability-gated future work | Direct path must hide or explicitly error for unsupported MCP. |
| Web search | Server/backends | Provider-specific or unsupported | Disable or map per provider capability. |
| Reasoning/thinking | Server messages | Normalize provider reasoning events | At minimum avoid dropping all visible output when model emits reasoning first. |
| Images | Feature-flagged server path | Capability-gated future work | Do not pass images to providers that lack support. |
| Usage/cost | Server metadata | Provider usage where available | Show local token usage, not Warp credit cost. |
| Conversation restore | Server token metadata | Local conversation/run IDs | Avoid GraphQL metadata dependency for Direct API runs. |
| Task status sync | Server update | Disabled for local Direct API | Do not call Warp server APIs for local Direct API task status. |
| Auth | Warp login | No login required for local Direct API | OSS login persistence is a separate blocker to handle locally. |

## File Structure

- Modify `crates/ai/src/provider/types.rs`
  - Expand provider-neutral event and capability types.
  - Add explicit reasoning, tool argument delta, usage, finish reason, provider request ID, and provider error category fields.
- Modify `crates/ai/src/provider/genai_adapter.rs`
  - Convert every relevant `genai::chat::ChatStreamEvent` into provider-neutral events.
  - Preserve tool-call chunks, reasoning chunks, thought signatures where available, usage, and finish reasons.
- Create `crates/ai/src/provider/stream_accumulator.rs`
  - Assemble text, reasoning, and tool-call fragments by provider index/call ID.
  - Produce complete assistant messages and complete tool calls in provider order.
- Modify `app/src/ai/agent/api/direct_tools.rs`
  - Split request conversion, tool schema definitions, proto conversion, and tool result rendering into focused helpers.
  - Add capability-gated tool selection instead of always exposing the same fixed subset.
- Create `app/src/ai/agent/api/direct_run.rs`
  - Own the Direct API run loop.
  - Convert `RequestParams` to provider messages.
  - Stream provider output into `ResponseEvent`.
  - Pause on tool calls, let existing action machinery execute them, then resume the provider request with tool results.
- Modify `app/src/ai/agent/api/direct.rs`
  - Keep route validation and response-stream setup.
  - Delegate run execution to `direct_run`.
  - Emit usage, finish reason, and actionable local errors.
- Modify `app/src/ai/agent/api/impl.rs`
  - Keep Warp Provider behavior unchanged.
  - Ensure Direct API route uses local-only request state and does not compute or send server-only settings unnecessarily.
- Modify `app/src/ai/agent/conversation.rs`
  - Mark local Direct API conversations distinctly from server-backed conversations where server metadata or GraphQL refreshes are optional.
- Modify `app/src/ai/blocklist/task_status_sync_model.rs`
  - Skip task-status server updates for local Direct API conversations and local-only OSS runs.
- Modify `app/src/ai/blocklist/controller/response_stream.rs`
  - Treat Direct API local stream errors, empty final streams, cancellation, and tool waits as first-class states.
- Modify `app/src/ai/execution_profiles/direct_api_model_choices.rs`
  - Attach capability metadata to model choices where known.
  - Keep manual/stale selections selectable but visibly marked in the profile editor.
- Modify `app/src/settings_view/execution_profile_view.rs`
  - Show Direct API route status and unsupported-feature hints in the profile summary.
- Modify `app/src/ai/agent_sdk/driver.rs`
  - Respect selected profile routing for CLI-launched local agent runs, or reject Direct API profiles with a clear error until the CLI path is wired.
- Modify `crates/ai/src/logging/`
  - Add redacted Direct API run diagnostics for request shape, provider, model label policy, stream event counts, tool-call counts, and final state.
- Modify `app/src/lib.rs` and auth-adjacent OSS startup code only if implementation scope includes the startup-login blocker.
  - Use a local persistent auth state or no-login local mode instead of noop secure storage for OSS.

## Architectural Decisions

1. The branch point stays in `generate_multi_agent_output`.
   - Reason: all existing blocklist response stream callers already converge there.
   - Guardrail: the Warp Provider branch must not depend on Direct API structs beyond `RequestParams` fields already present.

2. Direct API does not call Warp server GraphQL endpoints for local run lifecycle.
   - Reason: `warp-oss` has no Warp server access.
   - Guardrail: local Direct API conversations need local IDs and local metadata.

3. The Direct API adapter normalizes to existing `warp_multi_agent_api::ResponseEvent` for UI compatibility.
   - Reason: the response stream controller and conversation model already consume that contract.
   - Guardrail: provider-neutral internal events must retain enough data to create valid response events.

4. Tool calls are resumed through the provider, not treated as final output.
   - Reason: provider docs define tool use as multi-step, and agents need final text after tool results.
   - Guardrail: every assistant tool-call message must be preserved in history before sending tool results.

5. Unsupported Direct API features are disabled or surfaced, not silently routed through Warp Provider.
   - Reason: explicit profile routing is a user choice.
   - Guardrail: Direct API route errors must be visible and actionable.

## Task 1: Add Provider Stream Contract Tests

**Files:**
- Modify `crates/ai/src/provider/types.rs`
- Create `crates/ai/src/provider/stream_accumulator.rs`
- Create `crates/ai/src/provider/stream_accumulator_tests.rs`
- Modify `crates/ai/src/provider/mod.rs`

- [ ] **Step 1: Add failing accumulator tests**

Create `crates/ai/src/provider/stream_accumulator_tests.rs`:

```rust
use super::stream_accumulator::{AccumulatedAssistantTurn, StreamAccumulator};
use super::{FinishReason, StreamEvent, ToolCall, TokenUsage};
use serde_json::json;

#[test]
fn accumulator_preserves_text_and_tool_call_order() {
    let mut acc = StreamAccumulator::default();

    acc.push(StreamEvent::TextChunk("Reading ".to_string())).unwrap();
    acc.push(StreamEvent::ToolCallChunk {
        index: 0,
        id: "call_a".to_string(),
        name: "ReadFiles".to_string(),
        args_fragment: r#"{"files":[{"#.to_string(),
    }).unwrap();
    acc.push(StreamEvent::ToolCallChunk {
        index: 0,
        id: "call_a".to_string(),
        name: "ReadFiles".to_string(),
        args_fragment: r#""name":"Cargo.toml"}]}"#.to_string(),
    }).unwrap();
    acc.push(StreamEvent::TextChunk("done.".to_string())).unwrap();
    acc.push(StreamEvent::End {
        finish_reason: FinishReason::ToolUse,
        usage: Some(TokenUsage {
            input_tokens: 10,
            output_tokens: 4,
            cache_read_tokens: None,
        }),
    }).unwrap();

    let turn = acc.finish().unwrap();
    assert_eq!(turn.text, "Reading done.");
    assert_eq!(turn.tool_calls, vec![ToolCall {
        id: "call_a".to_string(),
        name: "ReadFiles".to_string(),
        input: json!({"files": [{"name": "Cargo.toml"}]}),
    }]);
    assert_eq!(turn.finish_reason, FinishReason::ToolUse);
    assert_eq!(turn.usage.unwrap().total_tokens(), 14);
}

#[test]
fn accumulator_rejects_invalid_tool_json() {
    let mut acc = StreamAccumulator::default();
    acc.push(StreamEvent::ToolCallChunk {
        index: 0,
        id: "call_bad".to_string(),
        name: "ReadFiles".to_string(),
        args_fragment: "{not-json".to_string(),
    }).unwrap();
    acc.push(StreamEvent::End {
        finish_reason: FinishReason::ToolUse,
        usage: None,
    }).unwrap();

    let err = acc.finish().unwrap_err();
    assert!(err.to_string().contains("invalid streamed tool arguments"));
}

#[test]
fn accumulator_reports_empty_success_as_no_output() {
    let mut acc = StreamAccumulator::default();
    acc.push(StreamEvent::End {
        finish_reason: FinishReason::Stop,
        usage: None,
    }).unwrap();

    let turn = acc.finish().unwrap();
    assert_eq!(turn, AccumulatedAssistantTurn::empty_stop());
}
```

- [ ] **Step 2: Wire the test module**

In `crates/ai/src/provider/mod.rs`, add:

```rust
pub mod stream_accumulator;
```

In `crates/ai/src/provider/stream_accumulator.rs`, include:

```rust
#[cfg(test)]
#[path = "stream_accumulator_tests.rs"]
mod tests;
```

- [ ] **Step 3: Run failing tests**

Run:

```bash
cargo test -p ai stream_accumulator -- --nocapture
```

Expected: compile failure because `StreamAccumulator` does not exist.

- [ ] **Step 4: Implement the accumulator**

Create `crates/ai/src/provider/stream_accumulator.rs` with:

```rust
use std::collections::BTreeMap;

use anyhow::{anyhow, Context as _};
use serde_json::Value;

use super::{FinishReason, StreamEvent, TokenUsage, ToolCall};

#[derive(Debug, Clone, PartialEq)]
pub struct AccumulatedAssistantTurn {
    pub text: String,
    pub reasoning: Vec<String>,
    pub tool_calls: Vec<ToolCall>,
    pub finish_reason: FinishReason,
    pub usage: Option<TokenUsage>,
}

impl AccumulatedAssistantTurn {
    pub fn empty_stop() -> Self {
        Self {
            text: String::new(),
            reasoning: Vec::new(),
            tool_calls: Vec::new(),
            finish_reason: FinishReason::Stop,
            usage: None,
        }
    }
}

#[derive(Debug, Default)]
pub struct StreamAccumulator {
    text: String,
    reasoning: Vec<String>,
    tool_chunks: BTreeMap<usize, ToolCallBuilder>,
    ready_tool_calls: Vec<ToolCall>,
    finish_reason: Option<FinishReason>,
    usage: Option<TokenUsage>,
}

#[derive(Debug, Default)]
struct ToolCallBuilder {
    id: String,
    name: String,
    args: String,
}

impl StreamAccumulator {
    pub fn push(&mut self, event: StreamEvent) -> anyhow::Result<()> {
        match event {
            StreamEvent::Start => {}
            StreamEvent::TextChunk(chunk) => self.text.push_str(&chunk),
            StreamEvent::ReasoningChunk(chunk) => self.reasoning.push(chunk),
            StreamEvent::ToolCallChunk {
                index,
                id,
                name,
                args_fragment,
            } => {
                let builder = self.tool_chunks.entry(index).or_default();
                if !id.is_empty() {
                    builder.id = id;
                }
                if !name.is_empty() {
                    builder.name = name;
                }
                builder.args.push_str(&args_fragment);
            }
            StreamEvent::ToolCallReady(tool_call) => self.ready_tool_calls.push(tool_call),
            StreamEvent::End {
                finish_reason,
                usage,
            } => {
                self.finish_reason = Some(finish_reason);
                self.usage = usage;
            }
        }
        Ok(())
    }

    pub fn finish(self) -> anyhow::Result<AccumulatedAssistantTurn> {
        let mut tool_calls = self.ready_tool_calls;
        for (_index, builder) in self.tool_chunks {
            let input: Value = serde_json::from_str(&builder.args)
                .with_context(|| format!("invalid streamed tool arguments for {}", builder.id))?;
            if builder.id.trim().is_empty() {
                return Err(anyhow!("streamed tool call missing id"));
            }
            if builder.name.trim().is_empty() {
                return Err(anyhow!("streamed tool call missing name"));
            }
            tool_calls.push(ToolCall {
                id: builder.id,
                name: builder.name,
                input,
            });
        }

        Ok(AccumulatedAssistantTurn {
            text: self.text,
            reasoning: self.reasoning,
            tool_calls,
            finish_reason: self.finish_reason.unwrap_or(FinishReason::Other),
            usage: self.usage,
        })
    }
}
```

- [ ] **Step 5: Add `ReasoningChunk` to `StreamEvent`**

In `crates/ai/src/provider/types.rs`, extend `StreamEvent`:

```rust
    ReasoningChunk(String),
```

- [ ] **Step 6: Run tests**

Run:

```bash
cargo test -p ai stream_accumulator -- --nocapture
```

Expected: all `stream_accumulator` tests pass.

- [ ] **Step 7: Commit**

```bash
git add crates/ai/src/provider/mod.rs crates/ai/src/provider/types.rs crates/ai/src/provider/stream_accumulator.rs crates/ai/src/provider/stream_accumulator_tests.rs
git commit -m "Add Direct API stream accumulator"
```

## Task 2: Preserve Provider Streaming Events

**Files:**
- Modify `crates/ai/src/provider/genai_adapter.rs`
- Modify `crates/ai/src/provider/genai_adapter_tests.rs`

- [ ] **Step 1: Add failing genai adapter tests**

Add tests that verify these behaviors:

```rust
#[test]
fn converts_reasoning_chunks_to_provider_events() {
    let events = convert_genai_stream_event(fake_reasoning_chunk("thinking"));
    assert_eq!(events_to_debug_names(events), vec!["ReasoningChunk"]);
}

#[test]
fn converts_tool_call_chunks_to_provider_events() {
    let events = convert_genai_stream_event(fake_tool_call_chunk(
        0,
        "call_1",
        "ReadFiles",
        r#"{"files":"#,
    ));
    assert_eq!(events_to_debug_names(events), vec!["ToolCallChunk"]);
}

#[test]
fn end_event_preserves_usage_and_finish_reason() {
    let events = convert_genai_stream_event(fake_end_with_usage(7, 11));
    assert!(events.iter().any(|event| matches!(
        event,
        Ok(StreamEvent::End {
            usage: Some(TokenUsage {
                input_tokens: 7,
                output_tokens: 11,
                ..
            }),
            ..
        })
    )));
}
```

Use the existing test style in `crates/ai/src/provider/genai_adapter_tests.rs`. If constructing `genai::chat::ChatStreamEvent` variants directly is blocked by private fields, add narrow helper functions behind `#[cfg(test)]` in `genai_adapter.rs` that return local provider events from simple test structs.

- [ ] **Step 2: Run failing tests**

Run:

```bash
cargo test -p ai genai_adapter -- --nocapture
```

Expected: failures show tool-call and reasoning chunks are currently dropped.

- [ ] **Step 3: Convert all relevant stream event variants**

Update `convert_genai_stream_event` so:

```rust
ChatStreamEvent::ToolCallChunk(chunk) => {
    vec![Ok(StreamEvent::ToolCallChunk {
        index: chunk.index.unwrap_or(0) as usize,
        id: chunk.call_id.unwrap_or_default(),
        name: chunk.fn_name.unwrap_or_default(),
        args_fragment: chunk.fn_arguments_delta.unwrap_or_default(),
    })]
}
ChatStreamEvent::ReasoningChunk(chunk) => {
    vec![Ok(StreamEvent::ReasoningChunk(chunk.content))]
}
ChatStreamEvent::ThoughtSignatureChunk(chunk) => {
    vec![Ok(StreamEvent::ReasoningChunk(format!(
        "[thought-signature:{}]",
        chunk.content
    )))]
}
```

If the exact genai field names differ, preserve the same output contract and adapt to the current crate API.

- [ ] **Step 4: Run tests**

Run:

```bash
cargo test -p ai genai_adapter -- --nocapture
```

Expected: genai adapter tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/ai/src/provider/genai_adapter.rs crates/ai/src/provider/genai_adapter_tests.rs
git commit -m "Preserve Direct API provider stream events"
```

## Task 3: Build the Local Direct API Run Engine

**Files:**
- Create `app/src/ai/agent/api/direct_run.rs`
- Modify `app/src/ai/agent/api/direct.rs`
- Modify `app/src/ai/agent/api/mod.rs`
- Modify `app/src/ai/agent/api/impl_tests.rs`

- [ ] **Step 1: Add failing direct run tests**

Add tests in `app/src/ai/agent/api/impl_tests.rs`:

```rust
#[test]
fn direct_run_rejects_empty_success_without_text_or_tool_calls() {
    let turn = AccumulatedAssistantTurn::empty_stop();
    let err = direct_run::validate_visible_turn(&turn).unwrap_err();
    assert_eq!(
        err.to_string(),
        "Direct API provider returned no text and no tool calls"
    );
}

#[test]
fn direct_run_accepts_tool_only_turn() {
    let turn = AccumulatedAssistantTurn {
        text: String::new(),
        reasoning: Vec::new(),
        tool_calls: vec![ToolCall {
            id: "call_read".to_string(),
            name: "ReadFiles".to_string(),
            input: serde_json::json!({"files": [{"name": "Cargo.toml"}]}),
        }],
        finish_reason: FinishReason::ToolUse,
        usage: None,
    };

    assert!(direct_run::validate_visible_turn(&turn).is_ok());
}
```

- [ ] **Step 2: Run failing tests**

Run:

```bash
cargo test -p warp direct_run -- --nocapture
```

Expected: compile failure because `direct_run` does not exist.

- [ ] **Step 3: Create `direct_run` with visible-turn validation**

Create `app/src/ai/agent/api/direct_run.rs`:

```rust
use anyhow::anyhow;
use ai::provider::stream_accumulator::AccumulatedAssistantTurn;

pub fn validate_visible_turn(turn: &AccumulatedAssistantTurn) -> anyhow::Result<()> {
    if turn.text.trim().is_empty() && turn.tool_calls.is_empty() {
        return Err(anyhow!("Direct API provider returned no text and no tool calls"));
    }
    Ok(())
}
```

Wire it in `app/src/ai/agent/api/mod.rs`:

```rust
mod direct_run;
```

- [ ] **Step 4: Move stream handling into `direct_run`**

Move `run_direct_text_stream` logic out of `direct.rs` into `direct_run.rs`, then change it from event-by-event append-only handling to:

```rust
pub async fn run_direct_provider_turn(
    params: RequestParams,
    request_id: String,
    tx: async_channel::Sender<Event>,
) -> anyhow::Result<()> {
    let task_id = root_task_id(&params);
    let message_id = uuid::Uuid::new_v4().to_string();
    let mut stream = super::direct_tools::run_provider_stream(params).await?;
    let mut accumulator = ai::provider::stream_accumulator::StreamAccumulator::default();

    send_initial_output_if_needed(&tx, &task_id, &request_id, &message_id).await?;

    while let Some(event) = stream.next().await {
        let event = event?;
        if let ai::provider::StreamEvent::TextChunk(chunk) = &event {
            if !chunk.is_empty() {
                super::direct::send_client_action(
                    &tx,
                    super::direct::append_agent_output_chunk_action(
                        task_id.clone(),
                        request_id.clone(),
                        message_id.clone(),
                        chunk.clone(),
                    ),
                ).await?;
            }
        }
        accumulator.push(event)?;
    }

    let turn = accumulator.finish()?;
    validate_visible_turn(&turn)?;
    send_reasoning_and_tool_calls(&tx, &task_id, &request_id, turn).await?;
    Ok(())
}
```

Keep the helper functions private except the minimal functions needed by tests. Make `send_client_action` in `direct.rs` `pub(super)` if needed.

- [ ] **Step 5: Run tests**

Run:

```bash
cargo test -p warp direct_run -- --nocapture
cargo test -p warp direct_api_initial_actions_create_fresh_task_and_stream_message -- --nocapture
```

Expected: tests pass.

- [ ] **Step 6: Commit**

```bash
git add app/src/ai/agent/api/direct.rs app/src/ai/agent/api/direct_run.rs app/src/ai/agent/api/mod.rs app/src/ai/agent/api/impl_tests.rs
git commit -m "Add Direct API local run engine"
```

## Task 4: Add Tool Result Continuation

**Files:**
- Modify `app/src/ai/agent/api/direct_run.rs`
- Modify `app/src/ai/agent/api/direct_tools.rs`
- Modify `app/src/ai/blocklist/controller/response_stream.rs`
- Modify `app/src/ai/agent/api/impl_tests.rs`

- [ ] **Step 1: Add a test for provider history after a tool result**

Add this test beside the existing `direct_api_chat_request_preserves_current_action_result_as_tool_result` test:

```rust
#[test]
fn direct_api_followup_request_preserves_assistant_tool_call_then_tool_result() {
    let params = request_params_with_tool_call_and_result(
        "call_read",
        "ReadFiles",
        serde_json::json!({"files": [{"name": "Cargo.toml"}]}),
        "Cargo.toml contents",
    );

    let request = direct_tools::build_chat_request(&params);

    assert!(request.messages.iter().any(|message| matches!(
        message,
        ChatMessage::Assistant { tool_calls, .. }
            if tool_calls.iter().any(|call| call.id == "call_read")
    )));
    assert!(request.messages.iter().any(|message| matches!(
        message,
        ChatMessage::User(blocks)
            if blocks.iter().any(|block| matches!(
                block,
                ContentBlock::ToolResult { tool_use_id, .. } if tool_use_id == "call_read"
            ))
    )));
}
```

- [ ] **Step 2: Run the test**

Run:

```bash
cargo test -p warp direct_api_followup_request_preserves_assistant_tool_call_then_tool_result -- --nocapture
```

Expected: pass if history conversion is already sufficient; otherwise fail on missing assistant tool-call or tool-result mapping.

- [ ] **Step 3: Define the continuation state machine**

In `direct_run.rs`, model local Direct API as these states:

```rust
enum DirectRunState {
    StreamingProvider,
    WaitingForToolResults { tool_call_ids: Vec<String> },
    Completed,
    Failed,
    Cancelled,
}
```

Use it to decide whether the response stream should finish or wait for action results.

- [ ] **Step 4: Resume after tool results**

When a Direct API turn produces tool calls:

```rust
if !turn.tool_calls.is_empty() {
    emit_tool_call_messages(&tx, &task_id, &request_id, &turn.tool_calls).await?;
    emit_waiting_for_tool_results_status(&tx, &task_id, &request_id).await?;
    return Ok(DirectRunState::WaitingForToolResults {
        tool_call_ids: turn.tool_calls.iter().map(|call| call.id.clone()).collect(),
    });
}
```

Then ensure the next user/action-result input builds a provider request that includes:

- prior assistant tool-call message
- tool result message with matching `tool_use_id`
- current user context and attachments

This can reuse the existing follow-up request creation path instead of running a hidden loop in the same spawned task, because Warp's action executor already owns permission prompts and action completion.

- [ ] **Step 5: Treat unsupported tool calls as local errors**

Update `provider_tool_call_to_proto` failures so unsupported tools emit a user-visible error message and a failed stream instead of silent success.

Expected message:

```text
Direct API provider requested unsupported tool: <tool_name>
```

- [ ] **Step 6: Run tests**

Run:

```bash
cargo test -p warp direct_api_chat_request_preserves_persisted_tool_call_and_result -- --nocapture
cargo test -p warp direct_api_followup_request_preserves_assistant_tool_call_then_tool_result -- --nocapture
cargo test -p warp direct_api_unsupported_tool -- --nocapture
```

Expected: tests pass.

- [ ] **Step 7: Commit**

```bash
git add app/src/ai/agent/api/direct_run.rs app/src/ai/agent/api/direct_tools.rs app/src/ai/blocklist/controller/response_stream.rs app/src/ai/agent/api/impl_tests.rs
git commit -m "Continue Direct API runs after tool results"
```

## Task 5: Add Capability-Gated Tool and Feature Selection

**Files:**
- Modify `crates/ai/src/provider/types.rs`
- Modify `app/src/ai/agent/api/direct_tools.rs`
- Modify `app/src/ai/execution_profiles/direct_api_model_choices.rs`
- Modify `app/src/settings_view/execution_profile_view.rs`
- Modify tests under `app/src/ai/execution_profiles/`

- [ ] **Step 1: Extend capabilities**

Update `ModelCapabilities`:

```rust
pub struct ModelCapabilities {
    pub context_window: u32,
    pub supports_tools: bool,
    pub supports_parallel_tools: bool,
    pub supports_vision: bool,
    pub supports_streaming: bool,
    pub supports_reasoning: bool,
    pub supports_json_schema: bool,
    pub supports_web_search: bool,
}
```

Set conservative defaults:

```rust
supports_tools: true,
supports_parallel_tools: false,
supports_vision: false,
supports_streaming: true,
supports_reasoning: false,
supports_json_schema: false,
supports_web_search: false,
```

- [ ] **Step 2: Add model choice capability tests**

Add tests that prove:

- OpenRouter cached model metadata can expose tool support when available.
- Manual/stale model choices are marked `capabilities_known=false`.
- Unknown capabilities hide context-window and unsupported feature controls.

Run:

```bash
cargo test -p warp direct_api_choices_ -- --nocapture
```

- [ ] **Step 3: Gate Direct API tool definitions**

Change `direct_tool_definitions` to accept a capability and permission input:

```rust
pub(super) fn direct_tool_definitions(capabilities: &ModelCapabilities) -> Vec<Tool> {
    if !capabilities.supports_tools {
        return Vec::new();
    }

    vec![
        read_files_tool_definition(),
        grep_tool_definition(),
        run_shell_command_tool_definition(),
    ]
}
```

- [ ] **Step 4: Show unsupported feature hints**

In the profile summary, show concise status text for Direct API profiles:

```text
Direct API: OpenRouter / moonshotai/kimi-k2.6
Tools: supported
Vision: not available
Web search: provider-specific
```

Use existing settings row typography. Do not introduce a modal.

- [ ] **Step 5: Run tests**

Run:

```bash
cargo test -p warp direct_api_choices_ -- --nocapture
cargo test -p warp execution_profile_ -- --nocapture
```

Expected: tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/ai/src/provider/types.rs app/src/ai/agent/api/direct_tools.rs app/src/ai/execution_profiles/direct_api_model_choices.rs app/src/settings_view/execution_profile_view.rs app/src/ai/execution_profiles/*tests.rs
git commit -m "Gate Direct API features by model capability"
```

## Task 6: Remove Cloud Lifecycle Calls from Local Direct API Runs

**Files:**
- Modify `app/src/ai/agent/conversation.rs`
- Modify `app/src/ai/blocklist/task_status_sync_model.rs`
- Modify `app/src/ai/blocklist/history_model.rs`
- Modify relevant tests under `app/src/ai/blocklist/`

- [ ] **Step 1: Add a local-backend marker**

Add:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversationBackend {
    WarpProvider,
    DirectApiLocal,
}
```

Store it on `AIConversation`.

- [ ] **Step 2: Mark Direct API conversations from stream init**

When the active request has `ModelRouting::DirectApi`, set the conversation backend to `DirectApiLocal`.

Expected behavior:

- Direct API local runs do not require `server_metadata`.
- Direct API local runs can have local UUID conversation IDs.
- Direct API local runs do not fetch GraphQL metadata after completion.

- [ ] **Step 3: Skip task status sync**

In `TaskStatusSyncModel::on_conversation_status_updated`, add:

```rust
if conversation.backend() == ConversationBackend::DirectApiLocal {
    return;
}
```

- [ ] **Step 4: Add tests**

Add:

```rust
#[test]
fn task_status_sync_skips_direct_api_local_conversations() {
    let mut harness = TaskStatusSyncHarness::new();
    let conversation_id = harness.insert_direct_api_local_conversation();
    harness.set_status(conversation_id, ConversationStatus::Success);
    assert_eq!(harness.ai_client.update_agent_task_call_count(), 0);
}
```

- [ ] **Step 5: Run tests**

Run:

```bash
cargo test -p warp task_status_sync -- --nocapture
```

Expected: tests pass and existing Warp Provider task status sync tests still pass.

- [ ] **Step 6: Commit**

```bash
git add app/src/ai/agent/conversation.rs app/src/ai/blocklist/task_status_sync_model.rs app/src/ai/blocklist/history_model.rs app/src/ai/blocklist/*tests.rs
git commit -m "Keep Direct API runs local-only"
```

## Task 7: Make Direct API Errors Actionable

**Files:**
- Modify `crates/ai/src/provider/error.rs`
- Modify `crates/ai/src/provider/genai_adapter.rs`
- Modify `app/src/ai/agent/api/direct.rs`
- Modify `app/src/ai/blocklist/controller/response_stream.rs`
- Modify provider and app tests

- [ ] **Step 1: Define provider error categories**

Add:

```rust
pub enum ProviderErrorKind {
    Authentication,
    RateLimited,
    Network,
    ModelNotFound,
    UnsupportedFeature,
    InvalidToolCall,
    InvalidResponse,
    Cancelled,
    Other,
}
```

Update `ProviderError::Remote` to include `kind: ProviderErrorKind`.

- [ ] **Step 2: Map common provider errors**

In `GenaiAdapter`, classify errors by provider response where available:

- HTTP 401 or 403 -> `Authentication`
- HTTP 404 with model text -> `ModelNotFound`
- HTTP 429 -> `RateLimited`
- transport/connect timeout -> `Network`
- invalid JSON or stream parse -> `InvalidResponse`

- [ ] **Step 3: Surface direct user-facing messages**

Map local errors to messages:

```text
Direct API authentication failed for <provider>. Check the saved API key.
Direct API rate limit hit for <provider>. Try again later or choose another model.
Direct API model was not found: <model_id>.
Direct API provider returned an invalid stream response.
Direct API provider requested unsupported tool: <tool_name>.
```

- [ ] **Step 4: Add tests**

Run:

```bash
cargo test -p ai provider_error -- --nocapture
cargo test -p warp direct_api_routing_reports_route_resolution_error -- --nocapture
```

Expected: tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/ai/src/provider/error.rs crates/ai/src/provider/genai_adapter.rs app/src/ai/agent/api/direct.rs app/src/ai/blocklist/controller/response_stream.rs
git commit -m "Classify Direct API provider errors"
```

## Task 8: Add Redacted Direct API Diagnostics

**Files:**
- Modify `crates/ai/src/logging/mod.rs`
- Modify `crates/ai/src/logging/logger_tests.rs`
- Modify `crates/ai/src/provider/genai_adapter.rs`
- Modify `app/src/ai/agent/api/direct_run.rs`

- [ ] **Step 1: Add redaction tests**

Add tests that include:

- OpenAI-style key
- Anthropic-style key
- OpenRouter key
- bearer token
- custom base URL with token query parameter

Expected redacted output:

```text
Authorization: Bearer <redacted>
api_key=<redacted>
https://example.local/v1?token=<redacted>
```

- [ ] **Step 2: Log only safe run metadata**

For Direct API runs, log:

- provider enum
- model ID only when provider is public; hash custom model IDs
- request ID
- stream event counts
- text chunk count
- tool-call count
- finish reason
- error category

Do not log:

- API keys
- Authorization headers
- full prompts
- file contents
- command output
- full custom base URLs with private hostnames unless the user enables verbose local diagnostics

- [ ] **Step 3: Run tests**

Run:

```bash
cargo test -p ai logging -- --nocapture
```

Expected: redaction tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/ai/src/logging/mod.rs crates/ai/src/logging/logger_tests.rs crates/ai/src/provider/genai_adapter.rs app/src/ai/agent/api/direct_run.rs
git commit -m "Add redacted Direct API diagnostics"
```

## Task 9: Handle OSS Startup and Auth Boundaries

**Files:**
- Modify `app/src/lib.rs`
- Modify `app/src/auth/auth_state.rs`
- Modify tests in `app/src/lib.rs` or `app/src/auth/auth_state_tests.rs`

- [ ] **Step 1: Decide the OSS auth mode**

Use one of these explicit paths:

- `LocalNoLogin`: local Direct API and terminal features work without Warp sign-in.
- `PersistentLocalAuth`: Warp sign-in persists through a non-noop local secure storage implementation.

For `warp-oss`, prefer `LocalNoLogin` for Direct API features because this fork has no Warp server.

- [ ] **Step 2: Add a test proving Direct API does not require login**

```rust
#[test]
fn oss_direct_api_features_do_not_require_warp_login() {
    let launch_mode = LaunchMode::new_for_unit_test_with_channel(Channel::Oss);
    let auth_state = AuthState::initialize_for_test(&launch_mode);
    assert!(auth_state.local_direct_api_available_without_login());
}
```

- [ ] **Step 3: Keep secure storage decisions explicit**

If `LocalNoLogin` is chosen, keep noop secure storage but stop using secure-storage auth as a gate for Direct API local features.

If `PersistentLocalAuth` is chosen, replace `should_use_noop_secure_storage(Channel::Oss)` with a local file-backed secure storage implementation under `warp_core::paths::state_dir()`.

- [ ] **Step 4: Run tests**

Run:

```bash
cargo test -p warp oss_secure_storage_tests auth_state -- --nocapture
```

Expected: Direct API local availability no longer depends on a Warp login refresh token.

- [ ] **Step 5: Commit**

```bash
git add app/src/lib.rs app/src/auth/auth_state.rs app/src/*auth*tests.rs
git commit -m "Separate OSS Direct API from Warp login state"
```

## Task 10: Wire CLI and Agent Profile Selection Consistently

**Files:**
- Modify `app/src/ai/agent_sdk/driver.rs`
- Modify `app/src/ai/agent_sdk/profiles.rs`
- Modify tests under `app/src/ai/agent_sdk/`

- [ ] **Step 1: Add CLI profile routing tests**

Add tests:

```rust
#[test]
fn cli_agent_profile_list_includes_model_routing() {
    let output = render_profile_list_for_test(profile_with_direct_api());
    assert!(output.contains("Direct API"));
    assert!(output.contains("OpenRouter / moonshotai/kimi-k2.6"));
}

#[test]
fn cli_agent_run_rejects_direct_api_profile_until_driver_supports_local_backend() {
    let err = run_cli_agent_with_profile(profile_with_direct_api()).unwrap_err();
    assert!(err.to_string().contains("Direct API profiles are not supported by this CLI path"));
}
```

Once the CLI driver is wired to the same local run engine, replace the rejection test with a success test using a mock provider.

- [ ] **Step 2: Run tests**

Run:

```bash
cargo test -p warp agent_sdk profile -- --nocapture
```

Expected: tests pass.

- [ ] **Step 3: Commit**

```bash
git add app/src/ai/agent_sdk/driver.rs app/src/ai/agent_sdk/profiles.rs app/src/ai/agent_sdk/*tests.rs
git commit -m "Report Direct API routing in agent profile CLI"
```

## Task 11: Add End-to-End Direct API Mock Provider Tests

**Files:**
- Modify `crates/ai/tests/e2e_direct_provider.rs`
- Add mock stream fixtures under `crates/ai/tests/fixtures/` if needed
- Modify `app/src/ai/agent/api/impl_tests.rs`

- [ ] **Step 1: Add mock stream cases**

Cover:

- text-only stream
- reasoning plus text stream
- tool-call chunk stream split across multiple chunks
- tool-only turn followed by tool result and final text
- auth error
- rate limit error
- invalid tool JSON
- empty successful stream

- [ ] **Step 2: Run provider tests**

Run:

```bash
cargo test -p ai e2e_direct_provider -- --nocapture
```

Expected: all mock provider cases pass.

- [ ] **Step 3: Run app routing tests**

Run:

```bash
cargo test -p warp direct_api --lib -- --nocapture
```

Expected: Direct API app tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/ai/tests/e2e_direct_provider.rs crates/ai/tests/fixtures app/src/ai/agent/api/impl_tests.rs
git commit -m "Test Direct API provider parity cases"
```

## Task 12: Manual Verification Matrix

**Files:**
- Modify `docs/features/direct-api-profile-routing.md`
- Modify `docs/QUICK-START.md`

- [ ] **Step 1: Document setup**

Document:

```toml
[agents.direct_api]
selected_provider = "OpenRouter"

[agents.direct_api.api_keys]
open_router = "..."

[agents.direct_api.base_urls]
openrouter = "https://openrouter.ai/api/v1"

[agents.direct_api.selected_models]
open_router = "moonshotai/kimi-k2.6"
```

State that the path is `~/.warp-oss/settings.toml` for macOS OSS.

- [ ] **Step 2: Manual test legacy route**

Steps:

1. Start `warp-oss`.
2. Select an Agent Profile with `Warp Provider`.
3. Send a basic prompt.
4. Confirm legacy request still uses the existing server route.
5. Confirm no Direct API provider logs are emitted.

- [ ] **Step 3: Manual test Direct API text route**

Steps:

1. Select an Agent Profile with `Direct API`.
2. Select `OpenRouter / moonshotai/kimi-k2.6` or another configured model.
3. Send `Say hello in one sentence`.
4. Confirm visible text appears.
5. Confirm no GraphQL `update_agent_task` warning appears for the Direct API local run.

- [ ] **Step 4: Manual test Direct API tool route**

Steps:

1. Ask `Read Cargo.toml and summarize the package name`.
2. Confirm Warp shows a tool-call request.
3. Approve the read-file action.
4. Confirm the provider receives the tool result and returns final text.

- [ ] **Step 5: Manual test error route**

Steps:

1. Configure an invalid Direct API key.
2. Run a Direct API profile.
3. Confirm the UI shows `Direct API authentication failed`.
4. Confirm logs redact the key.

- [ ] **Step 6: Run final validation**

Run:

```bash
cargo fmt --check
git diff --check
cargo test -p ai genai_adapter stream_accumulator logging -- --nocapture
cargo test -p ai e2e_direct_provider -- --nocapture
cargo test -p warp direct_api --lib -- --nocapture
cargo test -p warp task_status_sync -- --nocapture
cargo check -p warp --bin warp-oss
```

Expected: all commands pass. Existing unrelated warnings must be listed in the completion report if they remain.

- [ ] **Step 7: Commit docs**

```bash
git add docs/features/direct-api-profile-routing.md docs/QUICK-START.md
git commit -m "Document Direct API profile routing parity"
```

## Open Risks

- `genai` event structs may not expose every provider stream field needed for chunk-level tool-call deltas. If so, either patch the adapter usage around available final captured tool calls or add provider-specific clients for providers where genai cannot expose required semantics.
- OpenRouter's Responses API tool-calling surface is documented as beta, so OpenRouter Direct API support should prefer stable Chat Completions semantics unless a model requires Responses.
- Gemini thought signatures and tool history may need provider-specific preservation. Dropping them can break follow-up reasoning in manual REST loops.
- Ollama model support for tool calls varies by model. The UI must not imply all local Ollama models support tool use.
- Some legacy Warp Provider features depend on server-side tools or metadata with no repo-local equivalent. Those should stay disabled or explicit on Direct API until implemented locally.
- The OSS login persistence issue is adjacent but user-visible. Direct API local mode should not be blocked by Warp sign-in.

## Completion Criteria

- A profile set to `Warp Provider` behaves as it does today.
- A profile set to `Direct API` sends no Direct API keys to Warp server APIs.
- Direct API text streams produce visible output.
- Direct API tool calls execute through existing permission/action flows.
- Direct API tool results are sent back to the provider and produce final text.
- Direct API local runs do not call GraphQL task-status APIs.
- Direct API provider errors are visible and categorized.
- Direct API logs redact secrets and do not capture prompt/file content by default.
- Empty provider streams are errors unless a valid no-output state is explicitly represented in the UI.
- `cargo check -p warp --bin warp-oss` passes.

## Self-Review

- Spec coverage: profile selection, legacy preservation, Direct API routing, provider streaming, tool continuation, settings storage, auth boundary, cloud lifecycle, observability, security, docs, and validation are covered.
- Placeholder scan: no implementation step depends on `TBD`, hidden follow-up work, or unspecified tests.
- Type consistency: provider stream events flow through `StreamEvent`, `StreamAccumulator`, `direct_run`, and existing `ResponseEvent` output.
