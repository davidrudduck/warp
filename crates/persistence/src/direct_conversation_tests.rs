use super::*;
use chrono::Utc;

#[test]
fn new_direct_conversation_can_be_constructed() {
    let new_conv = NewDirectConversation {
        conversation_id: "test-conv-123".into(),
        provider_kind: "openai".into(),
        model_id: "gpt-4o".into(),
        created_at: Utc::now().naive_utc(),
        last_message_at: Utc::now().naive_utc(),
        title: Some("Test conversation".into()),
    };

    assert_eq!(new_conv.conversation_id, "test-conv-123");
    assert_eq!(new_conv.provider_kind, "openai");
    assert_eq!(new_conv.model_id, "gpt-4o");
    assert_eq!(new_conv.title, Some("Test conversation".into()));
}

#[test]
fn new_direct_message_can_be_constructed() {
    let conv_id = "test-conv-456";
    let new_msg = NewDirectMessage {
        conversation_id: conv_id.into(),
        message_index: 0,
        role: "user".into(),
        content_json: r#"[{"Text":"Hello"}]"#.into(),
        tool_calls_json: None,
        input_tokens: None,
        output_tokens: None,
        created_at: Utc::now().naive_utc(),
    };

    assert_eq!(new_msg.conversation_id, conv_id);
    assert_eq!(new_msg.message_index, 0);
    assert_eq!(new_msg.role, "user");
    assert_eq!(new_msg.content_json, r#"[{"Text":"Hello"}]"#);
}

#[test]
fn direct_conversation_types_have_debug_impl() {
    let conv = DirectConversation {
        id: 1,
        conversation_id: "test-123".into(),
        provider_kind: "anthropic".into(),
        model_id: "claude-3-5-sonnet-20241022".into(),
        created_at: Utc::now().naive_utc(),
        last_message_at: Utc::now().naive_utc(),
        title: None,
        message_count: 0,
        total_tokens: 0,
    };

    let debug_str = format!("{:?}", conv);
    assert!(debug_str.contains("DirectConversation"));
    assert!(debug_str.contains("test-123"));
}

#[test]
fn direct_message_types_have_debug_impl() {
    let msg = DirectMessage {
        id: 1,
        conversation_id: "test-123".into(),
        message_index: 0,
        role: "assistant".into(),
        content_json: r#"[{"Text":"Hi there"}]"#.into(),
        tool_calls_json: Some(r#"[{"id":"call_1","name":"get_weather"}]"#.into()),
        input_tokens: Some(100),
        output_tokens: Some(50),
        created_at: Utc::now().naive_utc(),
    };

    let debug_str = format!("{:?}", msg);
    assert!(debug_str.contains("DirectMessage"));
    assert!(debug_str.contains("test-123"));
}
