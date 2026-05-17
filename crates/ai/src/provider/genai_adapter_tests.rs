use super::*;
use futures::StreamExt;
use serde_json::json;

#[test]
fn converts_assistant_tool_calls_to_genai_messages() {
    let req = ChatRequest {
        messages: vec![ChatMessage::Assistant {
            text: Some("I need a file".to_string()),
            tool_calls: vec![ToolCall {
                id: "call-1".to_string(),
                name: "ReadFiles".to_string(),
                input: serde_json::json!({"files":[{"name":"Cargo.toml"}]}),
            }],
        }],
        tools: Vec::new(),
        options: ChatOptions::default(),
    };

    let genai_req = convert_to_genai_request(req);

    assert_eq!(genai_req.messages.len(), 2);
    let text = genai_req.messages[0]
        .content
        .joined_texts()
        .expect("assistant text should be present");
    assert!(text.contains("I need a file"));

    let tool_calls = genai_req.messages[1].content.tool_calls();
    assert_eq!(tool_calls.len(), 1);
    assert_eq!(tool_calls[0].call_id, "call-1");
    assert_eq!(tool_calls[0].fn_name, "ReadFiles");
    assert_eq!(tool_calls[0].fn_arguments["files"][0]["name"], "Cargo.toml");
}

#[test]
fn converts_user_tool_results_without_dropping_text() {
    let req = ChatRequest {
        messages: vec![ChatMessage::User(vec![
            ContentBlock::ToolResult {
                tool_use_id: "call-1".to_string(),
                content: ToolResultContent::Text("file contents".to_string()),
                is_error: false,
            },
            ContentBlock::Text("continue".to_string()),
        ])],
        tools: Vec::new(),
        options: ChatOptions::default(),
    };

    let genai_req = convert_to_genai_request(req);

    assert_eq!(genai_req.messages.len(), 2);
    let text = genai_req.messages[0]
        .content
        .joined_texts()
        .expect("user text should be present");
    assert!(text.contains("continue"));

    let tool_responses = genai_req.messages[1].content.tool_responses();
    assert_eq!(tool_responses.len(), 1);
    assert_eq!(tool_responses[0].call_id, "call-1");
    assert_eq!(tool_responses[0].content, "file contents");
}

#[test]
fn converts_nested_tool_result_blocks_to_text() {
    let text = content_blocks_to_text(&[ContentBlock::ToolResult {
        tool_use_id: "call-2".to_string(),
        content: ToolResultContent::Blocks(vec![
            ContentBlock::Text("first".to_string()),
            ContentBlock::ToolUse {
                id: "nested".to_string(),
                name: "NestedTool".to_string(),
                input: serde_json::json!({"ok":true}),
            },
        ]),
        is_error: true,
    }]);

    assert!(text.contains("Tool result for call-2 (error): first"));
    assert!(text.contains("Tool call nested NestedTool"));
}

#[test]
fn with_base_url_sets_openai_compatible_provider_kind() {
    let adapter =
        GenaiAdapter::new("custom", "test-key", "model").with_base_url("https://example.test");

    match adapter.provider_kind() {
        ProviderKind::OpenAICompatible { label, base_url } => {
            assert_eq!(label, "custom");
            assert_eq!(base_url, "https://example.test/v1/");
        }
        other => panic!("expected OpenAICompatible provider kind, got {other:?}"),
    }
}

#[tokio::test]
async fn openrouter_stream_sends_authorization_header_to_custom_endpoint() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/chat/completions")
        .match_header("authorization", "Bearer test-openrouter-key")
        .with_status(200)
        .with_header("content-type", "text/event-stream")
        .with_body(
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"ok\"}}]}\n\n\
             data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n\
             data: [DONE]\n\n",
        )
        .create_async()
        .await;

    let adapter = GenaiAdapter::new("openrouter", "test-openrouter-key", "moonshotai/kimi-k2.6")
        .with_base_url(&server.url());
    let request = ChatRequest {
        messages: vec![ChatMessage::User(vec![ContentBlock::Text("hello".into())])],
        tools: vec![],
        options: Default::default(),
    };

    let mut stream = adapter
        .chat_stream(request)
        .await
        .expect("stream should start");
    while let Some(event) = stream.next().await {
        event.expect("stream event should parse");
    }

    mock.assert_async().await;
}

#[test]
fn genai_http_error_status_uses_structured_status() {
    let error = genai::Error::HttpError {
        status: reqwest::StatusCode::UNAUTHORIZED,
        canonical_reason: "Unauthorized".to_string(),
        body: "{\"error\":{\"message\":\"User not found.\"}}".to_string(),
    };

    assert_eq!(genai_error_http_status(&error), Some(401));
}

#[test]
fn openrouter_401_maps_to_provider_auth_without_body() {
    let error = genai::Error::HttpError {
        status: reqwest::StatusCode::UNAUTHORIZED,
        canonical_reason: "Unauthorized".to_string(),
        body: "{\"error\":{\"message\":\"User not found.\"}}".to_string(),
    };

    let provider_error = provider_error_from_genai_error("openrouter", error.to_string(), &error);

    assert!(matches!(
        provider_error,
        ProviderError::Auth(message)
            if message == "OpenRouter rejected the saved API key"
    ));
}

#[test]
fn openrouter_web_stream_401_maps_to_provider_auth_without_body() {
    let error = genai::Error::WebStream {
        model_iden: genai::ModelIden::new(
            genai::adapter::AdapterKind::OpenAI,
            "moonshotai/kimi-k2.6",
        ),
        cause: "HTTP error.\nStatus: 401 Unauthorized\nBody: {\"error\":{\"message\":\"User not found.\"}}".to_string(),
        error: std::io::Error::other("stream failed").into(),
    };

    let provider_error = provider_error_from_genai_error("openrouter", error.to_string(), &error);

    assert!(matches!(
        provider_error,
        ProviderError::Auth(message)
            if message == "OpenRouter rejected the saved API key"
    ));
}

#[test]
fn openrouter_403_preserves_http_error_context() {
    let error = genai::Error::HttpError {
        status: reqwest::StatusCode::FORBIDDEN,
        canonical_reason: "Forbidden".to_string(),
        body: "{\"error\":{\"message\":\"insufficient credits\"}}".to_string(),
    };

    let provider_error = provider_error_from_genai_error("openrouter", error.to_string(), &error);

    assert!(matches!(
        provider_error,
        ProviderError::Http { status: 403, body }
            if body.contains("insufficient credits")
    ));
}

#[test]
fn provider_403_preserves_http_error_context() {
    let error = genai::Error::HttpError {
        status: reqwest::StatusCode::FORBIDDEN,
        canonical_reason: "Forbidden".to_string(),
        body: "{\"error\":{\"message\":\"forbidden\"}}".to_string(),
    };

    let provider_error = provider_error_from_genai_error("openai", error.to_string(), &error);

    assert!(matches!(
        provider_error,
        ProviderError::Http { status: 403, body }
            if body.contains("forbidden")
    ));
}

#[test]
fn stream_end_emits_complete_tool_calls_before_end() {
    let events = convert_genai_stream_event(ChatStreamEvent::End(genai::chat::StreamEnd {
        captured_content: Some(genai::chat::MessageContent::from_tool_calls(vec![
            genai::chat::ToolCall {
                call_id: "call-1".to_string(),
                fn_name: "ReadFiles".to_string(),
                fn_arguments: serde_json::json!({"files":[{"name":"Cargo.toml"}]}),
                thought_signatures: None,
            },
        ])),
        ..Default::default()
    }));

    assert_eq!(events.len(), 2);
    match events[0].as_ref().expect("first event should succeed") {
        StreamEvent::ToolCallReady(tool_call) => {
            assert_eq!(tool_call.id, "call-1");
            assert_eq!(tool_call.name, "ReadFiles");
            assert_eq!(tool_call.input["files"][0]["name"], "Cargo.toml");
        }
        other => panic!("expected ToolCallReady, got {other:?}"),
    }
    match events[1].as_ref().expect("second event should succeed") {
        StreamEvent::End { finish_reason, .. } => assert_eq!(*finish_reason, FinishReason::ToolUse),
        other => panic!("expected End, got {other:?}"),
    }
}

#[test]
fn stream_options_capture_tool_calls() {
    assert_eq!(genai_stream_options().capture_tool_calls, Some(true));
}

#[tokio::test]
#[ignore] // Requires OPENAI_API_KEY
async fn genai_chat_basic_text() {
    let api_key =
        std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY must be set for this test");

    let adapter = GenaiAdapter::new("openai", &api_key, "gpt-4o");
    let request = ChatRequest {
        messages: vec![ChatMessage::User(vec![ContentBlock::Text("Hello".into())])],
        tools: vec![],
        options: Default::default(),
    };

    let response = adapter.chat(request).await.unwrap();
    assert!(response.text.is_some());
    assert_eq!(response.finish_reason, FinishReason::Stop);
}

#[tokio::test]
#[ignore] // Requires OPENAI_API_KEY
async fn test_tool_calls() {
    // Test: Agent calls a "get_weather" tool via genai
    // Input: "What's the weather in SF?" + tool definition
    // Expected: ToolCall with name="get_weather", input contains "San Francisco"
    // This test MUST FAIL initially (tool support not implemented)

    let api_key =
        std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY must be set for this test");

    let adapter = GenaiAdapter::new("openai", &api_key, "gpt-4o");
    let tools = vec![Tool {
        name: "get_weather".into(),
        description: "Get weather for a location".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "location": {
                    "type": "string",
                    "description": "The city and state, e.g. San Francisco, CA"
                }
            },
            "required": ["location"]
        }),
    }];

    let request = ChatRequest {
        messages: vec![ChatMessage::User(vec![ContentBlock::Text(
            "What's the weather in SF?".into(),
        )])],
        tools,
        options: Default::default(),
    };

    let response = adapter.chat(request).await.unwrap();
    assert!(
        !response.tool_calls.is_empty(),
        "Expected at least one tool call"
    );
    assert_eq!(response.tool_calls[0].name, "get_weather");
    assert_eq!(response.finish_reason, FinishReason::ToolUse);

    // Verify the input contains location information
    let input = &response.tool_calls[0].input;
    assert!(
        input.get("location").is_some(),
        "Tool call should have location parameter"
    );
}

// TDD Cycle 4: Multi-Provider Compatibility Tests
// These tests validate that genai works with all 4 target providers

#[tokio::test]
#[ignore] // Requires OPENAI_API_KEY environment variable
async fn test_openai_provider() {
    let api_key =
        std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY must be set for this test");

    let adapter = GenaiAdapter::new("openai", &api_key, "gpt-4o");
    let request = ChatRequest {
        messages: vec![ChatMessage::User(vec![ContentBlock::Text(
            "What is 2+2? Answer with just the number.".into(),
        )])],
        tools: vec![],
        options: Default::default(),
    };

    let response = adapter
        .chat(request)
        .await
        .expect("OpenAI provider should respond successfully");

    assert!(response.text.is_some(), "OpenAI should return text");
    let text = response.text.unwrap();
    assert!(
        text.contains("4"),
        "Response should contain '4', got: {}",
        text
    );
}

#[tokio::test]
#[ignore] // Requires ANTHROPIC_API_KEY environment variable
async fn test_anthropic_provider() {
    let api_key =
        std::env::var("ANTHROPIC_API_KEY").expect("ANTHROPIC_API_KEY must be set for this test");

    let adapter = GenaiAdapter::new("anthropic", &api_key, "claude-3-5-sonnet-20241022");
    let request = ChatRequest {
        messages: vec![ChatMessage::User(vec![ContentBlock::Text(
            "What is 2+2? Answer with just the number.".into(),
        )])],
        tools: vec![],
        options: Default::default(),
    };

    let response = adapter
        .chat(request)
        .await
        .expect("Anthropic provider should respond successfully");

    assert!(response.text.is_some(), "Anthropic should return text");
    let text = response.text.unwrap();
    assert!(
        text.contains("4"),
        "Response should contain '4', got: {}",
        text
    );
}

#[tokio::test]
async fn test_ollama_provider() {
    // Check if Ollama is running locally before attempting the test
    let check_result = tokio::net::TcpStream::connect("127.0.0.1:11434").await;
    if check_result.is_err() {
        eprintln!("Skipping test_ollama_provider: Ollama not running on localhost:11434");
        return;
    }

    let adapter = GenaiAdapter::new("ollama", "", "llama3");
    let request = ChatRequest {
        messages: vec![ChatMessage::User(vec![ContentBlock::Text(
            "What is 2+2? Answer with just the number.".into(),
        )])],
        tools: vec![],
        options: Default::default(),
    };

    let response = adapter
        .chat(request)
        .await
        .expect("Ollama provider should respond successfully");

    assert!(response.text.is_some(), "Ollama should return text");
    let text = response.text.unwrap();
    assert!(
        text.contains("4"),
        "Response should contain '4', got: {}",
        text
    );
}

#[tokio::test]
#[ignore] // Requires GEMINI_API_KEY environment variable
async fn test_gemini_provider() {
    let api_key =
        std::env::var("GEMINI_API_KEY").expect("GEMINI_API_KEY must be set for this test");

    let adapter = GenaiAdapter::new("gemini", &api_key, "gemini-2.0-flash");
    let request = ChatRequest {
        messages: vec![ChatMessage::User(vec![ContentBlock::Text(
            "What is 2+2? Answer with just the number.".into(),
        )])],
        tools: vec![],
        options: Default::default(),
    };

    let response = adapter
        .chat(request)
        .await
        .expect("Gemini provider should respond successfully");

    assert!(response.text.is_some(), "Gemini should return text");
    let text = response.text.unwrap();
    assert!(
        text.contains("4"),
        "Response should contain '4', got: {}",
        text
    );
}

// TDD Cycle 3: Streaming Support Test
#[tokio::test]
#[ignore] // Requires OPENAI_API_KEY
async fn test_streaming() {
    let api_key = std::env::var("OPENAI_API_KEY").unwrap_or_else(|_| "test-key".to_string());

    let adapter = GenaiAdapter::new("openai", &api_key, "gpt-4o");
    let request = ChatRequest {
        messages: vec![ChatMessage::User(vec![ContentBlock::Text(
            "Write a haiku about coding".into(),
        )])],
        tools: vec![],
        options: Default::default(),
    };

    let mut stream = adapter.chat_stream(request).await.unwrap();
    let mut events = vec![];

    while let Some(event) = stream.next().await {
        events.push(event.unwrap());
    }

    // Verify stream lifecycle
    assert!(
        matches!(events[0], StreamEvent::Start),
        "First event should be Start"
    );
    assert!(
        events
            .iter()
            .filter(|e| matches!(e, StreamEvent::TextChunk(_)))
            .count()
            >= 3,
        "Should have multiple text chunks"
    );
    assert!(
        matches!(events.last().unwrap(), StreamEvent::End { .. }),
        "Last event should be End"
    );
}
