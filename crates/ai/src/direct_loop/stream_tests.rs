use super::*;
use crate::provider::{
    agent_event_channel, mock::MockLlmProvider, AgentEvent, ChatMessage, ChatOptions, ChatRequest,
    ContentBlock, FinishReason, ProviderError, SharedProvider, StreamEvent, ToolCall,
};
use futures::FutureExt;
use std::sync::Arc;

fn make_request() -> ChatRequest {
    ChatRequest {
        messages: vec![ChatMessage::User(vec![ContentBlock::Text("hello".into())])],
        tools: vec![],
        options: ChatOptions::default(),
    }
}

#[tokio::test]
async fn collect_text_chunks_emits_text_events() {
    let events = vec![
        StreamEvent::Start,
        StreamEvent::TextChunk("Hello".into()),
        StreamEvent::TextChunk(", world".into()),
        StreamEvent::End {
            finish_reason: FinishReason::Stop,
            usage: None,
        },
    ];
    let provider: SharedProvider = Arc::new(MockLlmProvider::new().with_stream(events));
    let (tx, mut rx) = agent_event_channel(16);
    let (_cancel_tx, cancel_rx) = futures::channel::oneshot::channel();
    let mut cancel_signal = cancel_rx.fuse();

    let result = collect_and_emit_stream(&provider, make_request(), &tx, &mut cancel_signal).await;
    drop(tx);

    assert!(result.is_ok());
    let (finish_reason, _usage, tool_calls) = result.unwrap();
    assert_eq!(finish_reason, FinishReason::Stop);
    assert!(tool_calls.is_empty());

    let mut received = vec![];
    while let Some(ev) = rx.recv().await {
        received.push(ev);
    }

    let text_chunks: Vec<String> = received
        .iter()
        .filter_map(|ev| {
            if let AgentEvent::TextChunk(s) = ev {
                Some(s.clone())
            } else {
                None
            }
        })
        .collect();

    assert_eq!(text_chunks, vec!["Hello", ", world"]);
    assert!(received
        .iter()
        .any(|ev| matches!(ev, AgentEvent::Done { finish_reason: FinishReason::Stop, .. })));
}

#[tokio::test]
async fn collect_tool_call_ready_forwarded() {
    let tc = ToolCall {
        id: "c1".into(),
        name: "ReadFile".into(),
        input: serde_json::json!({"path": "/tmp/foo"}),
    };
    let events = vec![
        StreamEvent::Start,
        StreamEvent::ToolCallReady(tc.clone()),
        StreamEvent::End {
            finish_reason: FinishReason::ToolUse,
            usage: None,
        },
    ];
    let provider: SharedProvider = Arc::new(MockLlmProvider::new().with_stream(events));
    let (tx, mut rx) = agent_event_channel(16);
    let (_cancel_tx, cancel_rx) = futures::channel::oneshot::channel();
    let mut cancel_signal = cancel_rx.fuse();

    let result = collect_and_emit_stream(&provider, make_request(), &tx, &mut cancel_signal)
        .await
        .unwrap();
    drop(tx);

    let (_finish_reason, _usage, tool_calls_returned) = result;
    assert_eq!(tool_calls_returned.len(), 1);
    assert_eq!(tool_calls_returned[0].name, "ReadFile");
    assert_eq!(tool_calls_returned[0].id, "c1");

    let mut received = vec![];
    while let Some(ev) = rx.recv().await {
        received.push(ev);
    }

    let tool_events: Vec<_> = received
        .iter()
        .filter(|ev| matches!(ev, AgentEvent::ToolCallReady(_)))
        .collect();

    assert_eq!(tool_events.len(), 1);
    if let AgentEvent::ToolCallReady(got) = &tool_events[0] {
        assert_eq!(got.name, "ReadFile");
        assert_eq!(got.id, "c1");
    }
}

#[tokio::test]
async fn collect_tool_call_chunks_assembled() {
    let events = vec![
        StreamEvent::Start,
        StreamEvent::ToolCallChunk {
            index: 0,
            id: "c2".into(),
            name: "Grep".into(),
            args_fragment: r#"{"pat"#.into(),
        },
        StreamEvent::ToolCallChunk {
            index: 0,
            id: "".into(),
            name: "".into(),
            args_fragment: r#"tern":"foo"}"#.into(),
        },
        StreamEvent::End {
            finish_reason: FinishReason::ToolUse,
            usage: None,
        },
    ];
    let provider: SharedProvider = Arc::new(MockLlmProvider::new().with_stream(events));
    let (tx, mut rx) = agent_event_channel(16);
    let (_cancel_tx, cancel_rx) = futures::channel::oneshot::channel();
    let mut cancel_signal = cancel_rx.fuse();

    let result = collect_and_emit_stream(&provider, make_request(), &tx, &mut cancel_signal)
        .await
        .unwrap();
    drop(tx);

    let (_finish_reason, _usage, tool_calls_returned) = result;
    assert_eq!(tool_calls_returned.len(), 1);
    assert_eq!(tool_calls_returned[0].name, "Grep");
    assert_eq!(tool_calls_returned[0].input["pattern"], "foo");

    let mut received = vec![];
    while let Some(ev) = rx.recv().await {
        received.push(ev);
    }

    let tool_events: Vec<_> = received
        .iter()
        .filter(|ev| matches!(ev, AgentEvent::ToolCallReady(_)))
        .collect();

    assert_eq!(tool_events.len(), 1);
    if let AgentEvent::ToolCallReady(got) = &tool_events[0] {
        assert_eq!(got.name, "Grep");
        assert_eq!(got.input["pattern"], "foo");
    }
}

#[tokio::test]
async fn collect_stream_error_propagated() {
    let provider: SharedProvider = Arc::new(
        MockLlmProvider::new()
            .with_stream(vec![StreamEvent::Start])
            // No End event — stream terminates early.
    );
    let (tx, mut rx) = agent_event_channel(16);
    let (_cancel_tx, cancel_rx) = futures::channel::oneshot::channel();
    let mut cancel_signal = cancel_rx.fuse();

    let result = collect_and_emit_stream(&provider, make_request(), &tx, &mut cancel_signal).await;
    drop(tx);

    assert!(result.is_err());

    // The error event should have been sent.
    let mut received = vec![];
    while let Some(ev) = rx.recv().await {
        received.push(ev);
    }
    assert!(received
        .iter()
        .any(|ev| matches!(ev, AgentEvent::Error(_))));
}

#[tokio::test]
async fn collect_stream_respects_cancel_signal() {
    let events = vec![
        StreamEvent::Start,
        StreamEvent::TextChunk("Hello".into()),
        StreamEvent::TextChunk(", world".into()),
        StreamEvent::End {
            finish_reason: FinishReason::Stop,
            usage: None,
        },
    ];
    let provider: SharedProvider = Arc::new(MockLlmProvider::new().with_stream(events));
    let (tx, _rx) = agent_event_channel(16);
    let (cancel_tx, cancel_rx) = futures::channel::oneshot::channel();
    let mut cancel_signal = cancel_rx.fuse();

    // Cancel immediately.
    let _ = cancel_tx.send(());

    let result = collect_and_emit_stream(&provider, make_request(), &tx, &mut cancel_signal).await;

    assert!(result.is_err());
    if let Err(ProviderError::Cancelled) = result {
        // Expected.
    } else {
        panic!("Expected ProviderError::Cancelled, got {result:?}");
    }
}
