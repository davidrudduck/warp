use super::*;
use serde_json::json;

fn init_crypto_provider() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    });
}

fn create_test_provider() -> OpenAIProvider {
    init_crypto_provider();

    // Create a minimal client for testing that won't make actual requests
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(1))
        .build()
        .expect("Failed to create test client");

    OpenAIProvider::with_client("test-key".into(), "gpt-4o".into(), client)
}

#[test]
fn chat_converts_messages_correctly() {
    let provider = create_test_provider();

    let request = ChatRequest {
        messages: vec![
            ChatMessage::System("You are a helpful assistant.".into()),
            ChatMessage::User(vec![ContentBlock::Text("Hello".into())]),
            ChatMessage::Assistant {
                text: Some("Hi there!".into()),
                tool_calls: vec![],
            },
            ChatMessage::User(vec![ContentBlock::Text("How are you?".into())]),
        ],
        tools: vec![],
        options: ChatOptions::default(),
    };

    let openai_req = provider.build_request(&request);

    assert_eq!(openai_req.model, "gpt-4o");
    assert_eq!(openai_req.messages.len(), 4);
    assert_eq!(openai_req.messages[0].role, "system");
    assert_eq!(openai_req.messages[1].role, "user");
    assert_eq!(openai_req.messages[2].role, "assistant");
    assert_eq!(openai_req.messages[3].role, "user");
}

#[test]
fn chat_handles_tool_calls_in_messages() {
    let provider = create_test_provider();

    let request = ChatRequest {
        messages: vec![
            ChatMessage::System("You are helpful.".into()),
            ChatMessage::User(vec![ContentBlock::Text("What's the weather?".into())]),
            ChatMessage::Assistant {
                text: None,
                tool_calls: vec![ToolCall {
                    id: "call_123".into(),
                    name: "get_weather".into(),
                    input: json!({"location": "SF"}),
                }],
            },
            ChatMessage::User(vec![ContentBlock::ToolResult {
                tool_use_id: "call_123".into(),
                content: ToolResultContent::Text("Sunny, 72F".into()),
                is_error: false,
            }]),
        ],
        tools: vec![],
        options: ChatOptions::default(),
    };

    let openai_req = provider.build_request(&request);

    // Assistant message with tool_calls
    assert_eq!(openai_req.messages[2].role, "assistant");
    assert!(openai_req.messages[2].content.is_none());
    assert_eq!(openai_req.messages[2].tool_calls.as_ref().unwrap().len(), 1);
    assert_eq!(
        openai_req.messages[2].tool_calls.as_ref().unwrap()[0].id,
        "call_123"
    );

    // Tool result as role "tool"
    assert_eq!(openai_req.messages[3].role, "tool");
    assert_eq!(openai_req.messages[3].tool_call_id.as_ref().unwrap(), "call_123");
}

#[test]
fn chat_stream_parses_text_chunks() {
    let sse_data = r#"data: {"id":"chatcmpl-123","object":"chat.completion.chunk","choices":[{"delta":{"content":"Hello"},"index":0}]}

data: {"id":"chatcmpl-123","object":"chat.completion.chunk","choices":[{"delta":{"content":" world"},"index":0}]}

data: {"id":"chatcmpl-123","object":"chat.completion.chunk","choices":[{"delta":{},"index":0,"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":2}}

data: [DONE]
"#;

    let events = parse_sse_stream(sse_data.as_bytes());
    let mut collected = vec![];

    for event in events {
        let event = event.unwrap();
        collected.push(event);
    }

    assert_eq!(collected.len(), 4);

    match &collected[0] {
        StreamEvent::Start => {}
        _ => panic!("Expected Start event"),
    }

    match &collected[1] {
        StreamEvent::TextChunk(text) => assert_eq!(text, "Hello"),
        _ => panic!("Expected TextChunk"),
    }

    match &collected[2] {
        StreamEvent::TextChunk(text) => assert_eq!(text, " world"),
        _ => panic!("Expected TextChunk"),
    }

    match &collected[3] {
        StreamEvent::End { finish_reason, usage } => {
            assert_eq!(*finish_reason, FinishReason::Stop);
            assert!(usage.is_some());
            let usage = usage.as_ref().unwrap();
            assert_eq!(usage.input_tokens, 10);
            assert_eq!(usage.output_tokens, 2);
        }
        _ => panic!("Expected End event"),
    }
}

#[test]
fn chat_stream_parses_tool_calls() {
    let sse_data = r#"data: {"id":"chatcmpl-123","object":"chat.completion.chunk","choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_abc","type":"function","function":{"name":"get_weather","arguments":""}}]},"index":0}]}

data: {"id":"chatcmpl-123","object":"chat.completion.chunk","choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"loc"}}]},"index":0}]}

data: {"id":"chatcmpl-123","object":"chat.completion.chunk","choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"ation\":"}}]},"index":0}]}

data: {"id":"chatcmpl-123","object":"chat.completion.chunk","choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"SF\"}"}}]},"index":0}]}

data: {"id":"chatcmpl-123","object":"chat.completion.chunk","choices":[{"delta":{},"index":0,"finish_reason":"tool_calls"}]}

data: [DONE]
"#;

    let events = parse_sse_stream(sse_data.as_bytes());
    let mut collected = vec![];

    for event in events {
        let event = event.unwrap();
        collected.push(event);
    }

    // Should have: Start, ToolCallChunk (x4), ToolCallReady, End
    assert!(collected.len() >= 7);

    match &collected[0] {
        StreamEvent::Start => {}
        _ => panic!("Expected Start event"),
    }

    // Find the ToolCallReady event
    let tool_call_ready = collected.iter().find(|e| matches!(e, StreamEvent::ToolCallReady(_)));
    assert!(tool_call_ready.is_some());

    match tool_call_ready.unwrap() {
        StreamEvent::ToolCallReady(tc) => {
            assert_eq!(tc.id, "call_abc");
            assert_eq!(tc.name, "get_weather");
            let input = tc.input.as_object().unwrap();
            assert_eq!(input.get("location").unwrap().as_str().unwrap(), "SF");
        }
        _ => panic!("Expected ToolCallReady"),
    }

    // Last event should be End
    match collected.last().unwrap() {
        StreamEvent::End { finish_reason, .. } => {
            assert_eq!(*finish_reason, FinishReason::ToolUse);
        }
        _ => panic!("Expected End event"),
    }
}

#[test]
fn chat_stream_handles_done_event() {
    let sse_data = r#"data: {"id":"chatcmpl-123","object":"chat.completion.chunk","choices":[{"delta":{"content":"Hi"},"index":0}]}

data: [DONE]
"#;

    let events = parse_sse_stream(sse_data.as_bytes());
    let collected: Vec<_> = events.collect::<Result<Vec<_>, _>>().unwrap();

    assert_eq!(collected.len(), 2);

    match &collected[0] {
        StreamEvent::Start => {}
        _ => panic!("Expected Start"),
    }

    match &collected[1] {
        StreamEvent::TextChunk(text) => assert_eq!(text, "Hi"),
        _ => panic!("Expected TextChunk"),
    }

    // Stream should end cleanly after [DONE]
}

#[test]
fn parse_sse_handles_empty_lines() {
    let sse_data = "data: {\"id\":\"test\"}\n\n\ndata: {\"id\":\"test2\"}\n\n";

    let events = parse_sse_stream(sse_data.as_bytes());
    let collected: Vec<_> = events.collect::<Result<Vec<_>, _>>().unwrap();

    // Empty lines should be skipped
    assert!(collected.len() >= 2);
}
