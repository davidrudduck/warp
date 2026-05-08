use super::{
    types::{ChatRequest, ChatResponse, StreamEvent},
    ChatStream, LlmProvider, ModelCapabilities, ProviderError, ProviderKind,
};
use async_trait::async_trait;
use futures::stream;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

pub struct MockLlmProvider {
    responses: Mutex<VecDeque<Result<ChatResponse, ProviderError>>>,
    stream_sequences: Mutex<VecDeque<Vec<StreamEvent>>>,
    pub requests_received: Arc<Mutex<Vec<ChatRequest>>>,
    capabilities: ModelCapabilities,
    pub base_url: Option<String>,
}

impl MockLlmProvider {
    pub fn new() -> Self {
        Self {
            responses: Mutex::new(VecDeque::new()),
            stream_sequences: Mutex::new(VecDeque::new()),
            requests_received: Arc::new(Mutex::new(Vec::new())),
            capabilities: ModelCapabilities::default(),
            base_url: None,
        }
    }

    pub fn with_response(self, response: Result<ChatResponse, ProviderError>) -> Self {
        self.responses.lock().unwrap().push_back(response);
        self
    }

    pub fn with_stream(self, events: Vec<StreamEvent>) -> Self {
        self.stream_sequences.lock().unwrap().push_back(events);
        self
    }
}

impl Default for MockLlmProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LlmProvider for MockLlmProvider {
    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse, ProviderError> {
        self.requests_received.lock().unwrap().push(req);
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Err(ProviderError::StreamParse("no response queued".into())))
    }

    async fn chat_stream(&self, req: ChatRequest) -> Result<ChatStream, ProviderError> {
        self.requests_received.lock().unwrap().push(req);
        let events = self
            .stream_sequences
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_default();
        let s = stream::iter(events.into_iter().map(Ok));
        Ok(Box::pin(s))
    }

    fn capabilities(&self) -> &ModelCapabilities {
        &self.capabilities
    }

    fn provider_kind(&self) -> ProviderKind {
        ProviderKind::OpenAI
    }

    fn with_base_url(mut self, url: &str) -> Self {
        self.base_url = Some(url.to_owned());
        self
    }
}

#[cfg(test)]
#[path = "mock_tests.rs"]
mod mock_tests;
