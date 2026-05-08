pub mod error;
pub mod mock;
pub mod openai;
pub mod types;

pub use error::ProviderError;
pub use openai::OpenAIProvider;
pub use types::{
    ChatMessage, ChatOptions, ChatRequest, ChatResponse, ContentBlock, FinishReason,
    ImageMediaType, StreamEvent, TokenUsage, Tool, ToolCall, ToolResultContent,
};

use async_trait::async_trait;
use futures::Stream;
use std::pin::Pin;
use std::sync::Arc;

pub type ChatStream = Pin<Box<dyn Stream<Item = Result<StreamEvent, ProviderError>> + Send>>;

/// High-level events emitted by the direct loop as it processes a stream.
#[derive(Debug, Clone)]
pub enum AgentEvent {
    /// A text chunk arrived from the model.
    TextChunk(String),
    /// A tool call is fully assembled and ready to execute.
    ToolCallReady(ToolCall),
    /// The stream ended normally.
    Done {
        finish_reason: FinishReason,
        usage: Option<TokenUsage>,
    },
    /// The loop is paused and waiting for user confirmation before executing tools.
    ConfirmationRequired { tool_calls: Vec<ToolCall> },
    /// An error occurred while reading the stream.
    Error(String),
}

pub type AgentEventSender = tokio::sync::mpsc::Sender<AgentEvent>;
pub type AgentEventReceiver = tokio::sync::mpsc::Receiver<AgentEvent>;

/// Create a bounded channel for agent events.
pub fn agent_event_channel(capacity: usize) -> (AgentEventSender, AgentEventReceiver) {
    tokio::sync::mpsc::channel(capacity)
}

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
