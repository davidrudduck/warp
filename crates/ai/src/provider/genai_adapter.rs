use super::*;
use async_trait::async_trait;
use futures::StreamExt;
use genai::adapter::AdapterKind;
use genai::chat::{
    ChatMessage as GenaiChatMessage, ChatOptions as GenaiChatOptions,
    ChatRequest as GenaiChatRequest, ChatStreamEvent, Tool as GenaiTool, ToolCall as GenaiToolCall,
    ToolResponse as GenaiToolResponse,
};
use genai::resolver::{AuthData, AuthResolver, Endpoint, ServiceTargetResolver};
use genai::ModelIden;

pub struct GenaiAdapter {
    client: genai::Client,
    provider: String,
    model: String,
    api_key: Option<String>,
    base_url: Option<String>,
    capabilities: ModelCapabilities,
}

impl GenaiAdapter {
    pub fn new(provider: &str, api_key: &str, model: &str) -> Self {
        let api_key = (!api_key.is_empty()).then_some(api_key.to_string());
        let client = build_client(provider, api_key.clone(), None);

        Self {
            client,
            provider: provider.to_string(),
            model: model.to_string(),
            api_key,
            base_url: None,
            capabilities: ModelCapabilities::default(),
        }
    }
}

#[async_trait]
impl LlmProvider for GenaiAdapter {
    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse, ProviderError> {
        let genai_req = convert_to_genai_request(req);

        let genai_resp = self
            .client
            .exec_chat(&self.model, genai_req, None)
            .await
            .map_err(|e| ProviderError::Remote {
                provider: self.provider.clone(),
                code: None,
                message: e.to_string(),
            })?;

        Ok(convert_from_genai_response(genai_resp))
    }

    async fn chat_stream(&self, req: ChatRequest) -> Result<ChatStream, ProviderError> {
        let genai_req = convert_to_genai_request(req);
        let stream_options = genai_stream_options();

        let stream_resp = self
            .client
            .exec_chat_stream(&self.model, genai_req, Some(&stream_options))
            .await
            .map_err(|e| ProviderError::Remote {
                provider: self.provider.clone(),
                code: None,
                message: e.to_string(),
            })?;

        let provider = self.provider.clone();
        let stream = stream_resp.stream.flat_map(move |result| {
            let events = match result {
                Ok(event) => convert_genai_stream_event(event),
                Err(e) => vec![Err(ProviderError::Remote {
                    provider: provider.clone(),
                    code: None,
                    message: e.to_string(),
                })],
            };
            futures::stream::iter(events)
        });

        Ok(Box::pin(stream))
    }

    fn capabilities(&self) -> &ModelCapabilities {
        &self.capabilities
    }

    fn provider_kind(&self) -> ProviderKind {
        match self.provider.as_str() {
            "openai" => ProviderKind::OpenAI,
            "anthropic" => ProviderKind::Anthropic,
            "ollama" => ProviderKind::Ollama,
            "gemini" => ProviderKind::Google,
            "custom" | "openai-compatible" => ProviderKind::OpenAICompatible {
                label: self.provider.clone(),
                base_url: self.base_url.clone().unwrap_or_default(),
            },
            "openrouter" => ProviderKind::OpenAICompatible {
                label: "openrouter".to_string(),
                base_url: self.base_url.clone().unwrap_or_default(),
            },
            _ => ProviderKind::OpenAI,
        }
    }

    fn with_base_url(mut self, url: &str) -> Self {
        let base_url = endpoint_base_url_for_provider(&self.provider, url);
        self.client = build_client(&self.provider, self.api_key.clone(), Some(base_url.clone()));
        self.base_url = Some(base_url);
        self
    }
}

fn genai_stream_options() -> GenaiChatOptions {
    GenaiChatOptions::default().with_capture_tool_calls(true)
}

fn build_client(
    provider: &str,
    api_key: Option<String>,
    base_url: Option<String>,
) -> genai::Client {
    let adapter_kind = adapter_kind_for_provider(provider);
    let mut builder = genai::Client::builder().with_adapter_kind(adapter_kind);

    if let Some(api_key) = api_key {
        let auth_resolver = AuthResolver::from_resolver_fn(
            move |_model_iden: ModelIden| -> Result<Option<AuthData>, genai::resolver::Error> {
                Ok(Some(AuthData::from_single(api_key.clone())))
            },
        );
        builder = builder.with_auth_resolver(auth_resolver);
    }

    if let Some(base_url) = base_url {
        let target_resolver = ServiceTargetResolver::from_resolver_fn(
            move |mut service_target: genai::ServiceTarget| -> Result<genai::ServiceTarget, genai::resolver::Error> {
                if service_target.model.adapter_kind == adapter_kind {
                    service_target.endpoint = Endpoint::from_owned(base_url.clone());
                }
                Ok(service_target)
            },
        );
        builder = builder.with_service_target_resolver(target_resolver);
    }

    builder.build()
}

fn adapter_kind_for_provider(provider: &str) -> AdapterKind {
    match provider {
        "anthropic" => AdapterKind::Anthropic,
        "gemini" => AdapterKind::Gemini,
        "ollama" => AdapterKind::Ollama,
        "openai" | "openrouter" | "custom" | "openai-compatible" => AdapterKind::OpenAI,
        _ => AdapterKind::OpenAI,
    }
}

fn endpoint_base_url_for_provider(provider: &str, url: &str) -> String {
    let mut base_url = match provider {
        "custom" | "openai-compatible" => {
            crate::url_validation::openai_compatible_base_url_with_v1(url)
        }
        "openrouter" | "openai" | "anthropic" | "gemini" | "ollama" => {
            url.trim().trim_end_matches('/').to_string()
        }
        _ => url.trim().trim_end_matches('/').to_string(),
    };
    base_url.push('/');
    base_url
}

fn convert_to_genai_request(req: ChatRequest) -> GenaiChatRequest {
    let mut genai_messages = Vec::new();

    for msg in req.messages {
        match msg {
            ChatMessage::System(text) => {
                genai_messages.push(GenaiChatMessage::system(text));
            }
            ChatMessage::User(blocks) => {
                append_user_content_blocks(&mut genai_messages, &blocks);
            }
            ChatMessage::Assistant { text, tool_calls } => {
                if let Some(text) = text {
                    if !text.is_empty() {
                        genai_messages.push(GenaiChatMessage::assistant(text));
                    }
                }
                if !tool_calls.is_empty() {
                    genai_messages.push(GenaiChatMessage::from(
                        tool_calls
                            .into_iter()
                            .map(genai_tool_call_from_tool_call)
                            .collect::<Vec<_>>(),
                    ));
                }
            }
        }
    }

    let mut chat_req = GenaiChatRequest::new(genai_messages);

    // Add tools if present
    if !req.tools.is_empty() {
        let genai_tools: Vec<GenaiTool> = req
            .tools
            .into_iter()
            .map(|tool| {
                GenaiTool::new(&tool.name)
                    .with_description(&tool.description)
                    .with_schema(tool.input_schema)
            })
            .collect();

        chat_req = chat_req.with_tools(genai_tools);
    }

    chat_req
}

fn append_user_content_blocks(genai_messages: &mut Vec<GenaiChatMessage>, blocks: &[ContentBlock]) {
    let mut text_parts = Vec::new();
    let mut tool_responses = Vec::new();

    for block in blocks {
        match block {
            ContentBlock::Text(text) => text_parts.push(text.clone()),
            ContentBlock::ToolResult {
                tool_use_id,
                content,
                is_error,
            } => {
                let content = match content {
                    ToolResultContent::Text(text) => text.clone(),
                    ToolResultContent::Blocks(blocks) => content_blocks_to_text(blocks),
                };
                let content = if *is_error {
                    format!("Error: {content}")
                } else {
                    content
                };
                tool_responses.push(GenaiToolResponse::new(tool_use_id.clone(), content));
            }
            ContentBlock::ToolUse { id, name, input } => {
                text_parts.push(format!("Tool call {id} {name}: {input}"));
            }
            ContentBlock::Image { .. } => {}
        }
    }

    if !text_parts.is_empty() {
        genai_messages.push(GenaiChatMessage::user(text_parts.join("\n")));
    }
    if !tool_responses.is_empty() {
        genai_messages.push(GenaiChatMessage::from(tool_responses));
    }
}

fn genai_tool_call_from_tool_call(tool_call: ToolCall) -> GenaiToolCall {
    GenaiToolCall {
        call_id: tool_call.id,
        fn_name: tool_call.name,
        fn_arguments: tool_call.input,
        thought_signatures: None,
    }
}

fn tool_call_from_genai(tool_call: &GenaiToolCall) -> ToolCall {
    ToolCall {
        id: tool_call.call_id.clone(),
        name: tool_call.fn_name.clone(),
        input: tool_call.fn_arguments.clone(),
    }
}

fn content_blocks_to_text(blocks: &[ContentBlock]) -> String {
    blocks
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text(text) => Some(text.clone()),
            ContentBlock::ToolResult {
                tool_use_id,
                content,
                is_error,
            } => {
                let status = if *is_error { "error" } else { "success" };
                Some(format!(
                    "Tool result for {tool_use_id} ({status}): {}",
                    tool_result_content_to_text(content)
                ))
            }
            ContentBlock::ToolUse { id, name, input } => {
                Some(format!("Tool call {id} {name}: {input}"))
            }
            ContentBlock::Image { .. } => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn tool_result_content_to_text(content: &ToolResultContent) -> String {
    match content {
        ToolResultContent::Text(text) => text.clone(),
        ToolResultContent::Blocks(blocks) => content_blocks_to_text(blocks),
    }
}

fn convert_from_genai_response(resp: genai::chat::ChatResponse) -> ChatResponse {
    let text = resp.first_text().map(|s| s.to_string());

    // Extract tool calls from genai response
    let genai_tool_calls = resp.tool_calls();
    let tool_calls: Vec<ToolCall> = genai_tool_calls
        .iter()
        .map(|tc| {
            // genai's fn_arguments is already a serde_json::Value
            ToolCall {
                id: tc.call_id.clone(),
                name: tc.fn_name.clone(),
                input: tc.fn_arguments.clone(),
            }
        })
        .collect();

    // Set finish_reason to ToolUse if tool calls present
    let finish_reason = if !tool_calls.is_empty() {
        FinishReason::ToolUse
    } else {
        FinishReason::Stop
    };

    ChatResponse {
        text,
        tool_calls,
        finish_reason,
        usage: None,
    }
}

fn convert_genai_stream_event(event: ChatStreamEvent) -> Vec<Result<StreamEvent, ProviderError>> {
    match event {
        ChatStreamEvent::Start => vec![Ok(StreamEvent::Start)],
        ChatStreamEvent::Chunk(chunk) => vec![Ok(StreamEvent::TextChunk(chunk.content))],
        ChatStreamEvent::ToolCallChunk(_) => Vec::new(),
        ChatStreamEvent::End(end) => {
            // Determine finish reason first (before moving end)
            let has_tool_calls = end
                .captured_tool_calls()
                .map(|calls| !calls.is_empty())
                .unwrap_or(false);

            let finish_reason = if has_tool_calls {
                FinishReason::ToolUse
            } else {
                FinishReason::Stop
            };
            let tool_calls = end
                .captured_tool_calls()
                .unwrap_or_default()
                .into_iter()
                .map(tool_call_from_genai)
                .collect::<Vec<_>>();

            // Convert usage if available (i32 -> u32)
            let usage = end.captured_usage.map(|u| TokenUsage {
                input_tokens: u.prompt_tokens.unwrap_or(0) as u32,
                output_tokens: u.completion_tokens.unwrap_or(0) as u32,
                cache_read_tokens: u
                    .prompt_tokens_details
                    .and_then(|d| d.cached_tokens)
                    .map(|t| t as u32),
            });

            let mut events = tool_calls
                .into_iter()
                .map(|tool_call| Ok(StreamEvent::ToolCallReady(tool_call)))
                .collect::<Vec<_>>();
            events.push(Ok(StreamEvent::End {
                finish_reason,
                usage,
            }));
            events
        }
        ChatStreamEvent::ReasoningChunk(chunk) => {
            vec![Ok(StreamEvent::ReasoningChunk(chunk.content))]
        }
        ChatStreamEvent::ThoughtSignatureChunk(_) => {
            // Ignore thought signatures for now
            Vec::new()
        }
    }
}

#[cfg(test)]
#[path = "genai_adapter_tests.rs"]
mod tests;
