use super::{
    ChatMessage, ChatStream, ContentBlock, FinishReason, ProviderError, StreamEvent, TokenUsage,
    ToolCall, ToolResultContent,
};
use crate::logging::{redact_rig_diagnostic_event, RigDiagnosticEvent};
use futures::StreamExt;
use rig_core::client::{CompletionClient, Nothing};
use rig_core::completion::{
    CompletionError, CompletionModel, CompletionRequest, GetTokenUsage, ToolDefinition,
};
use rig_core::message::{
    AssistantContent as RigAssistantContent, Message as RigMessage, Text as RigText,
    ToolCall as RigToolCall, ToolResultContent as RigToolResultContent,
    UserContent as RigUserContent,
};
use rig_core::streaming::{StreamedAssistantContent, ToolCallDeltaContent};
use rig_core::OneOrMany;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RigProviderKind {
    OpenAI,
    Anthropic,
    GoogleGemini,
    Ollama,
    OpenRouter,
    CustomOpenAICompatible,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RigBackendConfig {
    pub provider_kind: RigProviderKind,
    pub model_id: String,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
}

impl RigBackendConfig {
    pub fn new(
        provider_kind: RigProviderKind,
        model_id: impl Into<String>,
        api_key: Option<String>,
        base_url: Option<String>,
    ) -> Self {
        Self {
            provider_kind,
            model_id: model_id.into(),
            api_key,
            base_url,
        }
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        if self.model_id.trim().is_empty() {
            anyhow::bail!("Rig Direct API backend requires a model");
        }

        match self.provider_kind {
            RigProviderKind::Ollama => Ok(()),
            RigProviderKind::OpenAI
            | RigProviderKind::Anthropic
            | RigProviderKind::GoogleGemini
            | RigProviderKind::OpenRouter => {
                if self
                    .api_key
                    .as_deref()
                    .is_none_or(|key| key.trim().is_empty())
                {
                    anyhow::bail!("Rig Direct API backend requires an API key");
                }
                Ok(())
            }
            RigProviderKind::CustomOpenAICompatible => {
                if self
                    .base_url
                    .as_deref()
                    .is_none_or(|url| url.trim().is_empty())
                {
                    anyhow::bail!("Rig Direct API backend requires a base URL");
                }
                Ok(())
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RigBackendEvent {
    Start,
    TextChunk(String),
    ReasoningChunk(String),
    ToolCallDelta {
        index: usize,
        id: String,
        name: String,
        args_fragment: String,
    },
    ToolCallReady(ToolCall),
    End {
        finish_reason: FinishReason,
        usage: Option<TokenUsage>,
    },
}

pub type RigEventStream =
    std::pin::Pin<Box<dyn futures::Stream<Item = Result<RigBackendEvent, ProviderError>> + Send>>;

pub struct RigDirectBackend {
    config: RigBackendConfig,
}

impl RigDirectBackend {
    pub fn new(config: RigBackendConfig) -> anyhow::Result<Self> {
        config.validate()?;
        Ok(Self { config })
    }

    pub async fn stream_turn(&self, request: super::ChatRequest) -> anyhow::Result<ChatStream> {
        let rig_stream = stream_turn_with_rig(self.config.clone(), request).await?;
        Ok(rig_event_stream_to_chat_stream(rig_stream))
    }
}

pub async fn stream_turn_with_rig(
    config: RigBackendConfig,
    request: super::ChatRequest,
) -> anyhow::Result<RigEventStream> {
    let mut diagnostics = rig_diagnostic_event_from_config(&config);
    if let Err(err) = config.validate() {
        diagnostics.error_category = Some("validation".to_string());
        log::debug!("{}", redact_rig_diagnostic_event(&diagnostics));
        return Err(err);
    }
    log::debug!("{}", redact_rig_diagnostic_event(&diagnostics));
    match config.provider_kind {
        RigProviderKind::OpenAI => {
            let api_key = required_api_key(&config).map_err(|err| {
                log_rig_diagnostic_provider_error(&mut diagnostics, &err);
                err
            })?;
            let client = rig_core::providers::openai::Client::new(api_key)
                .map_err(|err| rig_client_error_with_diagnostics(err, &mut diagnostics))?;
            stream_with_model(
                client.completion_model(config.model_id.clone()),
                request,
                diagnostics,
            )
            .await
        }
        RigProviderKind::Anthropic => {
            let api_key = required_api_key(&config).map_err(|err| {
                log_rig_diagnostic_provider_error(&mut diagnostics, &err);
                err
            })?;
            let client = rig_core::providers::anthropic::Client::new(api_key)
                .map_err(|err| rig_client_error_with_diagnostics(err, &mut diagnostics))?;
            stream_with_model(
                client.completion_model(config.model_id.clone()),
                request,
                diagnostics,
            )
            .await
        }
        RigProviderKind::GoogleGemini => {
            let api_key = required_api_key(&config).map_err(|err| {
                log_rig_diagnostic_provider_error(&mut diagnostics, &err);
                err
            })?;
            let client = rig_core::providers::gemini::Client::new(api_key)
                .map_err(|err| rig_client_error_with_diagnostics(err, &mut diagnostics))?;
            stream_with_model(
                client.completion_model(config.model_id.clone()),
                request,
                diagnostics,
            )
            .await
        }
        RigProviderKind::Ollama => {
            let client = if let Some(base_url) = config.base_url.as_deref() {
                rig_core::providers::ollama::Client::builder()
                    .api_key(config.api_key.clone().unwrap_or_default())
                    .base_url(base_url)
                    .build()
                    .map_err(|err| rig_client_error_with_diagnostics(err, &mut diagnostics))?
            } else {
                rig_core::providers::ollama::Client::new(Nothing)
                    .map_err(|err| rig_client_error_with_diagnostics(err, &mut diagnostics))?
            };
            stream_with_model(
                client.completion_model(config.model_id.clone()),
                request,
                diagnostics,
            )
            .await
        }
        RigProviderKind::OpenRouter => {
            let api_key = required_api_key(&config).map_err(|err| {
                log_rig_diagnostic_provider_error(&mut diagnostics, &err);
                err
            })?;
            let mut builder = rig_core::providers::openrouter::Client::builder().api_key(api_key);
            if let Some(base_url) = config.base_url.as_deref() {
                builder = builder.base_url(base_url);
            }
            let client = builder
                .build()
                .map_err(|err| rig_client_error_with_diagnostics(err, &mut diagnostics))?;
            stream_with_model(
                client.completion_model(config.model_id.clone()),
                request,
                diagnostics,
            )
            .await
        }
        RigProviderKind::CustomOpenAICompatible => {
            let base_url = match config.base_url.clone() {
                Some(base_url) => base_url,
                None => {
                    let err = ProviderError::UnsupportedModel(
                        "Rig Direct API backend requires a base URL".to_string(),
                    );
                    log_rig_diagnostic_provider_error(&mut diagnostics, &err);
                    return Err(err.into());
                }
            };
            let client = rig_core::providers::openai::CompletionsClient::builder()
                .api_key(config.api_key.clone().unwrap_or_default())
                .base_url(base_url)
                .build()
                .map_err(|err| rig_client_error_with_diagnostics(err, &mut diagnostics))?;
            stream_with_model(
                client.completion_model(config.model_id.clone()),
                request,
                diagnostics,
            )
            .await
        }
    }
}

fn rig_client_error_with_diagnostics(
    err: rig_core::http_client::Error,
    diagnostics: &mut RigDiagnosticEvent,
) -> ProviderError {
    let err = rig_client_error(err);
    log_rig_diagnostic_provider_error(diagnostics, &err);
    err
}

fn log_rig_diagnostic_provider_error(diagnostics: &mut RigDiagnosticEvent, error: &ProviderError) {
    log::debug!("{}", categorized_rig_diagnostic(diagnostics, error));
}

fn log_rig_diagnostic_stream_error(diagnostics: &mut RigDiagnosticEvent, error: &ProviderError) {
    diagnostics.error_category = Some(provider_error_category(error).to_string());
    diagnostics.http_status = provider_error_http_status(error);
    log::debug!("{}", redact_rig_diagnostic_event(diagnostics));
}

fn categorized_rig_diagnostic(
    diagnostics: &mut RigDiagnosticEvent,
    error: &ProviderError,
) -> String {
    diagnostics.error_category = Some(provider_error_category(error).to_string());
    diagnostics.http_status = provider_error_http_status(error);
    redact_rig_diagnostic_event(diagnostics)
}

async fn stream_with_model<M>(
    model: M,
    request: super::ChatRequest,
    mut diagnostics: RigDiagnosticEvent,
) -> anyhow::Result<RigEventStream>
where
    M: CompletionModel + 'static,
    M::StreamingResponse: 'static,
{
    let completion_request = match rig_completion_request_from_chat_request(request) {
        Ok(request) => request,
        Err(err) => {
            diagnostics.error_category = Some(provider_error_category(&err).to_string());
            diagnostics.http_status = provider_error_http_status(&err);
            log::debug!("{}", redact_rig_diagnostic_event(&diagnostics));
            return Err(err.into());
        }
    };
    let stream = match model.stream(completion_request).await {
        Ok(stream) => stream,
        Err(err) => {
            let err = rig_completion_error(err);
            log_rig_diagnostic_stream_error(&mut diagnostics, &err);
            return Err(err.into());
        }
    };
    let start = futures::stream::once(async { Ok(RigBackendEvent::Start) });
    let mut mapper = RigStreamMapper::default();
    let mut tool_call_ids = HashSet::new();
    let mapped = stream.filter_map(move |item| {
        let event = match item {
            Ok(item) => mapper.map_stream_item(item).transpose(),
            Err(err) => Some(Err(rig_completion_error(err))),
        };
        if let Some(result) = event.as_ref() {
            match result {
                Ok(event) => {
                    diagnostics.event_count += 1;
                    match event {
                        RigBackendEvent::ToolCallDelta { id, .. } => {
                            if !id.is_empty() {
                                tool_call_ids.insert(id.clone());
                                diagnostics.tool_call_count = tool_call_ids.len();
                            }
                        }
                        RigBackendEvent::ToolCallReady(tool_call) => {
                            tool_call_ids.insert(tool_call.id.clone());
                            diagnostics.tool_call_count = tool_call_ids.len();
                        }
                        RigBackendEvent::End { finish_reason, .. } => {
                            diagnostics.finish_reason = Some(format!("{finish_reason:?}"));
                            log::debug!("{}", redact_rig_diagnostic_event(&diagnostics));
                        }
                        RigBackendEvent::Start
                        | RigBackendEvent::TextChunk(_)
                        | RigBackendEvent::ReasoningChunk(_) => {}
                    }
                }
                Err(err) => {
                    log_rig_diagnostic_stream_error(&mut diagnostics, err);
                }
            }
        }
        async move { event }
    });
    Ok(Box::pin(start.chain(mapped)))
}

fn rig_diagnostic_event_from_config(config: &RigBackendConfig) -> RigDiagnosticEvent {
    RigDiagnosticEvent {
        provider: rig_provider_label(config.provider_kind).to_string(),
        model_id: config.model_id.clone(),
        model_id_is_public: is_public_rig_model_id(config.provider_kind, &config.model_id),
        ..Default::default()
    }
}

fn is_public_rig_model_id(provider: RigProviderKind, model_id: &str) -> bool {
    match provider {
        RigProviderKind::OpenAI => matches!(
            model_id,
            "gpt-4o"
                | "gpt-4o-mini"
                | "gpt-4.1"
                | "gpt-4.1-mini"
                | "gpt-4.1-nano"
                | "gpt-5"
                | "gpt-5-mini"
                | "gpt-5-nano"
        ),
        RigProviderKind::Anthropic => matches!(
            model_id,
            "claude-3-5-sonnet-20241022"
                | "claude-3-5-haiku-20241022"
                | "claude-3-7-sonnet-20250219"
                | "claude-sonnet-4-20250514"
                | "claude-opus-4-20250514"
        ),
        RigProviderKind::GoogleGemini => matches!(
            model_id,
            "gemini-2.0-flash" | "gemini-2.5-flash" | "gemini-2.5-pro"
        ),
        RigProviderKind::OpenRouter => matches!(
            model_id,
            "moonshotai/kimi-k2.6"
                | "openai/gpt-4o-mini"
                | "anthropic/claude-3.5-sonnet"
                | "google/gemini-2.5-flash"
        ),
        RigProviderKind::Ollama | RigProviderKind::CustomOpenAICompatible => false,
    }
}

fn rig_provider_label(provider: RigProviderKind) -> &'static str {
    match provider {
        RigProviderKind::OpenAI => "OpenAI",
        RigProviderKind::Anthropic => "Anthropic",
        RigProviderKind::GoogleGemini => "GoogleGemini",
        RigProviderKind::Ollama => "Ollama",
        RigProviderKind::OpenRouter => "OpenRouter",
        RigProviderKind::CustomOpenAICompatible => "CustomOpenAICompatible",
    }
}

fn provider_error_category(error: &ProviderError) -> &'static str {
    match error {
        ProviderError::Auth(_) => "auth",
        ProviderError::Http { .. } => "http",
        ProviderError::Remote { .. } => "remote",
        ProviderError::RateLimited { .. } => "rate_limited",
        ProviderError::ServiceUnavailable => "service_unavailable",
        ProviderError::ContextLengthExceeded => "context_length",
        ProviderError::Transport(_) => "transport",
        ProviderError::StreamParse(_) => "stream_parse",
        ProviderError::Cancelled => "cancelled",
        ProviderError::UnsupportedModel(_) => "unsupported_model",
    }
}

fn provider_error_http_status(error: &ProviderError) -> Option<u16> {
    match error {
        ProviderError::Http { status, .. } => Some(*status),
        ProviderError::Remote { message, .. } => {
            crate::logging::http_status_from_diagnostic_message(message)
        }
        ProviderError::Auth(_)
        | ProviderError::RateLimited { .. }
        | ProviderError::ServiceUnavailable
        | ProviderError::ContextLengthExceeded
        | ProviderError::Transport(_)
        | ProviderError::StreamParse(_)
        | ProviderError::Cancelled
        | ProviderError::UnsupportedModel(_) => None,
    }
}

fn rig_completion_request_from_chat_request(
    request: super::ChatRequest,
) -> Result<CompletionRequest, ProviderError> {
    let messages = request
        .messages
        .into_iter()
        .map(rig_message_from_chat_message)
        .collect::<Result<Vec<_>, _>>()?;
    let chat_history = OneOrMany::many(messages).map_err(|_| {
        ProviderError::StreamParse("Rig request requires at least one message".to_string())
    })?;

    Ok(CompletionRequest {
        model: None,
        preamble: request.options.system,
        chat_history,
        documents: Vec::new(),
        tools: request
            .tools
            .into_iter()
            .map(|tool| ToolDefinition {
                name: tool.name,
                description: tool.description,
                parameters: tool.input_schema,
            })
            .collect(),
        temperature: request.options.temperature.map(f64::from),
        max_tokens: request.options.max_tokens.map(u64::from),
        tool_choice: None,
        additional_params: None,
        output_schema: None,
    })
}

fn rig_message_from_chat_message(message: ChatMessage) -> Result<RigMessage, ProviderError> {
    match message {
        ChatMessage::System(text) => Ok(RigMessage::system(text)),
        ChatMessage::User(blocks) => {
            let content = blocks
                .into_iter()
                .map(rig_user_content_from_content_block)
                .collect::<Result<Vec<_>, _>>()?;
            let content = OneOrMany::many(content).map_err(|_| {
                ProviderError::StreamParse("Rig user message requires content".to_string())
            })?;
            Ok(RigMessage::User { content })
        }
        ChatMessage::Assistant { text, tool_calls } => {
            let mut content = Vec::new();
            if let Some(text) = text.filter(|text| !text.is_empty()) {
                content.push(RigAssistantContent::text(text));
            }
            content.extend(tool_calls.into_iter().map(rig_assistant_tool_call));
            let content = OneOrMany::many(content).map_err(|_| {
                ProviderError::StreamParse(
                    "Rig assistant message requires text or tool calls".to_string(),
                )
            })?;
            Ok(RigMessage::Assistant { id: None, content })
        }
    }
}

fn rig_user_content_from_content_block(
    block: ContentBlock,
) -> Result<RigUserContent, ProviderError> {
    match block {
        ContentBlock::Text(text) => Ok(RigUserContent::text(text)),
        ContentBlock::ToolResult {
            tool_use_id,
            content,
            is_error,
        } => {
            let mut text = tool_result_content_text(content);
            if is_error {
                text = format!("Error: {text}");
            }
            Ok(RigUserContent::tool_result(
                tool_use_id,
                OneOrMany::one(RigToolResultContent::Text(RigText { text })),
            ))
        }
        ContentBlock::ToolUse { id, name, input } => Ok(RigUserContent::Text(RigText {
            text: serde_json::to_string(&serde_json::json!({
                "tool_call": {
                    "id": id,
                    "name": name,
                    "input": input,
                }
            }))
            .map_err(|err| ProviderError::StreamParse(err.to_string()))?,
        })),
        ContentBlock::Image { .. } => Err(ProviderError::UnsupportedModel(
            "Rig Direct API backend does not yet support image content".to_string(),
        )),
    }
}

fn rig_assistant_tool_call(tool_call: ToolCall) -> RigAssistantContent {
    RigAssistantContent::tool_call(tool_call.id, tool_call.name, tool_call.input)
}

fn tool_result_content_text(content: ToolResultContent) -> String {
    match content {
        ToolResultContent::Text(text) => text,
        ToolResultContent::Blocks(blocks) => blocks
            .into_iter()
            .map(|block| match block {
                ContentBlock::Text(text) => text,
                ContentBlock::ToolResult {
                    tool_use_id,
                    content,
                    is_error,
                } => {
                    let text = tool_result_content_text(content);
                    if is_error {
                        format!("{tool_use_id}: error: {text}")
                    } else {
                        format!("{tool_use_id}: {text}")
                    }
                }
                ContentBlock::ToolUse { id, name, input } => {
                    format!("{id}: {name}({input})")
                }
                ContentBlock::Image { .. } => "<image omitted>".to_string(),
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

#[derive(Default)]
struct RigStreamMapper {
    tool_call_indices: HashMap<String, usize>,
    chunked_tool_calls: std::collections::HashSet<String>,
    chunked_tool_calls_with_args: std::collections::HashSet<String>,
    saw_tool_call: bool,
}

impl RigStreamMapper {
    fn map_stream_item<R>(
        &mut self,
        item: StreamedAssistantContent<R>,
    ) -> Result<Option<RigBackendEvent>, ProviderError>
    where
        R: Clone + Unpin + GetTokenUsage,
    {
        match item {
            StreamedAssistantContent::Text(text) => Ok(Some(RigBackendEvent::TextChunk(text.text))),
            StreamedAssistantContent::ToolCall {
                tool_call,
                internal_call_id,
            } => {
                self.saw_tool_call = true;
                let index = self.tool_index_for_internal_call_id(&internal_call_id);
                if self.chunked_tool_calls.contains(&internal_call_id) {
                    if !self
                        .chunked_tool_calls_with_args
                        .contains(&internal_call_id)
                    {
                        return Ok(Some(RigBackendEvent::ToolCallDelta {
                            index,
                            id: String::new(),
                            name: String::new(),
                            args_fragment: serde_json::to_string(&tool_call.function.arguments)
                                .map_err(|err| ProviderError::StreamParse(err.to_string()))?,
                        }));
                    }
                    return Ok(None);
                }
                Ok(Some(RigBackendEvent::ToolCallReady(tool_call_from_rig(
                    tool_call,
                ))))
            }
            StreamedAssistantContent::ToolCallDelta {
                id,
                internal_call_id,
                content,
            } => {
                self.saw_tool_call = true;
                let index = self.tool_index_for_internal_call_id(&internal_call_id);
                self.chunked_tool_calls.insert(internal_call_id.clone());
                let (name, args_fragment) = match content {
                    ToolCallDeltaContent::Name(name) => (name, String::new()),
                    ToolCallDeltaContent::Delta(args_fragment) => {
                        self.chunked_tool_calls_with_args
                            .insert(internal_call_id.clone());
                        (String::new(), args_fragment)
                    }
                };
                Ok(Some(RigBackendEvent::ToolCallDelta {
                    index,
                    id,
                    name,
                    args_fragment,
                }))
            }
            StreamedAssistantContent::Reasoning(reasoning) => Ok(Some(
                RigBackendEvent::ReasoningChunk(reasoning.display_text()),
            )),
            StreamedAssistantContent::ReasoningDelta { id: _, reasoning } => {
                Ok(Some(RigBackendEvent::ReasoningChunk(reasoning)))
            }
            StreamedAssistantContent::Final(response) => Ok(Some(RigBackendEvent::End {
                finish_reason: if self.saw_tool_call {
                    FinishReason::ToolUse
                } else {
                    FinishReason::Stop
                },
                usage: response.token_usage().map(token_usage_from_rig),
            })),
        }
    }

    fn tool_index_for_internal_call_id(&mut self, internal_call_id: &str) -> usize {
        let next_index = self.tool_call_indices.len();
        *self
            .tool_call_indices
            .entry(internal_call_id.to_string())
            .or_insert(next_index)
    }
}

fn tool_call_from_rig(tool_call: RigToolCall) -> ToolCall {
    ToolCall {
        id: tool_call.id,
        name: tool_call.function.name,
        input: tool_call.function.arguments,
    }
}

fn token_usage_from_rig(usage: rig_core::completion::Usage) -> TokenUsage {
    TokenUsage {
        input_tokens: capped_u32(usage.input_tokens),
        output_tokens: capped_u32(usage.output_tokens),
        cache_read_tokens: Some(capped_u32(usage.cached_input_tokens)),
    }
}

fn capped_u32(value: u64) -> u32 {
    value.min(u64::from(u32::MAX)) as u32
}

fn rig_event_stream_to_chat_stream(stream: RigEventStream) -> ChatStream {
    Box::pin(stream.filter_map(|event| async {
        match event {
            Ok(event) => stream_event_from_rig_backend_event(event).map(Ok),
            Err(err) => Some(Err(err)),
        }
    }))
}

fn stream_event_from_rig_backend_event(event: RigBackendEvent) -> Option<StreamEvent> {
    match event {
        RigBackendEvent::Start => Some(StreamEvent::Start),
        RigBackendEvent::TextChunk(text) => Some(StreamEvent::TextChunk(text)),
        RigBackendEvent::ReasoningChunk(reasoning) => Some(StreamEvent::ReasoningChunk(reasoning)),
        RigBackendEvent::ToolCallDelta {
            index,
            id,
            name,
            args_fragment,
        } => Some(StreamEvent::ToolCallChunk {
            index,
            id,
            name,
            args_fragment,
        }),
        RigBackendEvent::ToolCallReady(call) => Some(StreamEvent::ToolCallReady(call)),
        RigBackendEvent::End {
            finish_reason,
            usage,
        } => Some(StreamEvent::End {
            finish_reason,
            usage,
        }),
    }
}

fn required_api_key(config: &RigBackendConfig) -> Result<String, ProviderError> {
    config
        .api_key
        .clone()
        .filter(|key| !key.trim().is_empty())
        .ok_or_else(|| {
            ProviderError::Auth("Rig Direct API backend requires an API key".to_string())
        })
}

fn rig_completion_error(err: CompletionError) -> ProviderError {
    match err {
        CompletionError::HttpError(err) => rig_http_error(err),
        CompletionError::JsonError(err) => ProviderError::StreamParse(err.to_string()),
        CompletionError::UrlError(err) => ProviderError::StreamParse(err.to_string()),
        CompletionError::RequestError(err) => ProviderError::StreamParse(err.to_string()),
        CompletionError::ResponseError(message) => ProviderError::StreamParse(message),
        CompletionError::ProviderError(message) => rig_provider_error(message),
    }
}

fn rig_client_error(err: rig_core::http_client::Error) -> ProviderError {
    rig_http_error(err)
}

fn rig_http_error(err: rig_core::http_client::Error) -> ProviderError {
    match &err {
        rig_core::http_client::Error::InvalidStatusCode(status)
        | rig_core::http_client::Error::InvalidStatusCodeWithMessage(status, _)
            if matches!(status.as_u16(), 401 | 403) =>
        {
            ProviderError::Auth("Rig Direct API provider rejected the API key".to_string())
        }
        rig_core::http_client::Error::Protocol(_)
        | rig_core::http_client::Error::InvalidStatusCode(_)
        | rig_core::http_client::Error::InvalidStatusCodeWithMessage(_, _)
        | rig_core::http_client::Error::InvalidHeaderValue(_)
        | rig_core::http_client::Error::NoHeaders
        | rig_core::http_client::Error::StreamEnded
        | rig_core::http_client::Error::InvalidContentType(_)
        | rig_core::http_client::Error::Instance(_) => ProviderError::Remote {
            provider: "rig".to_string(),
            code: None,
            message: err.to_string(),
        },
    }
}

fn rig_provider_error(message: String) -> ProviderError {
    if matches!(
        crate::logging::http_status_from_diagnostic_message(&message),
        Some(401 | 403)
    ) {
        ProviderError::Auth("Rig Direct API provider rejected the API key".to_string())
    } else {
        ProviderError::Remote {
            provider: "rig".to_string(),
            code: None,
            message,
        }
    }
}

#[cfg(test)]
#[path = "rig_backend_tests.rs"]
mod tests;
