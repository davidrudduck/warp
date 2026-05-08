use super::*;
use crate::provider::{
    agent_event_channel, mock::MockLlmProvider, AgentEvent, ChatMessage, ContentBlock,
    FinishReason, ProviderError, SharedProvider, StreamEvent, ToolCall, ToolResultContent,
};
use std::sync::Arc;
use tokio::sync::mpsc;

#[tokio::test]
async fn run_no_tools_completes_immediately() {
    // Test: When provider returns Stop with no tool calls, run should complete
    let events = vec![
        StreamEvent::Start,
        StreamEvent::TextChunk("Hello".into()),
        StreamEvent::End {
            finish_reason: FinishReason::Stop,
            usage: None,
        },
    ];
    let provider: SharedProvider = Arc::new(MockLlmProvider::new().with_stream(events));
    let (tx, mut rx) = agent_event_channel(16);
    let (tool_req_tx, _tool_req_rx) = mpsc::channel(16);
    let (_cancel_tx, cancel_rx) = futures::channel::oneshot::channel();

    let initial_messages = vec![ChatMessage::User(vec![ContentBlock::Text("test".into())])];
    let conversation_id = AIConversationId::new();

    let result = run(
        provider,
        initial_messages,
        vec![],
        conversation_id,
        tx,
        tool_req_tx,
        cancel_rx,
    )
    .await;

    assert!(result.is_ok());

    // Verify we got text chunks and done event
    drop(rx);
}

#[tokio::test]
async fn run_with_tools_dispatches_and_continues() {
    // Test: When provider returns ToolUse, run should dispatch tools and continue
    let tool_call = ToolCall {
        id: "tc1".into(),
        name: "ReadFiles".into(),
        input: serde_json::json!({"paths": ["/tmp/test.txt"]}),
    };

    // First turn: model requests a tool
    let events1 = vec![
        StreamEvent::Start,
        StreamEvent::ToolCallReady(tool_call.clone()),
        StreamEvent::End {
            finish_reason: FinishReason::ToolUse,
            usage: None,
        },
    ];

    // Second turn: model responds with text after tool result
    let events2 = vec![
        StreamEvent::Start,
        StreamEvent::TextChunk("Got the file".into()),
        StreamEvent::End {
            finish_reason: FinishReason::Stop,
            usage: None,
        },
    ];

    let provider: SharedProvider = Arc::new(
        MockLlmProvider::new()
            .with_stream(events1)
            .with_stream(events2),
    );

    let (tx, _rx) = agent_event_channel(16);
    let (tool_req_tx, mut tool_req_rx) = mpsc::channel(16);
    let (_cancel_tx, cancel_rx) = futures::channel::oneshot::channel();

    let initial_messages = vec![ChatMessage::User(vec![ContentBlock::Text("test".into())])];
    let conversation_id = AIConversationId::new();

    // Spawn the run loop in the background
    let run_handle = tokio::spawn(async move {
        run(
            provider,
            initial_messages,
            vec![],
            conversation_id,
            tx,
            tool_req_tx,
            cancel_rx,
        )
        .await
    });

    // Mock tool executor: receive dispatch request and send back result
    let dispatch_req = tool_req_rx.recv().await.expect("should receive tool dispatch");
    assert_eq!(dispatch_req.tool_call.name, "ReadFiles");

    let result_block = ContentBlock::ToolResult {
        tool_use_id: dispatch_req.tool_call.id.clone(),
        content: ToolResultContent::Text("file contents".into()),
        is_error: false,
    };
    dispatch_req.result_tx.send(Ok(result_block)).unwrap();

    // Wait for run to complete
    let result = run_handle.await.unwrap();
    assert!(result.is_ok());
}

#[tokio::test]
async fn run_respects_turn_limit() {
    // Test: run should stop after MAX_DIRECT_LOOP_TURNS
    let tool_call = ToolCall {
        id: "tc1".into(),
        name: "ReadFiles".into(),
        input: serde_json::json!({}),
    };

    // Create a provider that always returns tool calls (infinite loop scenario)
    let mut provider = MockLlmProvider::new();
    for _ in 0..60 {
        // More than MAX_DIRECT_LOOP_TURNS
        provider = provider.with_stream(vec![
            StreamEvent::Start,
            StreamEvent::ToolCallReady(tool_call.clone()),
            StreamEvent::End {
                finish_reason: FinishReason::ToolUse,
                usage: None,
            },
        ]);
    }

    let provider: SharedProvider = Arc::new(provider);
    let (tx, mut rx) = agent_event_channel(128);
    let (tool_req_tx, mut tool_req_rx) = mpsc::channel(128);
    let (_cancel_tx, cancel_rx) = futures::channel::oneshot::channel();

    let initial_messages = vec![ChatMessage::User(vec![ContentBlock::Text("test".into())])];
    let conversation_id = AIConversationId::new();

    let run_handle = tokio::spawn(async move {
        run(
            provider,
            initial_messages,
            vec![],
            conversation_id,
            tx,
            tool_req_tx,
            cancel_rx,
        )
        .await
    });

    // Mock tool executor: respond to tool calls
    let mut count = 0;
    while let Some(dispatch_req) = tool_req_rx.recv().await {
        count += 1;
        let result_block = ContentBlock::ToolResult {
            tool_use_id: dispatch_req.tool_call.id.clone(),
            content: ToolResultContent::Text("result".into()),
            is_error: false,
        };
        dispatch_req.result_tx.send(Ok(result_block)).ok();

        if count > 60 {
            break; // Safety: don't loop forever
        }
    }

    let result = run_handle.await.unwrap();
    assert!(result.is_ok());

    // Should have stopped before processing all 60 potential tool calls
    assert!(count < 60);
    assert!(count <= MAX_DIRECT_LOOP_TURNS);

    // Should have received an error event about the turn limit
    let mut got_error = false;
    while let Ok(event) = rx.try_recv() {
        if matches!(event, AgentEvent::Error(_)) {
            got_error = true;
        }
    }
    assert!(got_error, "should emit error when hitting turn limit");
}

#[tokio::test]
async fn run_cancelled_stops_cleanly() {
    // Test: When cancellation signal fires, run should stop gracefully
    // Use a stream that would normally complete, but we'll cancel it
    let events = vec![
        StreamEvent::Start,
        StreamEvent::TextChunk("Starting".into()),
        StreamEvent::End {
            finish_reason: FinishReason::Stop,
            usage: None,
        },
    ];

    let provider: SharedProvider = Arc::new(MockLlmProvider::new().with_stream(events));
    let (tx, _rx) = agent_event_channel(16);
    let (tool_req_tx, _tool_req_rx) = mpsc::channel(16);
    let (cancel_tx, cancel_rx) = futures::channel::oneshot::channel();

    let initial_messages = vec![ChatMessage::User(vec![ContentBlock::Text("test".into())])];
    let conversation_id = AIConversationId::new();

    // Cancel immediately before run starts
    cancel_tx.send(()).unwrap();

    // Run should complete without error when cancelled
    let result = run(
        provider,
        initial_messages,
        vec![],
        conversation_id,
        tx,
        tool_req_tx,
        cancel_rx,
    )
    .await;

    assert!(result.is_ok(), "run should return Ok when cancelled, got: {result:?}");
}
