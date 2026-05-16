use std::sync::Arc;

use ai::provider::{ChatStream, StreamEvent, TokenUsage as ProviderTokenUsage, ToolCall};
use ai::url_validation::validate_direct_api_base_url;
use anyhow::anyhow;
use futures::{FutureExt, StreamExt};
use prost_types::FieldMask;
use uuid::Uuid;
use warp_multi_agent_api as api;

use super::{Event, RequestParams, ResponseStream};
use crate::ai::agent::api::ConvertToAPITypeError;
use crate::server::server_api::AIApiError;

pub fn validate_direct_route(params: &RequestParams) -> anyhow::Result<()> {
    let Some(config) = &params.direct_api_route_config else {
        if let Some(err) = params.direct_api_route_error.as_ref() {
            anyhow::bail!("{err}");
        }
        anyhow::bail!("Direct API routing is selected but no Direct API model is configured");
    };
    if config.model_id.trim().is_empty() {
        anyhow::bail!("Direct API routing is selected but the selected model is empty");
    }
    match config.provider_id {
        ai::model_registry::ProviderId::Ollama => {
            if let Some(base_url) = config.base_url.as_ref() {
                validate_direct_api_base_url(base_url)
                    .map_err(|err| anyhow!("Invalid Direct API base URL: {err:?}"))?;
            }
        }
        ai::model_registry::ProviderId::Custom => {
            let Some(base_url) = config
                .base_url
                .as_ref()
                .filter(|url| !url.trim().is_empty())
            else {
                anyhow::bail!("Direct API provider requires a base URL");
            };
            validate_direct_api_base_url(base_url)
                .map_err(|err| anyhow!("Invalid Direct API base URL: {err:?}"))?;
        }
        ai::model_registry::ProviderId::OpenRouter => {
            if config
                .api_key
                .as_ref()
                .is_none_or(|key| key.trim().is_empty())
            {
                anyhow::bail!("Direct API provider requires an API key");
            }
            let Some(base_url) = config
                .base_url
                .as_ref()
                .filter(|url| !url.trim().is_empty())
            else {
                anyhow::bail!("Direct API provider requires a base URL");
            };
            validate_direct_api_base_url(base_url)
                .map_err(|err| anyhow!("Invalid Direct API base URL: {err:?}"))?;
        }
        ai::model_registry::ProviderId::OpenAI
        | ai::model_registry::ProviderId::Anthropic
        | ai::model_registry::ProviderId::GoogleGemini => {
            if config
                .api_key
                .as_ref()
                .is_none_or(|key| key.trim().is_empty())
            {
                anyhow::bail!("Direct API provider requires an API key");
            }
        }
    }
    Ok(())
}

pub async fn generate_direct_api_output(
    params: RequestParams,
    cancellation_rx: futures::channel::oneshot::Receiver<()>,
) -> Result<ResponseStream, ConvertToAPITypeError> {
    if let Err(err) = validate_direct_route(&params) {
        return Ok(single_error_stream(err.to_string()));
    }

    let (tx, rx) = async_channel::unbounded::<Event>();
    let request_id = Uuid::new_v4().to_string();
    let conversation_id = params
        .conversation_token
        .as_ref()
        .map(|token| token.as_str().to_string())
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    tx.send(Ok(api::ResponseEvent {
        r#type: Some(api::response_event::Type::Init(
            api::response_event::StreamInit {
                conversation_id: conversation_id.clone(),
                request_id: request_id.clone(),
                run_id: conversation_id,
            },
        )),
    }))
    .await
    .ok();

    tokio::spawn(async move {
        let mut cancellation_rx = cancellation_rx.fuse();
        futures::select! {
            _ = cancellation_rx => {
                send_finished(&tx, api::response_event::stream_finished::Reason::Other(
                    api::response_event::stream_finished::Other {},
                ), Vec::new()).await;
            }
            result = run_direct_text_stream(params, request_id.clone(), tx.clone()).fuse() => {
                match result {
                    Ok(token_usage) => {
                        send_finished(&tx, api::response_event::stream_finished::Reason::Done(
                            api::response_event::stream_finished::Done {},
                        ), token_usage).await;
                    }
                    Err(err) => {
                        let _ = tx
                            .send(Err(Arc::new(AIApiError::Other(anyhow!(err)))))
                            .await;
                    }
                }
            }
        }
    });

    Ok(Box::pin(rx))
}

async fn run_direct_text_stream(
    params: RequestParams,
    request_id: String,
    tx: async_channel::Sender<Event>,
) -> anyhow::Result<Vec<api::response_event::stream_finished::TokenUsage>> {
    let task_id = params
        .tasks
        .first()
        .map(|task| task.id.clone())
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let message_id = Uuid::new_v4().to_string();
    let should_create_task = params.tasks.iter().all(|task| task.id != task_id);
    let model_id = params
        .direct_api_route_config
        .as_ref()
        .map(|config| config.model_id.clone())
        .unwrap_or_else(|| params.model.to_string());
    let mut stream = super::direct_tools::run_provider_stream(params).await?;

    run_direct_text_stream_events(
        task_id,
        should_create_task,
        request_id,
        message_id,
        model_id,
        tx,
        &mut stream,
    )
    .await
}

async fn run_direct_text_stream_events(
    task_id: String,
    should_create_task: bool,
    request_id: String,
    message_id: String,
    model_id: String,
    tx: async_channel::Sender<Event>,
    stream: &mut ChatStream,
) -> anyhow::Result<Vec<api::response_event::stream_finished::TokenUsage>> {
    let mut pending_tool_calls: PendingDirectToolCalls = Vec::new();

    if should_create_task {
        send_client_action(&tx, create_task_action(task_id.clone())).await?;
    }

    send_client_action(
        &tx,
        add_initial_agent_output_action(task_id.clone(), request_id.clone(), message_id.clone()),
    )
    .await?;

    while let Some(event) = stream.next().await {
        match event? {
            StreamEvent::Start => {}
            StreamEvent::TextChunk(chunk) => {
                if chunk.is_empty() {
                    continue;
                }
                send_client_action(
                    &tx,
                    append_agent_output_chunk_action(
                        task_id.clone(),
                        request_id.clone(),
                        message_id.clone(),
                        chunk,
                    ),
                )
                .await?;
            }
            StreamEvent::ReasoningChunk(chunk) => {
                if chunk.is_empty() {
                    continue;
                }
                send_client_action(
                    &tx,
                    add_reasoning_action(
                        task_id.clone(),
                        request_id.clone(),
                        Uuid::new_v4().to_string(),
                        chunk,
                    ),
                )
                .await?;
            }
            StreamEvent::ToolCallReady(tool_call) => {
                let proto_tool_call = super::direct_tools::provider_tool_call_to_proto(tool_call)?;
                send_client_action(
                    &tx,
                    add_tool_call_action(
                        task_id.clone(),
                        request_id.clone(),
                        Uuid::new_v4().to_string(),
                        proto_tool_call,
                    ),
                )
                .await?;
            }
            StreamEvent::ToolCallChunk {
                index,
                id,
                name,
                args_fragment,
            } => {
                push_direct_tool_call_chunk(&mut pending_tool_calls, index, id, name, args_fragment)
            }
            StreamEvent::End { usage, .. } => {
                for proto_tool_call in drain_direct_tool_call_chunks(&mut pending_tool_calls)? {
                    send_client_action(
                        &tx,
                        add_tool_call_action(
                            task_id.clone(),
                            request_id.clone(),
                            Uuid::new_v4().to_string(),
                            proto_tool_call,
                        ),
                    )
                    .await?;
                }
                return Ok(usage
                    .map(|usage| direct_api_token_usage(model_id, usage))
                    .into_iter()
                    .collect());
            }
        }
    }

    anyhow::bail!("Direct API provider stream ended without End event")
}

type PendingDirectToolCalls = Vec<(String, String, String)>;

fn push_direct_tool_call_chunk(
    pending_tool_calls: &mut PendingDirectToolCalls,
    index: usize,
    id: String,
    name: String,
    args_fragment: String,
) {
    if index >= pending_tool_calls.len() {
        pending_tool_calls.resize(index + 1, (String::new(), String::new(), String::new()));
    }
    let entry = &mut pending_tool_calls[index];
    if !id.is_empty() {
        entry.0 = id;
    }
    if !name.is_empty() {
        entry.1 = name;
    }
    entry.2.push_str(&args_fragment);
}

fn drain_direct_tool_call_chunks(
    pending_tool_calls: &mut PendingDirectToolCalls,
) -> anyhow::Result<Vec<api::message::ToolCall>> {
    pending_tool_calls
        .drain(..)
        .filter(|(id, name, _args_str)| !(id.is_empty() && name.is_empty()))
        .map(|(id, name, args_str)| {
            let input = serde_json::from_str(&args_str).unwrap_or(serde_json::Value::Null);
            super::direct_tools::provider_tool_call_to_proto(ToolCall { id, name, input })
        })
        .collect()
}

pub(super) fn create_task_action(task_id: String) -> api::client_action::Action {
    api::client_action::Action::CreateTask(api::client_action::CreateTask {
        task: Some(api::Task {
            id: task_id,
            messages: Vec::new(),
            dependencies: None,
            description: String::new(),
            summary: String::new(),
            server_data: String::new(),
        }),
    })
}

pub(super) fn add_initial_agent_output_action(
    task_id: String,
    request_id: String,
    message_id: String,
) -> api::client_action::Action {
    api::client_action::Action::AddMessagesToTask(api::client_action::AddMessagesToTask {
        task_id: task_id.clone(),
        messages: vec![agent_output_message(
            task_id,
            request_id,
            message_id,
            String::new(),
        )],
    })
}

pub(super) fn append_agent_output_chunk_action(
    task_id: String,
    request_id: String,
    message_id: String,
    chunk: String,
) -> api::client_action::Action {
    api::client_action::Action::AppendToMessageContent(api::client_action::AppendToMessageContent {
        task_id: task_id.clone(),
        message: Some(agent_output_message(task_id, request_id, message_id, chunk)),
        mask: Some(FieldMask {
            paths: vec!["message.agent_output.text".to_string()],
        }),
    })
}

pub(super) fn add_tool_call_action(
    task_id: String,
    request_id: String,
    message_id: String,
    tool_call: api::message::ToolCall,
) -> api::client_action::Action {
    api::client_action::Action::AddMessagesToTask(api::client_action::AddMessagesToTask {
        task_id: task_id.clone(),
        messages: vec![api::Message {
            id: message_id,
            task_id,
            request_id,
            timestamp: None,
            server_message_data: String::new(),
            citations: Vec::new(),
            message: Some(api::message::Message::ToolCall(tool_call)),
        }],
    })
}

pub(super) fn add_reasoning_action(
    task_id: String,
    request_id: String,
    message_id: String,
    reasoning: String,
) -> api::client_action::Action {
    api::client_action::Action::AddMessagesToTask(api::client_action::AddMessagesToTask {
        task_id: task_id.clone(),
        messages: vec![api::Message {
            id: message_id,
            task_id,
            request_id,
            timestamp: None,
            server_message_data: String::new(),
            citations: Vec::new(),
            message: Some(api::message::Message::AgentReasoning(
                api::message::AgentReasoning {
                    reasoning,
                    finished_duration: None,
                },
            )),
        }],
    })
}

async fn send_client_action(
    tx: &async_channel::Sender<Event>,
    action: api::client_action::Action,
) -> anyhow::Result<()> {
    tx.send(Ok(api::ResponseEvent {
        r#type: Some(api::response_event::Type::ClientActions(
            api::response_event::ClientActions {
                actions: vec![api::ClientAction {
                    action: Some(action),
                }],
            },
        )),
    }))
    .await?;
    Ok(())
}

fn agent_output_message(
    task_id: String,
    request_id: String,
    message_id: String,
    text: String,
) -> api::Message {
    api::Message {
        id: message_id,
        task_id,
        request_id,
        timestamp: None,
        server_message_data: String::new(),
        citations: Vec::new(),
        message: Some(api::message::Message::AgentOutput(
            api::message::AgentOutput { text },
        )),
    }
}

fn direct_api_token_usage(
    model_id: String,
    usage: ProviderTokenUsage,
) -> api::response_event::stream_finished::TokenUsage {
    api::response_event::stream_finished::TokenUsage {
        model_id,
        total_input: usage.input_tokens,
        output: usage.output_tokens,
        input_cache_read: usage.cache_read_tokens.unwrap_or_default(),
        input_cache_write: 0,
        cost_in_cents: 0.0,
    }
}

async fn send_finished(
    tx: &async_channel::Sender<Event>,
    reason: api::response_event::stream_finished::Reason,
    token_usage: Vec<api::response_event::stream_finished::TokenUsage>,
) {
    let _ = tx
        .send(Ok(api::ResponseEvent {
            r#type: Some(api::response_event::Type::Finished(
                api::response_event::StreamFinished {
                    reason: Some(reason),
                    token_usage,
                    should_refresh_model_config: false,
                    request_cost: None,
                    conversation_usage_metadata: None,
                },
            )),
        }))
        .await;
}

fn single_error_stream(message: String) -> ResponseStream {
    let (tx, rx) = async_channel::unbounded();
    tokio::spawn(async move {
        let _ = tx
            .send(Err(Arc::new(AIApiError::Other(anyhow!(message)))))
            .await;
    });
    Box::pin(rx)
}

#[cfg(test)]
mod rig_stream_tests {
    use super::*;
    use ai::provider::{FinishReason, StreamEvent};
    use futures::stream;

    #[tokio::test]
    async fn rig_stream_direct_api_emits_tool_call_action_from_chunks_on_end() {
        let mut stream: ChatStream = Box::pin(stream::iter(vec![
            Ok(StreamEvent::Start),
            Ok(StreamEvent::ToolCallChunk {
                index: 0,
                id: "call-read".to_string(),
                name: "ReadFiles".to_string(),
                args_fragment: String::new(),
            }),
            Ok(StreamEvent::ToolCallChunk {
                index: 0,
                id: String::new(),
                name: String::new(),
                args_fragment: r#"{"files":["#.to_string(),
            }),
            Ok(StreamEvent::ToolCallChunk {
                index: 0,
                id: String::new(),
                name: String::new(),
                args_fragment: r#"{"name":"Cargo.toml"}]}"#.to_string(),
            }),
            Ok(StreamEvent::End {
                finish_reason: FinishReason::ToolUse,
                usage: None,
            }),
        ]));
        let (tx, rx) = async_channel::unbounded();

        let token_usage = run_direct_text_stream_events(
            "task-local".to_string(),
            false,
            "request-local".to_string(),
            "message-local".to_string(),
            "test-model".to_string(),
            tx.clone(),
            &mut stream,
        )
        .await
        .unwrap();
        drop(tx);
        assert!(token_usage.is_empty());

        let mut tool_call_actions = Vec::new();
        while let Ok(event) = rx.recv().await {
            let event = event.unwrap();
            let Some(api::response_event::Type::ClientActions(client_actions)) = event.r#type
            else {
                continue;
            };
            for action in client_actions.actions {
                let Some(api::client_action::Action::AddMessagesToTask(add)) = action.action else {
                    continue;
                };
                for message in add.messages {
                    if let Some(api::message::Message::ToolCall(tool_call)) = message.message {
                        tool_call_actions.push(tool_call);
                    }
                }
            }
        }

        assert_eq!(tool_call_actions.len(), 1);
        assert_eq!(tool_call_actions[0].tool_call_id, "call-read");
        let Some(api::message::tool_call::Tool::ReadFiles(read_files)) =
            tool_call_actions[0].tool.as_ref()
        else {
            panic!("expected ReadFiles tool call");
        };
        assert_eq!(read_files.files.len(), 1);
        assert_eq!(read_files.files[0].name, "Cargo.toml");
    }

    #[tokio::test]
    async fn rig_stream_direct_api_errors_when_stream_ends_without_end_event() {
        let mut stream: ChatStream = Box::pin(stream::iter(Vec::new()));
        let (tx, _rx) = async_channel::unbounded();

        let err = run_direct_text_stream_events(
            "task-local".to_string(),
            false,
            "request-local".to_string(),
            "message-local".to_string(),
            "test-model".to_string(),
            tx,
            &mut stream,
        )
        .await
        .unwrap_err();

        assert!(err.to_string().contains("stream ended without End event"));
    }

    #[tokio::test]
    async fn rig_stream_direct_api_maps_usage_from_end_event() {
        let mut stream: ChatStream = Box::pin(stream::iter(vec![Ok(StreamEvent::End {
            finish_reason: FinishReason::Stop,
            usage: Some(ProviderTokenUsage {
                input_tokens: 9,
                output_tokens: 4,
                cache_read_tokens: Some(2),
            }),
        })]));
        let (tx, _rx) = async_channel::unbounded();

        let token_usage = run_direct_text_stream_events(
            "task-local".to_string(),
            false,
            "request-local".to_string(),
            "message-local".to_string(),
            "test-model".to_string(),
            tx,
            &mut stream,
        )
        .await
        .unwrap();

        assert_eq!(token_usage.len(), 1);
        assert_eq!(token_usage[0].model_id, "test-model");
        assert_eq!(token_usage[0].total_input, 9);
        assert_eq!(token_usage[0].output, 4);
        assert_eq!(token_usage[0].input_cache_read, 2);
    }

    #[tokio::test]
    async fn rig_stream_direct_api_emits_reasoning_action() {
        let mut stream: ChatStream = Box::pin(stream::iter(vec![
            Ok(StreamEvent::ReasoningChunk("thinking".to_string())),
            Ok(StreamEvent::End {
                finish_reason: FinishReason::Stop,
                usage: None,
            }),
        ]));
        let (tx, rx) = async_channel::unbounded();

        run_direct_text_stream_events(
            "task-local".to_string(),
            false,
            "request-local".to_string(),
            "message-local".to_string(),
            "test-model".to_string(),
            tx.clone(),
            &mut stream,
        )
        .await
        .unwrap();
        drop(tx);

        let mut reasoning = Vec::new();
        while let Ok(event) = rx.recv().await {
            let event = event.unwrap();
            let Some(api::response_event::Type::ClientActions(client_actions)) = event.r#type
            else {
                continue;
            };
            for action in client_actions.actions {
                let Some(api::client_action::Action::AddMessagesToTask(add)) = action.action else {
                    continue;
                };
                for message in add.messages {
                    if let Some(api::message::Message::AgentReasoning(agent_reasoning)) =
                        message.message
                    {
                        reasoning.push(agent_reasoning.reasoning);
                    }
                }
            }
        }

        assert_eq!(reasoning, vec!["thinking".to_string()]);
    }
}
