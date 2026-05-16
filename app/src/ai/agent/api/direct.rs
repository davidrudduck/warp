use std::sync::Arc;

use ai::provider::{StreamEvent, ToolCall};
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
                )).await;
            }
            result = run_direct_text_stream(params, request_id.clone(), tx.clone()).fuse() => {
                match result {
                    Ok(()) => {
                        send_finished(&tx, api::response_event::stream_finished::Reason::Done(
                            api::response_event::stream_finished::Done {},
                        )).await;
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
) -> anyhow::Result<()> {
    let task_id = params
        .tasks
        .first()
        .map(|task| task.id.clone())
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let message_id = Uuid::new_v4().to_string();
    let should_create_task = params.tasks.iter().all(|task| task.id != task_id);
    let mut stream = super::direct_tools::run_provider_stream(params).await?;
    let mut pending_tool_calls: Vec<(String, String, String)> = Vec::new();

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
                if index >= pending_tool_calls.len() {
                    pending_tool_calls
                        .resize(index + 1, (String::new(), String::new(), String::new()));
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
            StreamEvent::End { .. } => {
                for (id, name, args_str) in pending_tool_calls.drain(..) {
                    if id.is_empty() && name.is_empty() {
                        continue;
                    }
                    let input = serde_json::from_str(&args_str).unwrap_or(serde_json::Value::Null);
                    let proto_tool_call =
                        super::direct_tools::provider_tool_call_to_proto(ToolCall {
                            id,
                            name,
                            input,
                        })?;
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
            }
        }
    }

    Ok(())
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

async fn send_finished(
    tx: &async_channel::Sender<Event>,
    reason: api::response_event::stream_finished::Reason,
) {
    let _ = tx
        .send(Ok(api::ResponseEvent {
            r#type: Some(api::response_event::Type::Finished(
                api::response_event::StreamFinished {
                    reason: Some(reason),
                    token_usage: Vec::new(),
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
