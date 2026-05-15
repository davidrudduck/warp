use crate::conversation::repository::ConversationRepository;
use crate::provider::{
    AgentEvent, AgentEventSender, ChatMessage, ChatRequest, ContentBlock, FinishReason,
    ProviderError, SharedProvider, StreamEvent, TokenUsage, Tool, ToolCall,
};
use futures::{FutureExt, StreamExt};
use std::sync::Arc;
use tokio::sync::mpsc;

type FusedCancel = futures::future::Fuse<futures::channel::oneshot::Receiver<()>>;

/// Placeholder type for conversation ID (actual implementation is in app crate).
/// This module will accept it as a generic parameter in production.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AIConversationId(uuid::Uuid);

impl AIConversationId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }

    pub fn from_string(s: &str) -> Result<Self, uuid::Error> {
        Ok(Self(uuid::Uuid::parse_str(s)?))
    }
}

impl Default for AIConversationId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for AIConversationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Request to dispatch a tool call on the main thread.
/// Background task sends this; main thread executor receives and processes.
#[derive(Debug)]
pub struct ToolDispatchRequest {
    pub tool_call: ToolCall,
    pub index: usize,
    pub conversation_id: AIConversationId,
    pub result_tx: futures::channel::oneshot::Sender<Result<ContentBlock, ProviderError>>,
}

/// Drain a `ChatStream` produced by the provider, assemble `ToolCallReady`
/// events from `ToolCallChunk` fragments, and forward `AgentEvent`s to
/// `sender`.  Returns the finish reason, optional token usage, and collected
/// tool calls when the stream ends normally.
pub async fn collect_and_emit_stream(
    provider: &SharedProvider,
    request: ChatRequest,
    sender: &AgentEventSender,
    mut cancel: &mut FusedCancel,
) -> Result<(FinishReason, Option<TokenUsage>, Vec<ToolCall>), ProviderError> {
    let mut stream = futures::select_biased! {
        _ = cancel => return Err(ProviderError::Cancelled),
        stream = provider.chat_stream(request).fuse() => stream?,
    };

    // Accumulate tool-call fragments keyed by index.
    let mut pending_tool_calls: Vec<(String, String, String)> = Vec::new(); // (id, name, args)
    let mut collected_tool_calls: Vec<ToolCall> = Vec::new();

    loop {
        futures::select_biased! {
            _ = cancel => return Err(ProviderError::Cancelled),
            event = stream.next().fuse() => match event {
                Some(Ok(StreamEvent::Start)) => {}
                Some(Ok(StreamEvent::TextChunk(text))) => {
                    let _ = sender.send(AgentEvent::TextChunk(text)).await;
                }
                Some(Ok(StreamEvent::ToolCallChunk {
                    index,
                    id,
                    name,
                    args_fragment,
                })) => {
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
                Some(Ok(StreamEvent::ToolCallReady(tc))) => {
                    collected_tool_calls.push(tc.clone());
                    let _ = sender.send(AgentEvent::ToolCallReady(tc)).await;
                }
                Some(Ok(StreamEvent::End {
                    finish_reason,
                    usage,
                })) => {
                    // Flush any fragment-assembled tool calls.
                    for (id, name, args_str) in pending_tool_calls.drain(..) {
                        if id.is_empty() && name.is_empty() {
                            continue;
                        }
                        let input = serde_json::from_str(&args_str).unwrap_or(serde_json::Value::Null);
                        let tc = ToolCall { id, name, input };
                        collected_tool_calls.push(tc.clone());
                        let _ = sender
                            .send(AgentEvent::ToolCallReady(tc))
                            .await;
                    }
                    let _ = sender
                        .send(AgentEvent::Done {
                            finish_reason,
                            usage: usage.clone(),
                        })
                        .await;
                    return Ok((finish_reason, usage, collected_tool_calls));
                }
                Some(Err(e)) => return Err(e),
                None => {
                    // Stream ended without an End event — treat as an error.
                    let _ = sender
                        .send(AgentEvent::Error("stream ended without End event".into()))
                        .await;
                    return Err(ProviderError::StreamParse(
                        "stream ended without End event".into(),
                    ));
                }
            }
        }
    }
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
pub fn trim_to_context_window(messages: &[ChatMessage], limit: usize) -> Vec<ChatMessage> {
    if messages.len() <= limit {
        return messages.to_vec();
    }

    let system_count = messages
        .iter()
        .filter(|m| matches!(m, ChatMessage::System(_)))
        .count();

    // If the system messages alone fill the limit we can only keep those.
    if system_count >= limit {
        return messages
            .iter()
            .filter(|m| matches!(m, ChatMessage::System(_)))
            .cloned()
            .collect();
    }

    let non_system_budget = limit - system_count;

    // Collect system messages in order and the last `non_system_budget`
    // non-system messages.
    let mut systems: Vec<ChatMessage> = Vec::new();
    let mut non_systems: Vec<ChatMessage> = Vec::new();

    for msg in messages {
        if matches!(msg, ChatMessage::System(_)) {
            systems.push(msg.clone());
        } else {
            non_systems.push(msg.clone());
        }
    }

    // Keep only the most-recent non-system messages.
    let keep_from = non_systems.len().saturating_sub(non_system_budget);
    non_systems.drain(..keep_from);

    // Re-interleave: all system messages first, then non-system.
    systems.extend(non_systems);
    systems
}

/// Safety cap: prevent runaway agent loops from exhausting provider quota.
pub const MAX_DIRECT_LOOP_TURNS: usize = 50;

/// Main direct-mode agent loop.
/// Runs chat_stream → collect events → dispatch tools → loop until Stop.
#[allow(clippy::too_many_arguments)]
pub async fn run(
    provider: SharedProvider,
    initial_messages: Vec<ChatMessage>,
    tools: Vec<Tool>,
    conversation_id: AIConversationId,
    tx: AgentEventSender,
    tool_req_tx: mpsc::Sender<ToolDispatchRequest>,
    cancellation_rx: futures::channel::oneshot::Receiver<()>,
    repository: Option<Arc<ConversationRepository>>,
) -> Result<(), ProviderError> {
    let mut history = initial_messages;
    let mut cancel = cancellation_rx.fuse();

    loop {
        let request = ChatRequest {
            messages: trim_to_context_window(&history, 100),
            tools: tools.clone(),
            options: Default::default(),
        };

        // Call collect_and_emit_stream, checking cancellation within it
        let stream_result = collect_and_emit_stream(&provider, request, &tx, &mut cancel).await;

        // Check if we were cancelled
        if matches!(stream_result, Err(ProviderError::Cancelled)) {
            return Ok(());
        }

        let (_finish_reason, _usage, tool_calls) = stream_result?;

        // Build assistant message
        history.push(ChatMessage::Assistant {
            text: None,
            tool_calls: tool_calls.clone(),
        });

        // Save to repository if provided
        if let Some(ref repo) = repository {
            repo.save_messages(conversation_id.to_string(), history.clone())
                .await
                .map_err(|e| ProviderError::StreamParse(format!("Failed to save: {e}")))?;
        }

        if tool_calls.is_empty() {
            break;
        }

        // Enforce per-session safety cap before dispatching tools
        if history.len() > MAX_DIRECT_LOOP_TURNS {
            let _ = tx
                .send(AgentEvent::Error(
                    "Direct-mode agent reached the maximum turn limit.".into(),
                ))
                .await;
            break;
        }

        let requires_serial = batch_requires_confirmation(&tool_calls);
        let mut results: Vec<(usize, ContentBlock)> = Vec::new();

        if requires_serial {
            // Serialize all calls in original order when any call needs confirmation.
            for (i, tc) in tool_calls.into_iter().enumerate() {
                let dispatch = dispatch_one(tc, i, conversation_id, &tool_req_tx).fuse();
                futures::pin_mut!(dispatch);
                futures::select_biased! {
                    _ = cancel => return Ok(()),
                    block = dispatch => results.push(block?),
                }
            }
        } else {
            // Safe to dispatch concurrently with FuturesUnordered
            use futures::stream::FuturesUnordered;

            let mut pending: FuturesUnordered<_> = tool_calls
                .into_iter()
                .enumerate()
                .map(|(i, tc)| {
                    let tool_req_tx = tool_req_tx.clone();
                    async move { dispatch_one(tc, i, conversation_id, &tool_req_tx).await }
                })
                .collect();

            loop {
                futures::select_biased! {
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

/// Dispatch a single tool call and return the result as a ContentBlock.
/// This is a mock implementation for Phase 4a - the real adapter layer doesn't exist yet.
async fn dispatch_one(
    tool_call: ToolCall,
    index: usize,
    conversation_id: AIConversationId,
    tool_req_tx: &mpsc::Sender<ToolDispatchRequest>,
) -> Result<(usize, ContentBlock), ProviderError> {
    let (result_tx, result_rx) = futures::channel::oneshot::channel();

    let req = ToolDispatchRequest {
        tool_call,
        index,
        conversation_id,
        result_tx,
    };

    tool_req_tx
        .send(req)
        .await
        .map_err(|_| ProviderError::StreamParse("tool dispatch channel closed".into()))?;

    let result_block = result_rx
        .await
        .map_err(|_| ProviderError::StreamParse("tool result channel closed".into()))??;

    Ok((index, result_block))
}

#[cfg(test)]
#[path = "stream_tests.rs"]
mod stream_tests;

#[cfg(test)]
#[path = "trim_tests.rs"]
mod trim_tests;

#[cfg(test)]
#[path = "run_tests.rs"]
mod run_tests;
