use super::*;
use serde_json::json;

#[test]
fn tool_call_round_trip() {
    let tc = ToolCall {
        id: "call_abc".into(),
        name: "ReadFiles".into(),
        input: json!({"paths": ["/tmp/foo.txt"]}),
    };
    assert_eq!(tc.id, "call_abc");
    assert_eq!(tc.name, "ReadFiles");
}

#[test]
fn content_block_text() {
    let block = ContentBlock::Text("hello".into());
    match block {
        ContentBlock::Text(s) => assert_eq!(s, "hello"),
        ContentBlock::Image { .. } => panic!("expected Text"),
        ContentBlock::ToolUse { .. } => panic!("expected Text"),
        ContentBlock::ToolResult { .. } => panic!("expected Text"),
    }
}

#[test]
fn content_block_tool_use() {
    let block = ContentBlock::ToolUse {
        id: "tu_1".into(),
        name: "Grep".into(),
        input: json!({}),
    };
    match block {
        ContentBlock::ToolUse { id, .. } => assert_eq!(id, "tu_1"),
        ContentBlock::Text(_) => panic!("expected ToolUse"),
        ContentBlock::Image { .. } => panic!("expected ToolUse"),
        ContentBlock::ToolResult { .. } => panic!("expected ToolUse"),
    }
}

#[test]
fn content_block_tool_result_text() {
    let block = ContentBlock::ToolResult {
        tool_use_id: "tu_1".into(),
        content: ToolResultContent::Text("output".into()),
        is_error: false,
    };
    match block {
        ContentBlock::ToolResult {
            tool_use_id,
            is_error,
            ..
        } => {
            assert_eq!(tool_use_id, "tu_1");
            assert!(!is_error);
        }
        ContentBlock::Text(_) => panic!("expected ToolResult"),
        ContentBlock::Image { .. } => panic!("expected ToolResult"),
        ContentBlock::ToolUse { .. } => panic!("expected ToolResult"),
    }
}

#[test]
fn chat_message_system() {
    let msg = ChatMessage::System("You are helpful.".into());
    match msg {
        ChatMessage::System(s) => assert_eq!(s, "You are helpful."),
        ChatMessage::User(_) => panic!("expected System"),
        ChatMessage::Assistant { .. } => panic!("expected System"),
    }
}

#[test]
fn chat_message_user_with_text_block() {
    let msg = ChatMessage::User(vec![ContentBlock::Text("hi".into())]);
    match msg {
        ChatMessage::User(blocks) => assert_eq!(blocks.len(), 1),
        ChatMessage::System(_) => panic!("expected User"),
        ChatMessage::Assistant { .. } => panic!("expected User"),
    }
}

#[test]
fn chat_message_assistant_no_tools() {
    let msg = ChatMessage::Assistant {
        text: Some("answer".into()),
        tool_calls: vec![],
    };
    match msg {
        ChatMessage::Assistant { text, tool_calls } => {
            assert_eq!(text.unwrap(), "answer");
            assert!(tool_calls.is_empty());
        }
        ChatMessage::System(_) => panic!("expected Assistant"),
        ChatMessage::User(_) => panic!("expected Assistant"),
    }
}

#[test]
fn chat_message_assistant_with_tools() {
    let tc = ToolCall {
        id: "c1".into(),
        name: "Grep".into(),
        input: json!({}),
    };
    let msg = ChatMessage::Assistant {
        text: None,
        tool_calls: vec![tc],
    };
    match msg {
        ChatMessage::Assistant { text, tool_calls } => {
            assert!(text.is_none());
            assert_eq!(tool_calls.len(), 1);
        }
        ChatMessage::System(_) => panic!("expected Assistant"),
        ChatMessage::User(_) => panic!("expected Assistant"),
    }
}

#[test]
fn stream_event_text_chunk() {
    let ev = StreamEvent::TextChunk("hello".into());
    match ev {
        StreamEvent::TextChunk(s) => assert_eq!(s, "hello"),
        StreamEvent::Start => panic!("expected TextChunk"),
        StreamEvent::ToolCallChunk { .. } => panic!("expected TextChunk"),
        StreamEvent::ToolCallReady(_) => panic!("expected TextChunk"),
        StreamEvent::End { .. } => panic!("expected TextChunk"),
    }
}

#[test]
fn stream_event_tool_call_ready() {
    let tc = ToolCall {
        id: "c1".into(),
        name: "Grep".into(),
        input: json!({}),
    };
    let ev = StreamEvent::ToolCallReady(tc);
    match ev {
        StreamEvent::ToolCallReady(tc) => assert_eq!(tc.name, "Grep"),
        StreamEvent::Start => panic!("expected ToolCallReady"),
        StreamEvent::TextChunk(_) => panic!("expected ToolCallReady"),
        StreamEvent::ToolCallChunk { .. } => panic!("expected ToolCallReady"),
        StreamEvent::End { .. } => panic!("expected ToolCallReady"),
    }
}

#[test]
fn stream_event_end_with_usage() {
    let ev = StreamEvent::End {
        finish_reason: FinishReason::Stop,
        usage: Some(TokenUsage {
            input_tokens: 10,
            output_tokens: 5,
            cache_read_tokens: None,
        }),
    };
    match ev {
        StreamEvent::End { usage: Some(u), .. } => {
            assert_eq!(u.input_tokens, 10);
            assert_eq!(u.output_tokens, 5);
        }
        StreamEvent::End { usage: None, .. } => panic!("expected usage"),
        StreamEvent::Start => panic!("expected End"),
        StreamEvent::TextChunk(_) => panic!("expected End"),
        StreamEvent::ToolCallChunk { .. } => panic!("expected End"),
        StreamEvent::ToolCallReady(_) => panic!("expected End"),
    }
}

#[test]
fn token_usage_total() {
    let u = TokenUsage {
        input_tokens: 100,
        output_tokens: 50,
        cache_read_tokens: Some(20),
    };
    assert_eq!(u.total_tokens(), 150);
}

#[test]
fn finish_reason_exhaustive() {
    let reasons = [
        FinishReason::Stop,
        FinishReason::ToolUse,
        FinishReason::Length,
        FinishReason::Other,
    ];
    for r in reasons {
        let _ = match r {
            FinishReason::Stop => "stop",
            FinishReason::ToolUse => "tool_use",
            FinishReason::Length => "length",
            FinishReason::Other => "other",
        };
    }
}
