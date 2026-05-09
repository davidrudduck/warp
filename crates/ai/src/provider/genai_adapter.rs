use super::*;
use async_trait::async_trait;
use futures::StreamExt;
use genai::chat::{
    ChatMessage as GenaiChatMessage, ChatRequest as GenaiChatRequest, ChatStreamEvent,
    Tool as GenaiTool,
};
use genai::resolver::{AuthData, AuthResolver};
use genai::ModelIden;

pub struct GenaiAdapter {
    client: genai::Client,
    provider: String,
    model: String,
    capabilities: ModelCapabilities,
}

impl GenaiAdapter {
    pub fn new(provider: &str, api_key: &str, model: &str) -> Self {
        let client = if !api_key.is_empty() {
            let api_key = api_key.to_string();
            let auth_resolver = AuthResolver::from_resolver_fn(
                move |_model_iden: ModelIden| -> Result<Option<AuthData>, genai::resolver::Error> {
                    Ok(Some(AuthData::from_single(api_key.clone())))
                },
            );

            genai::Client::builder()
                .with_auth_resolver(auth_resolver)
                .build()
        } else {
            genai::Client::default()
        };

        Self {
            client,
            provider: provider.to_string(),
            model: model.to_string(),
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

        let stream_resp = self
            .client
            .exec_chat_stream(&self.model, genai_req, None)
            .await
            .map_err(|e| ProviderError::Remote {
                provider: self.provider.clone(),
                code: None,
                message: e.to_string(),
            })?;

        let provider = self.provider.clone();
        let stream = stream_resp.stream.map(move |result| {
            result
                .map_err(|e| ProviderError::Remote {
                    provider: provider.clone(),
                    code: None,
                    message: e.to_string(),
                })
                .and_then(convert_genai_stream_event)
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
            _ => ProviderKind::OpenAI,
        }
    }

    fn with_base_url(self, _url: &str) -> Self {
        self
    }
}

fn convert_to_genai_request(req: ChatRequest) -> GenaiChatRequest {
    let mut genai_messages = Vec::new();

    for msg in req.messages {
        match msg {
            ChatMessage::System(text) => {
                genai_messages.push(GenaiChatMessage::system(text));
            }
            ChatMessage::User(blocks) => {
                let text = blocks
                    .iter()
                    .filter_map(|b| match b {
                        ContentBlock::Text(t) => Some(t.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                genai_messages.push(GenaiChatMessage::user(text));
            }
            ChatMessage::Assistant { text, .. } => {
                if let Some(text) = text {
                    genai_messages.push(GenaiChatMessage::assistant(text));
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

fn convert_genai_stream_event(event: ChatStreamEvent) -> Result<StreamEvent, ProviderError> {
    match event {
        ChatStreamEvent::Start => Ok(StreamEvent::Start),
        ChatStreamEvent::Chunk(chunk) => Ok(StreamEvent::TextChunk(chunk.content)),
        ChatStreamEvent::ToolCallChunk(chunk) => {
            // genai's fn_arguments is a Value, convert to string for the fragment
            Ok(StreamEvent::ToolCallChunk {
                index: 0,
                id: chunk.tool_call.call_id,
                name: chunk.tool_call.fn_name,
                args_fragment: chunk.tool_call.fn_arguments.to_string(),
            })
        }
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

            // Convert usage if available (i32 -> u32)
            let usage = end.captured_usage.map(|u| TokenUsage {
                input_tokens: u.prompt_tokens.unwrap_or(0) as u32,
                output_tokens: u.completion_tokens.unwrap_or(0) as u32,
                cache_read_tokens: u
                    .prompt_tokens_details
                    .and_then(|d| d.cached_tokens)
                    .map(|t| t as u32),
            });

            Ok(StreamEvent::End {
                finish_reason,
                usage,
            })
        }
        ChatStreamEvent::ReasoningChunk(_) => {
            // Ignore reasoning chunks for now (DeepSeek, Claude thinking)
            Ok(StreamEvent::TextChunk(String::new()))
        }
        ChatStreamEvent::ThoughtSignatureChunk(_) => {
            // Ignore thought signatures for now
            Ok(StreamEvent::TextChunk(String::new()))
        }
    }
}

#[cfg(test)]
#[path = "genai_adapter_tests.rs"]
mod tests;
