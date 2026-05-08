use super::*;
use async_trait::async_trait;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;

const DEFAULT_BASE_URL: &str = "https://api.openai.com";

fn create_default_client() -> reqwest::Client {
    reqwest::Client::builder()
        .build()
        .expect("Failed to create HTTP client")
}

pub struct OpenAIProvider {
    client: reqwest::Client,
    api_key: String,
    model_id: String,
    base_url: String,
    capabilities: ModelCapabilities,
}

impl OpenAIProvider {
    pub fn new(api_key: String, model_id: String) -> Self {
        Self::with_client(api_key, model_id, create_default_client())
    }

    pub fn with_client(api_key: String, model_id: String, client: reqwest::Client) -> Self {
        Self {
            client,
            api_key,
            model_id,
            base_url: DEFAULT_BASE_URL.to_string(),
            capabilities: ModelCapabilities::default(),
        }
    }

    fn build_request(&self, req: &ChatRequest) -> OpenAIRequest {
        let messages = req
            .messages
            .iter()
            .flat_map(convert_message)
            .collect();

        let tools = if req.tools.is_empty() {
            None
        } else {
            Some(
                req.tools
                    .iter()
                    .map(|t| OpenAITool {
                        r#type: "function".to_string(),
                        function: OpenAIFunction {
                            name: t.name.clone(),
                            description: Some(t.description.clone()),
                            parameters: t.input_schema.clone(),
                        },
                    })
                    .collect(),
            )
        };

        OpenAIRequest {
            model: self.model_id.clone(),
            messages,
            tools,
            temperature: req.options.temperature,
            max_tokens: req.options.max_tokens,
            stream: None,
        }
    }
}

#[async_trait]
impl LlmProvider for OpenAIProvider {
    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse, ProviderError> {
        let mut openai_req = self.build_request(&req);
        openai_req.stream = Some(false);

        let url = format!("{}/v1/chat/completions", self.base_url);

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&openai_req)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(map_http_error(status.as_u16(), &body));
        }

        let openai_resp: OpenAIResponse = response.json().await?;
        Ok(convert_response(openai_resp))
    }

    async fn chat_stream(&self, req: ChatRequest) -> Result<ChatStream, ProviderError> {
        let mut openai_req = self.build_request(&req);
        openai_req.stream = Some(true);

        let url = format!("{}/v1/chat/completions", self.base_url);

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&openai_req)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(map_http_error(status.as_u16(), &body));
        }

        let byte_stream = response.bytes_stream();
        let stream = byte_stream
            .map(|chunk_result| {
                chunk_result.map_err(ProviderError::Transport)
            })
            .flat_map(|chunk_result| {
                match chunk_result {
                    Ok(bytes) => {
                        let events = parse_sse_stream(&bytes[..]);
                        futures::stream::iter(events).boxed()
                    }
                    Err(e) => futures::stream::once(async move { Err(e) }).boxed(),
                }
            });

        Ok(Box::pin(stream))
    }

    fn capabilities(&self) -> &ModelCapabilities {
        &self.capabilities
    }

    fn provider_kind(&self) -> ProviderKind {
        ProviderKind::OpenAI
    }

    fn with_base_url(mut self, url: &str) -> Self {
        self.base_url = url.to_string();
        self
    }
}

// OpenAI API types
#[derive(Debug, Serialize)]
struct OpenAIRequest {
    model: String,
    messages: Vec<OpenAIMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OpenAITool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OpenAIMessage {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<OpenAIToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OpenAIToolCall {
    pub id: String,
    pub r#type: String,
    pub function: OpenAIFunctionCall,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OpenAIFunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Serialize)]
struct OpenAITool {
    r#type: String,
    function: OpenAIFunction,
}

#[derive(Debug, Serialize)]
struct OpenAIFunction {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    parameters: Value,
}

#[derive(Debug, Deserialize)]
struct OpenAIResponse {
    choices: Vec<OpenAIChoice>,
    usage: Option<OpenAIUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenAIChoice {
    message: OpenAIMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
struct OpenAIUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct OpenAIStreamChunk {
    #[allow(dead_code)]
    id: String,
    #[allow(dead_code)]
    object: String,
    choices: Vec<OpenAIStreamChoice>,
    #[serde(default)]
    usage: Option<OpenAIUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenAIStreamChoice {
    delta: OpenAIDelta,
    #[serde(default)]
    #[allow(dead_code)]
    index: usize,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAIDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<OpenAIToolCallDelta>>,
}

#[derive(Debug, Deserialize)]
struct OpenAIToolCallDelta {
    index: usize,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    r#type: Option<String>,
    #[serde(default)]
    function: Option<OpenAIFunctionDelta>,
}

#[derive(Debug, Deserialize)]
struct OpenAIFunctionDelta {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

// Conversion functions
fn convert_message(msg: &ChatMessage) -> Vec<OpenAIMessage> {
    match msg {
        ChatMessage::System(content) => vec![OpenAIMessage {
            role: "system".to_string(),
            content: Some(content.clone()),
            tool_calls: None,
            tool_call_id: None,
        }],
        ChatMessage::User(blocks) => {
            let mut messages = vec![];
            for block in blocks {
                match block {
                    ContentBlock::Text(text) => {
                        messages.push(OpenAIMessage {
                            role: "user".to_string(),
                            content: Some(text.clone()),
                            tool_calls: None,
                            tool_call_id: None,
                        });
                    }
                    ContentBlock::ToolResult {
                        tool_use_id,
                        content,
                        ..
                    } => {
                        let text = match content {
                            ToolResultContent::Text(t) => t.clone(),
                            ToolResultContent::Blocks(_) => {
                                "[complex result with blocks]".to_string()
                            }
                        };
                        messages.push(OpenAIMessage {
                            role: "tool".to_string(),
                            content: Some(text),
                            tool_calls: None,
                            tool_call_id: Some(tool_use_id.clone()),
                        });
                    }
                    _ => {}
                }
            }
            if messages.is_empty() {
                messages.push(OpenAIMessage {
                    role: "user".to_string(),
                    content: Some(String::new()),
                    tool_calls: None,
                    tool_call_id: None,
                });
            }
            messages
        }
        ChatMessage::Assistant { text, tool_calls } => {
            let openai_tool_calls = if tool_calls.is_empty() {
                None
            } else {
                Some(
                    tool_calls
                        .iter()
                        .map(|tc| OpenAIToolCall {
                            id: tc.id.clone(),
                            r#type: "function".to_string(),
                            function: OpenAIFunctionCall {
                                name: tc.name.clone(),
                                arguments: tc.input.to_string(),
                            },
                        })
                        .collect(),
                )
            };

            vec![OpenAIMessage {
                role: "assistant".to_string(),
                content: text.clone(),
                tool_calls: openai_tool_calls,
                tool_call_id: None,
            }]
        }
    }
}

fn convert_response(resp: OpenAIResponse) -> ChatResponse {
    let choice = &resp.choices[0];
    let message = &choice.message;

    let tool_calls = message
        .tool_calls
        .as_ref()
        .map(|tcs| {
            tcs.iter()
                .filter_map(|tc| {
                    serde_json::from_str(&tc.function.arguments)
                        .ok()
                        .map(|input| ToolCall {
                            id: tc.id.clone(),
                            name: tc.function.name.clone(),
                            input,
                        })
                })
                .collect()
        })
        .unwrap_or_default();

    let finish_reason = match choice.finish_reason.as_deref() {
        Some("stop") => FinishReason::Stop,
        Some("tool_calls") => FinishReason::ToolUse,
        Some("length") => FinishReason::Length,
        _ => FinishReason::Other,
    };

    let usage = resp.usage.map(|u| TokenUsage {
        input_tokens: u.prompt_tokens,
        output_tokens: u.completion_tokens,
        cache_read_tokens: None,
    });

    ChatResponse {
        text: message.content.clone(),
        tool_calls,
        finish_reason,
        usage,
    }
}

fn map_http_error(status: u16, body: &str) -> ProviderError {
    match status {
        401 => ProviderError::Auth("Invalid API key".to_string()),
        429 => {
            let retry_after = parse_retry_after(body);
            ProviderError::RateLimited {
                retry_after_secs: retry_after,
            }
        }
        503 => ProviderError::ServiceUnavailable,
        _ => ProviderError::Http {
            status,
            body: body.to_string(),
        },
    }
}

fn parse_retry_after(body: &str) -> Option<u64> {
    // Try to parse retry_after from JSON response
    if let Ok(json) = serde_json::from_str::<Value>(body) {
        if let Some(retry) = json.get("retry_after").and_then(|v| v.as_u64()) {
            return Some(retry);
        }
    }
    None
}

pub(crate) fn parse_sse_stream(
    data: &[u8],
) -> impl Iterator<Item = Result<StreamEvent, ProviderError>> {
    let text = String::from_utf8_lossy(data).to_string();
    let lines: Vec<&str> = text.lines().collect();

    let mut events = vec![StreamEvent::Start];
    let mut tool_call_state: std::collections::HashMap<usize, ToolCallBuilder> =
        std::collections::HashMap::new();

    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if !line.starts_with("data: ") {
            continue;
        }

        let data = &line[6..];
        if data == "[DONE]" {
            break;
        }

        let chunk: OpenAIStreamChunk = match serde_json::from_str(data) {
            Ok(c) => c,
            Err(_) => {
                events.push(StreamEvent::End {
                    finish_reason: FinishReason::Other,
                    usage: None,
                });
                return events.into_iter().map(Ok).collect::<Vec<_>>().into_iter();
            }
        };

        for choice in chunk.choices {
            if let Some(content) = choice.delta.content {
                if !content.is_empty() {
                    events.push(StreamEvent::TextChunk(content));
                }
            }

            if let Some(tool_calls) = choice.delta.tool_calls {
                for tc_delta in tool_calls {
                    let builder = tool_call_state.entry(tc_delta.index).or_insert_with(|| {
                        ToolCallBuilder {
                            id: None,
                            name: None,
                            arguments: String::new(),
                        }
                    });

                    if let Some(id) = tc_delta.id {
                        builder.id = Some(id.clone());
                    }

                    if let Some(function) = tc_delta.function {
                        if let Some(name) = function.name {
                            builder.name = Some(name.clone());
                        }
                        if let Some(args) = function.arguments {
                            builder.arguments.push_str(&args);
                        }
                    }

                    if let (Some(id), Some(name)) = (&builder.id, &builder.name) {
                        events.push(StreamEvent::ToolCallChunk {
                            index: tc_delta.index,
                            id: id.clone(),
                            name: name.clone(),
                            args_fragment: builder.arguments.clone(),
                        });
                    }
                }
            }

            if let Some(finish_reason) = choice.finish_reason {
                // Emit ready tool calls
                for (_, builder) in tool_call_state.iter() {
                    if let (Some(id), Some(name)) = (&builder.id, &builder.name) {
                        if let Ok(input) = serde_json::from_str(&builder.arguments) {
                            events.push(StreamEvent::ToolCallReady(ToolCall {
                                id: id.clone(),
                                name: name.clone(),
                                input,
                            }));
                        }
                    }
                }

                let reason = match finish_reason.as_str() {
                    "stop" => FinishReason::Stop,
                    "tool_calls" => FinishReason::ToolUse,
                    "length" => FinishReason::Length,
                    _ => FinishReason::Other,
                };

                let token_usage = chunk.usage.clone().map(|u| TokenUsage {
                    input_tokens: u.prompt_tokens,
                    output_tokens: u.completion_tokens,
                    cache_read_tokens: None,
                });

                events.push(StreamEvent::End {
                    finish_reason: reason,
                    usage: token_usage,
                });
            }
        }
    }

    events.into_iter().map(Ok).collect::<Vec<_>>().into_iter()
}

struct ToolCallBuilder {
    id: Option<String>,
    name: Option<String>,
    arguments: String,
}

#[cfg(test)]
#[path = "openai_tests.rs"]
mod tests;
