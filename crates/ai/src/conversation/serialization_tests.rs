use super::*;
use crate::provider::{ChatMessage, ContentBlock, ToolCall};
use serde_json::json;

#[test]
fn serialize_system_message() {
    let msg = ChatMessage::System("You are a helpful assistant".into());
    let (role, content_json, tool_calls_json) = serialize_chat_message(&msg);

    assert_eq!(role, "system");
    assert!(content_json.contains("helpful assistant"));
    assert_eq!(tool_calls_json, None);
}

#[test]
fn serialize_user_message_with_text() {
    let msg = ChatMessage::User(vec![
        ContentBlock::Text("Hello".into()),
    ]);
    let (role, content_json, tool_calls_json) = serialize_chat_message(&msg);

    assert_eq!(role, "user");
    assert!(content_json.contains("Hello"));
    assert_eq!(tool_calls_json, None);
}

#[test]
fn serialize_assistant_message_with_tool_calls() {
    let tool_call = ToolCall {
        id: "call_123".into(),
        name: "get_weather".into(),
        input: json!({"location": "SF"}),
    };
    let msg = ChatMessage::Assistant {
        text: Some("Let me check the weather".into()),
        tool_calls: vec![tool_call],
    };
    let (role, content_json, tool_calls_json) = serialize_chat_message(&msg);

    assert_eq!(role, "assistant");
    assert!(content_json.contains("check the weather"));
    assert!(tool_calls_json.is_some());
    assert!(tool_calls_json.unwrap().contains("get_weather"));
}

#[test]
fn deserialize_roundtrip() {
    let original = ChatMessage::User(vec![
        ContentBlock::Text("Test message".into()),
    ]);

    let (role, content_json, tool_calls_json) = serialize_chat_message(&original);
    let deserialized = deserialize_chat_message(&role, &content_json, tool_calls_json.as_deref());

    match deserialized {
        ChatMessage::User(blocks) => {
            assert_eq!(blocks.len(), 1);
            if let ContentBlock::Text(text) = &blocks[0] {
                assert_eq!(text, "Test message");
            } else {
                panic!("Expected Text block");
            }
        },
        _ => panic!("Expected User message"),
    }
}
