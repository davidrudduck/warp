use super::*;
use crate::provider::{
    error::ProviderError,
    types::{
        ChatMessage, ChatOptions, ChatRequest, ChatResponse, ContentBlock, FinishReason,
        StreamEvent, TokenUsage,
    },
};
use futures::StreamExt;

fn make_request() -> ChatRequest {
    ChatRequest {
        messages: vec![ChatMessage::User(vec![ContentBlock::Text("hello".into())])],
        tools: vec![],
        options: ChatOptions::default(),
    }
}

#[tokio::test]
async fn mock_chat_returns_queued_response() {
    let mock = MockLlmProvider::new().with_response(Ok(ChatResponse {
        text: Some("Hi there".into()),
        tool_calls: vec![],
        finish_reason: FinishReason::Stop,
        usage: Some(TokenUsage {
            input_tokens: 5,
            output_tokens: 3,
            cache_read_tokens: None,
        }),
    }));

    let result = mock.chat(make_request()).await.unwrap();
    assert_eq!(result.text.unwrap(), "Hi there");
}

#[tokio::test]
async fn mock_chat_returns_error() {
    let mock = MockLlmProvider::new().with_response(Err(ProviderError::Auth("invalid".into())));

    let err = mock.chat(make_request()).await.unwrap_err();
    assert!(matches!(err, ProviderError::Auth(_)));
}

#[tokio::test]
async fn mock_stream_emits_events_in_order() {
    let events = vec![
        StreamEvent::Start,
        StreamEvent::TextChunk("Hello".into()),
        StreamEvent::TextChunk(" world".into()),
        StreamEvent::End {
            finish_reason: FinishReason::Stop,
            usage: Some(TokenUsage {
                input_tokens: 3,
                output_tokens: 2,
                cache_read_tokens: None,
            }),
        },
    ];
    let mock = MockLlmProvider::new().with_stream(events);

    let mut stream = mock.chat_stream(make_request()).await.unwrap();
    let mut collected = vec![];
    while let Some(ev) = stream.next().await {
        collected.push(ev.unwrap());
    }

    assert_eq!(collected.len(), 4);
    assert!(matches!(collected[1], StreamEvent::TextChunk(_)));
}

#[tokio::test]
async fn mock_records_received_requests() {
    let mock = MockLlmProvider::new().with_response(Ok(ChatResponse {
        text: Some("ok".into()),
        tool_calls: vec![],
        finish_reason: FinishReason::Stop,
        usage: None,
    }));

    let received = mock.requests_received.clone();
    mock.chat(make_request()).await.unwrap();
    let requests = received.lock().unwrap();
    assert_eq!(requests.len(), 1);
}

#[tokio::test]
async fn mock_with_base_url_stores_url() {
    let mock = MockLlmProvider::new().with_base_url("http://localhost:8080");
    assert_eq!(mock.base_url.as_deref(), Some("http://localhost:8080"));
}

#[test]
fn provider_kind_display() {
    let kind = ProviderKind::OpenAI;
    let _ = match kind {
        ProviderKind::OpenAI => "openai",
        ProviderKind::Anthropic => "anthropic",
        ProviderKind::Google => "google",
        ProviderKind::Ollama => "ollama",
        ProviderKind::OpenAICompatible { .. } => "openai_compatible",
    };
}
