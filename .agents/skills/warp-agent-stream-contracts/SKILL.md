---
name: warp-agent-stream-contracts
description: Protects Warp agent stream and tool-call contracts. Use when changing direct_loop, genai adapters, agent event streams, action execution, cancellation, conversation hydration, tool confirmation, SSE restore, or agent UI status updates.
---

# Warp Agent Stream Contracts

Use this skill when work touches streamed AI output, tool calls, action execution, or conversation/event hydration. These paths drive the terminal-agent experience and regressions are usually ordering, cancellation, or stale-state bugs.

## Core Files

- Direct loop: `crates/ai/src/direct_loop/mod.rs`
- Provider conversion: `crates/ai/src/provider/genai_adapter.rs`
- Server request stream: `app/src/ai/agent/api/impl.rs`
- Action queue/execution: `app/src/ai/blocklist/action_model.rs`
- SSE event stream: `app/src/ai/blocklist/orchestration_event_streamer.rs`
- Agent event driver/hydration: `app/src/ai/agent_events/`

## Contracts To Preserve

- Text chunks must arrive in display order.
- Tool-call fragments must assemble without reordering IDs, names, or arguments.
- Assistant messages that contain tool calls must preserve enough history for provider follow-up turns.
- Tool results must map back to the correct tool-call ID.
- Cancellation must stop streaming and action execution without leaving stale running UI.
- Unknown or side-effecting tools must fail safe behind confirmation.
- Background stream tasks must not update stale model generations after reconnect/restore.
- UI status must reflect running, blocked, cancelled, failed, and finished states.

## Review Workflow

1. Draw the event path from source to UI:
   - provider or server source
   - stream conversion
   - history persistence
   - action dispatch
   - UI status update
2. Identify ordering keys: sequence, index, tool call ID, conversation ID, run ID, generation.
3. Check cancellation and reconnect behavior.
4. Check whether any branch drops message content, tool calls, usage, or finish reason.
5. Add tests at the lowest layer that can prove the contract.

## Common Failure Patterns

- Hardcoded tool-call index values.
- Partitioning or chaining tool calls in a way that changes order.
- Treating malformed tool JSON as null without surfacing an error where the user needs one.
- Dropping assistant tool-call history when converting between provider formats.
- Leaving a cancelled action in running state.
- Replaying hydrated messages twice after restore.

## Testing

Use focused tests before integration:

```bash
cargo test -p ai direct_loop -- --nocapture
cargo test -p ai genai_adapter -- --nocapture
cargo test -p warp agent_events -- --nocapture
cargo check -p warp --bin warp-oss
```

If behavior spans terminal UI or real action execution, use `warp-integration-test`.
