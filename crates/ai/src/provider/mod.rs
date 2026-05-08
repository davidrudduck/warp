pub mod error;
pub mod mock;
pub mod types;

pub use error::ProviderError;
pub use types::{
    ChatMessage, ChatOptions, ChatRequest, ChatResponse, ContentBlock, FinishReason,
    ImageMediaType, StreamEvent, TokenUsage, Tool, ToolCall, ToolResultContent,
};

use async_trait::async_trait;
use futures::Stream;
use std::pin::Pin;
use std::sync::Arc;

pub type ChatStream = Pin<Box<dyn Stream<Item = Result<StreamEvent, ProviderError>> + Send>>;

#[derive(Debug, Clone)]
pub enum ProviderKind {
    OpenAI,
    Anthropic,
    Google,
    Ollama,
    OpenAICompatible { label: String, base_url: String },
}

#[derive(Debug, Clone)]
pub struct ModelCapabilities {
    pub context_window: u32,
    pub supports_tools: bool,
    pub supports_vision: bool,
    pub supports_streaming: bool,
}

impl Default for ModelCapabilities {
    fn default() -> Self {
        Self {
            context_window: 128_000,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
        }
    }
}

#[async_trait]
pub trait LlmProvider: Send + Sync + 'static {
    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse, ProviderError>;
    async fn chat_stream(&self, req: ChatRequest) -> Result<ChatStream, ProviderError>;
    fn capabilities(&self) -> &ModelCapabilities;
    fn provider_kind(&self) -> ProviderKind;
    fn with_base_url(self, url: &str) -> Self
    where
        Self: Sized;
}

pub type SharedProvider = Arc<dyn LlmProvider>;
