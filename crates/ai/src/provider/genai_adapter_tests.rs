use super::*;
use futures::StreamExt;
use serde_json::json;

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
