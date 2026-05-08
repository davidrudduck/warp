use crate::provider::{
    AgentEvent, AgentEventSender, ChatMessage, ChatRequest, FinishReason, ProviderError,
    SharedProvider, StreamEvent, ToolCall, TokenUsage,
};
use futures::StreamExt;

/// Drain a `ChatStream` produced by the provider, assemble `ToolCallReady`
/// events from `ToolCallChunk` fragments, and forward `AgentEvent`s to
/// `sender`.  Returns the finish reason and optional token usage when the
/// stream ends normally.
pub async fn collect_and_emit_stream(
    provider: &SharedProvider,
    request: ChatRequest,
    sender: &AgentEventSender,
) -> Result<(FinishReason, Option<TokenUsage>), ProviderError> {
    let mut stream = provider.chat_stream(request).await?;

    // Accumulate tool-call fragments keyed by index.
    let mut pending_tool_calls: Vec<(String, String, String)> = Vec::new(); // (id, name, args)

    while let Some(event) = stream.next().await {
        match event? {
            StreamEvent::Start => {}
            StreamEvent::TextChunk(text) => {
                let _ = sender.send(AgentEvent::TextChunk(text)).await;
            }
            StreamEvent::ToolCallChunk {
                index,
                id,
                name,
                args_fragment,
            } => {
                if index >= pending_tool_calls.len() {
                    pending_tool_calls.resize(index + 1, (String::new(), String::new(), String::new()));
                }
                let entry = &mut pending_tool_calls[index];
                if !id.is_empty() {
                    entry.0 = id;
                }
                if !name.is_empty() {
                    entry.1 = name;
                }
                entry.2.push_str(&args_fragment);
            }
            StreamEvent::ToolCallReady(tc) => {
                let _ = sender.send(AgentEvent::ToolCallReady(tc)).await;
            }
            StreamEvent::End {
                finish_reason,
                usage,
            } => {
                // Flush any fragment-assembled tool calls.
                for (id, name, args_str) in pending_tool_calls.drain(..) {
                    if id.is_empty() && name.is_empty() {
                        continue;
                    }
                    let input = serde_json::from_str(&args_str).unwrap_or(serde_json::Value::Null);
                    let _ = sender
                        .send(AgentEvent::ToolCallReady(ToolCall { id, name, input }))
                        .await;
                }
                let _ = sender
                    .send(AgentEvent::Done {
                        finish_reason,
                        usage: usage.clone(),
                    })
                    .await;
                return Ok((finish_reason, usage));
            }
        }
    }

    // Stream ended without an End event — treat as an error.
    let _ = sender
        .send(AgentEvent::Error("stream ended without End event".into()))
        .await;
    Err(ProviderError::StreamParse(
        "stream ended without End event".into(),
    ))
}

/// Returns `true` when the current tool calls should be presented to the user
/// for confirmation before execution.  Currently this returns `true` whenever
/// any tool calls are present and the finish reason is `ToolUse`.
pub fn requires_confirmation(finish_reason: FinishReason, tool_calls: &[ToolCall]) -> bool {
    finish_reason == FinishReason::ToolUse && !tool_calls.is_empty()
}

/// Returns `true` if a tool with this name requires user confirmation before executing.
///
/// Used by the adapter layer to pre-classify tool call batches. If ANY call in
/// a batch returns `true`, the entire batch is serialised behind a confirmation
/// modal (Anthropic requires all tool_use IDs to be answered before the next turn).
///
/// Unknown tool names default to `true` (fail-safe: require confirmation).
pub fn tool_requires_confirmation(name: &str) -> bool {
    match name {
        // Read-only / safe tools — no confirmation needed
        "ReadFiles"
        | "SearchCodebase"
        | "Grep"
        | "FileGlob"
        | "FileGlobV2"
        | "ReadShellCommandOutput"
        | "FetchConversation"
        | "ReadDocuments"
        | "ReadSkill"
        | "ReadMCPResource" => false,

        // Side-effecting tools — require confirmation
        "RequestCommandOutput"
        | "RequestFileEdits"
        | "AskUserQuestion"
        | "WriteToLongRunningShellCommand"
        | "UseComputer"
        | "InsertCodeReviewComments"
        | "CreateDocuments"
        | "EditDocuments"
        | "UploadArtifact"
        | "CallMCPTool"
        | "StartAgent"
        | "SendMessageToAgent"
        | "RunAgents"
        | "RequestComputerUse"
        | "TransferShellCommandControlToUser" => true,

        // Unknown tools: fail-safe — require confirmation
        _ => true,
    }
}

/// Returns `true` if any tool call in `batch` requires user confirmation.
/// If this returns `true`, the entire batch must be serialised (not parallelised).
pub fn batch_requires_confirmation(batch: &[ToolCall]) -> bool {
    batch.iter().any(|tc| tool_requires_confirmation(&tc.name))
}

/// Trim a message list so that the total count does not exceed `limit`.
/// System messages are always kept; the oldest non-system messages are
/// dropped first.
pub fn trim_to_context_window(messages: Vec<ChatMessage>, limit: usize) -> Vec<ChatMessage> {
    if messages.len() <= limit {
        return messages;
    }

    let system_count = messages
        .iter()
        .filter(|m| matches!(m, ChatMessage::System(_)))
        .count();

    // If the system messages alone fill the limit we can only keep those.
    if system_count >= limit {
        return messages
            .into_iter()
            .filter(|m| matches!(m, ChatMessage::System(_)))
            .collect();
    }

    let non_system_budget = limit - system_count;

    // Collect system messages in order and the last `non_system_budget`
    // non-system messages.
    let mut systems: Vec<ChatMessage> = Vec::new();
    let mut non_systems: Vec<ChatMessage> = Vec::new();

    for msg in messages {
        if matches!(msg, ChatMessage::System(_)) {
            systems.push(msg);
        } else {
            non_systems.push(msg);
        }
    }

    // Keep only the most-recent non-system messages.
    let keep_from = non_systems.len().saturating_sub(non_system_budget);
    non_systems.drain(..keep_from);

    // Re-interleave: all system messages first, then non-system.
    systems.extend(non_systems);
    systems
}

#[cfg(test)]
#[path = "stream_tests.rs"]
mod stream_tests;

#[cfg(test)]
#[path = "trim_tests.rs"]
mod trim_tests;
