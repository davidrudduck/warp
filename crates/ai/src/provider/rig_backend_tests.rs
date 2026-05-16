use super::{
    rig_completion_request_from_chat_request, stream_event_from_rig_backend_event,
    RigBackendConfig, RigBackendEvent, RigEventStream, RigProviderKind, RigStreamMapper,
};
use crate::provider::{
    agent_event_channel, mock::MockLlmProvider, AgentEvent, ChatMessage, ChatOptions, ChatRequest,
    ChatResponse, ChatStream, ContentBlock, FinishReason, LlmProvider, ModelCapabilities,
    ProviderError, ProviderKind, SharedProvider, StreamEvent, TokenUsage, ToolCall,
    ToolResultContent,
};
use async_trait::async_trait;
use futures::{FutureExt, StreamExt};
use rig_core::message::{
    AssistantContent as RigAssistantContent, Message as RigMessage, ToolCall as RigToolCall,
    ToolFunction as RigToolFunction, ToolResultContent as RigToolResultContent,
    UserContent as RigUserContent,
};
use rig_core::streaming::{StreamedAssistantContent, ToolCallDeltaContent};
use serde_json::json;
use std::sync::Arc;

#[test]
fn rig_backend_config_maps_openrouter() {
    let config = RigBackendConfig::new(
        RigProviderKind::OpenRouter,
        "moonshotai/kimi-k2.6",
        Some("test-key".to_string()),
        Some("https://openrouter.ai/api/v1".to_string()),
    );

    assert_eq!(config.provider_kind, RigProviderKind::OpenRouter);
    assert_eq!(config.model_id, "moonshotai/kimi-k2.6");
}

#[test]
fn rig_backend_config_rejects_missing_key_for_openrouter() {
    let err = RigBackendConfig::new(
        RigProviderKind::OpenRouter,
        "moonshotai/kimi-k2.6",
        None,
        Some("https://openrouter.ai/api/v1".to_string()),
    )
    .validate()
    .unwrap_err();

    assert!(err.to_string().contains("requires an API key"));
}

#[test]
fn rig_backend_config_rejects_empty_model_id() {
    let err = RigBackendConfig::new(RigProviderKind::Ollama, "  ", None, None)
        .validate()
        .unwrap_err();

    assert!(err.to_string().contains("requires a model"));
}

#[test]
fn rig_backend_config_allows_ollama_without_api_key() {
    RigBackendConfig::new(RigProviderKind::Ollama, "llama3.2", None, None)
        .validate()
        .unwrap();
}

#[test]
fn rig_backend_config_rejects_custom_endpoint_without_base_url() {
    let err = RigBackendConfig::new(
        RigProviderKind::CustomOpenAICompatible,
        "custom-model",
        Some("test-key".to_string()),
        None,
    )
    .validate()
    .unwrap_err();

    assert!(err.to_string().contains("requires a base URL"));
}

#[test]
fn rig_backend_failure_diagnostic_adds_safe_error_category() {
    let config = RigBackendConfig::new(
        RigProviderKind::CustomOpenAICompatible,
        "private-model\napi_key=secret",
        Some("sk-secret".to_string()),
        Some("http://localhost:11434/v1".to_string()),
    );
    let mut diagnostic = super::rig_diagnostic_event_from_config(&config);

    let rendered = super::categorized_rig_diagnostic(
        &mut diagnostic,
        &ProviderError::UnsupportedModel("raw error details".to_string()),
    );

    assert!(rendered.contains("error_category=unsupported_model"));
    assert!(rendered.contains("provider=CustomOpenAICompatible"));
    assert!(rendered.contains("model_id_hash="));
    assert!(!rendered.contains("private-model"));
    assert!(!rendered.contains("api_key"));
    assert!(!rendered.contains("sk-secret"));
    assert!(!rendered.contains("raw error details"));
}

#[tokio::test]
async fn rig_backend_emits_tool_call_without_executing_tool() {
    let mut backend = FakeRigBackend::new().with_streamed_tool_call(
        "call_read",
        "ReadFiles",
        r#"{"files":[{"name":"Cargo.toml"}]}"#,
    );

    let events = backend.collect_events_until_tool_call().await.unwrap();

    assert!(events.iter().any(|event| matches!(
        event,
        RigBackendEvent::ToolCallReady(call)
            if call.id == "call_read" && call.name == "ReadFiles"
    )));
    assert_eq!(backend.executed_tool_count(), 0);
}

#[tokio::test]
async fn rig_backend_can_resume_after_external_tool_result() {
    let mut backend = FakeRigBackend::new()
        .with_streamed_tool_call(
            "call_read",
            "ReadFiles",
            r#"{"files":[{"name":"Cargo.toml"}]}"#,
        )
        .with_final_text_after_tool_result("The package is warp.");

    let first = backend.collect_events_until_tool_call().await.unwrap();
    assert!(first
        .iter()
        .any(|event| matches!(event, RigBackendEvent::ToolCallReady(_))));

    let second = backend
        .resume_with_tool_result("call_read", "Cargo.toml contents")
        .await
        .unwrap();

    assert!(second.iter().any(|event| matches!(
        event,
        RigBackendEvent::TextChunk(text) if text.contains("warp")
    )));
}

#[test]
fn rig_stream_mapper_assigns_stable_indices_to_interleaved_tool_deltas() {
    let mut mapper = RigStreamMapper::default();

    let first = mapper
        .map_stream_item::<()>(StreamedAssistantContent::ToolCallDelta {
            id: "call_read".to_string(),
            internal_call_id: "internal_read".to_string(),
            content: ToolCallDeltaContent::Name("ReadFiles".to_string()),
        })
        .unwrap()
        .unwrap();
    let second = mapper
        .map_stream_item::<()>(StreamedAssistantContent::ToolCallDelta {
            id: "call_shell".to_string(),
            internal_call_id: "internal_shell".to_string(),
            content: ToolCallDeltaContent::Name("RunShellCommand".to_string()),
        })
        .unwrap()
        .unwrap();
    let third = mapper
        .map_stream_item::<()>(StreamedAssistantContent::ToolCallDelta {
            id: "call_read".to_string(),
            internal_call_id: "internal_read".to_string(),
            content: ToolCallDeltaContent::Delta(r#"{"files":["Cargo.toml"]}"#.to_string()),
        })
        .unwrap()
        .unwrap();

    assert!(matches!(
        first,
        RigBackendEvent::ToolCallDelta { index: 0, .. }
    ));
    assert!(matches!(
        second,
        RigBackendEvent::ToolCallDelta { index: 1, .. }
    ));
    assert!(matches!(
        third,
        RigBackendEvent::ToolCallDelta {
            index: 0,
            args_fragment,
            ..
        } if args_fragment.contains("Cargo.toml")
    ));
}

#[test]
fn rig_stream_mapper_ends_with_tool_use_after_tool_call() {
    let mut mapper = RigStreamMapper::default();

    mapper
        .map_stream_item::<()>(StreamedAssistantContent::ToolCall {
            tool_call: RigToolCall::new(
                "call_read".to_string(),
                RigToolFunction::new("ReadFiles".to_string(), json!({"files":["Cargo.toml"]})),
            ),
            internal_call_id: "internal_read".to_string(),
        })
        .unwrap();
    let end = mapper
        .map_stream_item(StreamedAssistantContent::Final(()))
        .unwrap()
        .unwrap();

    assert!(matches!(
        end,
        RigBackendEvent::End {
            finish_reason: FinishReason::ToolUse,
            ..
        }
    ));
}

#[test]
fn rig_stream_preserves_text_order() {
    let events = rig_events_from_stream_items(vec![
        StreamedAssistantContent::text("one"),
        StreamedAssistantContent::text(" two"),
        StreamedAssistantContent::Final(()),
    ]);

    assert_eq!(
        events,
        vec![
            RigBackendEvent::TextChunk("one".to_string()),
            RigBackendEvent::TextChunk(" two".to_string()),
            RigBackendEvent::End {
                finish_reason: FinishReason::Stop,
                usage: None,
            },
        ]
    );
}

#[test]
fn rig_stream_preserves_reasoning_chunks() {
    let events = rig_events_from_stream_items(vec![
        StreamedAssistantContent::Reasoning(rig_core::message::Reasoning::new("think")),
        StreamedAssistantContent::ReasoningDelta {
            id: Some("reasoning-1".to_string()),
            reasoning: "ing".to_string(),
        },
        StreamedAssistantContent::text("done"),
        StreamedAssistantContent::Final(()),
    ]);

    assert_eq!(
        events,
        vec![
            RigBackendEvent::ReasoningChunk("think".to_string()),
            RigBackendEvent::ReasoningChunk("ing".to_string()),
            RigBackendEvent::TextChunk("done".to_string()),
            RigBackendEvent::End {
                finish_reason: FinishReason::Stop,
                usage: None,
            },
        ]
    );

    let mut chat_stream = super::rig_event_stream_to_chat_stream(Box::pin(futures::stream::iter(
        events.into_iter().map(Ok),
    )));
    let chat_events = futures::executor::block_on(async {
        let mut chat_events = Vec::new();
        while let Some(event) = chat_stream.next().await {
            chat_events.push(event.unwrap());
        }
        chat_events
    });

    assert_eq!(chat_events.len(), 4);
    assert!(matches!(
        &chat_events[0],
        StreamEvent::ReasoningChunk(text) if text == "think"
    ));
    assert!(matches!(
        &chat_events[1],
        StreamEvent::ReasoningChunk(text) if text == "ing"
    ));
    assert!(matches!(
        &chat_events[2],
        StreamEvent::TextChunk(text) if text == "done"
    ));
    assert!(matches!(
        &chat_events[3],
        StreamEvent::End {
            finish_reason: FinishReason::Stop,
            usage: None,
        }
    ));
}

#[test]
fn rig_stream_assembles_tool_arguments_from_deltas() {
    let events = rig_events_from_stream_items(vec![
        StreamedAssistantContent::ToolCallDelta {
            id: "call_read".to_string(),
            internal_call_id: "internal_read".to_string(),
            content: ToolCallDeltaContent::Name("ReadFiles".to_string()),
        },
        StreamedAssistantContent::ToolCallDelta {
            id: "call_read".to_string(),
            internal_call_id: "internal_read".to_string(),
            content: ToolCallDeltaContent::Delta(r#"{"files":["#.to_string()),
        },
        StreamedAssistantContent::ToolCallDelta {
            id: "call_read".to_string(),
            internal_call_id: "internal_read".to_string(),
            content: ToolCallDeltaContent::Delta(r#""Cargo.toml"]}"#.to_string()),
        },
        StreamedAssistantContent::Final(()),
    ]);

    assert_eq!(
        events,
        vec![
            RigBackendEvent::ToolCallDelta {
                index: 0,
                id: "call_read".to_string(),
                name: "ReadFiles".to_string(),
                args_fragment: String::new(),
            },
            RigBackendEvent::ToolCallDelta {
                index: 0,
                id: "call_read".to_string(),
                name: String::new(),
                args_fragment: r#"{"files":["#.to_string(),
            },
            RigBackendEvent::ToolCallDelta {
                index: 0,
                id: "call_read".to_string(),
                name: String::new(),
                args_fragment: r#""Cargo.toml"]}"#.to_string(),
            },
            RigBackendEvent::End {
                finish_reason: FinishReason::ToolUse,
                usage: None,
            },
        ]
    );
}

#[test]
fn rig_stream_preserves_tool_call_ids() {
    let events = rig_events_from_stream_items(vec![
        StreamedAssistantContent::ToolCall {
            tool_call: RigToolCall::new(
                "call_read".to_string(),
                RigToolFunction::new("ReadFiles".to_string(), json!({"files":["Cargo.toml"]})),
            ),
            internal_call_id: "internal_read".to_string(),
        },
        StreamedAssistantContent::ToolCall {
            tool_call: RigToolCall::new(
                "call_grep".to_string(),
                RigToolFunction::new("Grep".to_string(), json!({"queries":["TODO"]})),
            ),
            internal_call_id: "internal_grep".to_string(),
        },
        StreamedAssistantContent::Final(()),
    ]);

    assert_eq!(
        events,
        vec![
            RigBackendEvent::ToolCallReady(ToolCall {
                id: "call_read".to_string(),
                name: "ReadFiles".to_string(),
                input: json!({"files":["Cargo.toml"]}),
            }),
            RigBackendEvent::ToolCallReady(ToolCall {
                id: "call_grep".to_string(),
                name: "Grep".to_string(),
                input: json!({"queries":["TODO"]}),
            }),
            RigBackendEvent::End {
                finish_reason: FinishReason::ToolUse,
                usage: None,
            },
        ]
    );
}

#[test]
fn rig_stream_maps_usage_on_end() {
    let events = rig_events_from_stream_items(vec![StreamedAssistantContent::Final(
        UsageResponse::new(11, 7, 3),
    )]);

    assert_eq!(
        events,
        vec![RigBackendEvent::End {
            finish_reason: FinishReason::Stop,
            usage: Some(TokenUsage {
                input_tokens: 11,
                output_tokens: 7,
                cache_read_tokens: Some(3),
            }),
        }]
    );
}

#[test]
fn rig_stream_empty_success_is_error() {
    let provider = RigStreamProvider::queued(Vec::new());
    let (tx, _rx) = agent_event_channel(16);
    let (_cancel_tx, cancel_rx) = futures::channel::oneshot::channel();
    let mut cancel_signal = cancel_rx.fuse();

    let err = futures::executor::block_on(crate::direct_loop::collect_and_emit_stream(
        &provider,
        ChatRequest {
            messages: vec![ChatMessage::User(vec![ContentBlock::Text("hello".into())])],
            tools: Vec::new(),
            options: ChatOptions::default(),
        },
        &tx,
        &mut cancel_signal,
    ))
    .unwrap_err();

    assert!(
        matches!(err, ProviderError::StreamParse(message) if message.contains("without End event"))
    );
}

#[tokio::test]
async fn rig_stream_cancellation_stops_events() {
    let (provider, rig_tx) = RigStreamProvider::controlled();
    let (tx, mut rx) = agent_event_channel(16);
    let (cancel_tx, cancel_rx) = futures::channel::oneshot::channel();
    let mut cancel_signal = cancel_rx.fuse();

    let handle = tokio::spawn(async move {
        crate::direct_loop::collect_and_emit_stream(
            &provider,
            ChatRequest {
                messages: vec![ChatMessage::User(vec![ContentBlock::Text("cancel".into())])],
                tools: Vec::new(),
                options: ChatOptions::default(),
            },
            &tx,
            &mut cancel_signal,
        )
        .await
    });
    rig_tx.send(Ok(RigBackendEvent::Start)).await.unwrap();
    rig_tx
        .send(Ok(RigBackendEvent::TextChunk("before cancel".to_string())))
        .await
        .unwrap();

    let first_event = rx.recv().await.unwrap();
    assert!(matches!(
        first_event,
        AgentEvent::TextChunk(text) if text == "before cancel"
    ));
    cancel_tx.send(()).unwrap();

    let result = handle.await.unwrap();
    assert!(matches!(result, Err(ProviderError::Cancelled)));
    assert!(rx.try_recv().is_err());
}

#[tokio::test]
async fn rig_stream_end_after_cancellation_does_not_emit_stale_events() {
    let (provider, rig_tx) = RigStreamProvider::controlled();
    let (tx, mut rx) = agent_event_channel(16);
    let (cancel_tx, cancel_rx) = futures::channel::oneshot::channel();
    let mut cancel_signal = cancel_rx.fuse();

    let handle = tokio::spawn(async move {
        crate::direct_loop::collect_and_emit_stream(
            &provider,
            ChatRequest {
                messages: vec![ChatMessage::User(vec![ContentBlock::Text("cancel".into())])],
                tools: Vec::new(),
                options: ChatOptions::default(),
            },
            &tx,
            &mut cancel_signal,
        )
        .await
    });
    rig_tx.send(Ok(RigBackendEvent::Start)).await.unwrap();
    rig_tx
        .send(Ok(RigBackendEvent::TextChunk("before cancel".to_string())))
        .await
        .unwrap();
    assert!(matches!(
        rx.recv().await.unwrap(),
        AgentEvent::TextChunk(text) if text == "before cancel"
    ));
    cancel_tx.send(()).unwrap();
    rig_tx
        .send(Ok(RigBackendEvent::End {
            finish_reason: FinishReason::Stop,
            usage: None,
        }))
        .await
        .unwrap();

    let result = handle.await.unwrap();
    assert!(matches!(result, Err(ProviderError::Cancelled)));
    assert!(rx.try_recv().is_err());
}

#[tokio::test]
async fn rig_stream_reasoning_does_not_block_direct_loop_completion() {
    let provider = RigStreamProvider::queued(vec![
        Ok(RigBackendEvent::Start),
        Ok(RigBackendEvent::ReasoningChunk(
            "private reasoning".to_string(),
        )),
        Ok(RigBackendEvent::TextChunk("visible".to_string())),
        Ok(RigBackendEvent::End {
            finish_reason: FinishReason::Stop,
            usage: None,
        }),
    ]);
    let (tx, mut rx) = agent_event_channel(16);
    let (_cancel_tx, cancel_rx) = futures::channel::oneshot::channel();
    let mut cancel_signal = cancel_rx.fuse();

    let (finish_reason, usage, tool_calls) = crate::direct_loop::collect_and_emit_stream(
        &provider,
        ChatRequest {
            messages: vec![ChatMessage::User(vec![ContentBlock::Text("reason".into())])],
            tools: Vec::new(),
            options: ChatOptions::default(),
        },
        &tx,
        &mut cancel_signal,
    )
    .await
    .unwrap();
    drop(tx);

    assert_eq!(finish_reason, FinishReason::Stop);
    assert!(usage.is_none());
    assert!(tool_calls.is_empty());

    let mut received = vec![];
    while let Some(event) = rx.recv().await {
        received.push(event);
    }
    assert!(received.iter().any(|event| matches!(
        event,
        AgentEvent::ReasoningChunk(reasoning) if reasoning == "private reasoning"
    )));
    assert!(received
        .iter()
        .any(|event| matches!(event, AgentEvent::TextChunk(text) if text == "visible")));
    assert!(received.iter().any(|event| matches!(
        event,
        AgentEvent::Done {
            finish_reason: FinishReason::Stop,
            usage: None,
        }
    )));
}

#[tokio::test]
async fn rig_delta_ready_final_reaches_direct_loop_as_one_tool_call() {
    let mut mapper = RigStreamMapper::default();
    let rig_items: Vec<StreamedAssistantContent<()>> = vec![
        StreamedAssistantContent::ToolCallDelta {
            id: "call_read".to_string(),
            internal_call_id: "internal_read".to_string(),
            content: ToolCallDeltaContent::Name("ReadFiles".to_string()),
        },
        StreamedAssistantContent::ToolCallDelta {
            id: "call_read".to_string(),
            internal_call_id: "internal_read".to_string(),
            content: ToolCallDeltaContent::Delta(r#"{"files":["Cargo.toml"]}"#.to_string()),
        },
        StreamedAssistantContent::ToolCall {
            tool_call: RigToolCall::new(
                "call_read".to_string(),
                RigToolFunction::new("ReadFiles".to_string(), json!({"files":["Cargo.toml"]})),
            ),
            internal_call_id: "internal_read".to_string(),
        },
        StreamedAssistantContent::Final(()),
    ];
    let mut stream_events = vec![];
    for item in rig_items {
        if let Some(event) = mapper.map_stream_item(item).unwrap() {
            if let Some(stream_event) = stream_event_from_rig_backend_event(event) {
                stream_events.push(stream_event);
            }
        }
    }

    let provider: SharedProvider = Arc::new(MockLlmProvider::new().with_stream(stream_events));
    let (tx, mut rx) = agent_event_channel(16);
    let (_cancel_tx, cancel_rx) = futures::channel::oneshot::channel();
    let mut cancel_signal = cancel_rx.fuse();

    let (_finish_reason, _usage, tool_calls) = crate::direct_loop::collect_and_emit_stream(
        &provider,
        ChatRequest {
            messages: vec![ChatMessage::User(vec![ContentBlock::Text(
                "read it".into(),
            )])],
            tools: Vec::new(),
            options: ChatOptions::default(),
        },
        &tx,
        &mut cancel_signal,
    )
    .await
    .unwrap();
    drop(tx);

    let mut received = vec![];
    while let Some(event) = rx.recv().await {
        received.push(event);
    }
    let tool_event_count = received
        .iter()
        .filter(|event| matches!(event, AgentEvent::ToolCallReady(_)))
        .count();

    assert_eq!(tool_calls.len(), 1);
    assert_eq!(tool_event_count, 1);
    assert_eq!(tool_calls[0].id, "call_read");
    assert_eq!(tool_calls[0].name, "ReadFiles");
}

#[tokio::test]
async fn rig_name_delta_ready_final_preserves_empty_tool_arguments() {
    let mut mapper = RigStreamMapper::default();
    let rig_items: Vec<StreamedAssistantContent<()>> = vec![
        StreamedAssistantContent::ToolCallDelta {
            id: "call_status".to_string(),
            internal_call_id: "internal_status".to_string(),
            content: ToolCallDeltaContent::Name("GetStatus".to_string()),
        },
        StreamedAssistantContent::ToolCall {
            tool_call: RigToolCall::new(
                "call_status".to_string(),
                RigToolFunction::new("GetStatus".to_string(), json!({})),
            ),
            internal_call_id: "internal_status".to_string(),
        },
        StreamedAssistantContent::Final(()),
    ];
    let mut stream_events = vec![];
    for item in rig_items {
        if let Some(event) = mapper.map_stream_item(item).unwrap() {
            if let Some(stream_event) = stream_event_from_rig_backend_event(event) {
                stream_events.push(stream_event);
            }
        }
    }

    let provider: SharedProvider = Arc::new(MockLlmProvider::new().with_stream(stream_events));
    let (tx, mut rx) = agent_event_channel(16);
    let (_cancel_tx, cancel_rx) = futures::channel::oneshot::channel();
    let mut cancel_signal = cancel_rx.fuse();

    let (_finish_reason, _usage, tool_calls) = crate::direct_loop::collect_and_emit_stream(
        &provider,
        ChatRequest {
            messages: vec![ChatMessage::User(vec![ContentBlock::Text(
                "get status".into(),
            )])],
            tools: Vec::new(),
            options: ChatOptions::default(),
        },
        &tx,
        &mut cancel_signal,
    )
    .await
    .unwrap();
    drop(tx);

    let mut received = vec![];
    while let Some(event) = rx.recv().await {
        received.push(event);
    }
    let tool_event_count = received
        .iter()
        .filter(|event| matches!(event, AgentEvent::ToolCallReady(_)))
        .count();

    assert_eq!(tool_calls.len(), 1);
    assert_eq!(tool_event_count, 1);
    assert_eq!(tool_calls[0].id, "call_status");
    assert_eq!(tool_calls[0].name, "GetStatus");
    assert_eq!(tool_calls[0].input, json!({}));
}

#[test]
fn rig_request_conversion_preserves_external_tool_result_turn() {
    let request = ChatRequest {
        messages: vec![
            ChatMessage::Assistant {
                text: None,
                tool_calls: vec![ToolCall {
                    id: "call_read".to_string(),
                    name: "ReadFiles".to_string(),
                    input: json!({"files":[{"name":"Cargo.toml"}]}),
                }],
            },
            ChatMessage::User(vec![ContentBlock::ToolResult {
                tool_use_id: "call_read".to_string(),
                content: ToolResultContent::Text("Cargo.toml contents".to_string()),
                is_error: false,
            }]),
        ],
        tools: Vec::new(),
        options: ChatOptions::default(),
    };

    let rig_request = rig_completion_request_from_chat_request(request).unwrap();
    let RigMessage::Assistant { content, .. } = rig_request.chat_history.first_ref() else {
        panic!("expected assistant tool-call message first");
    };
    let RigAssistantContent::ToolCall(tool_call) = content.first_ref() else {
        panic!("expected assistant tool call content");
    };
    assert_eq!(tool_call.id, "call_read");
    assert_eq!(tool_call.function.name, "ReadFiles");

    let rest = rig_request.chat_history.rest();
    let RigMessage::User { content } = &rest[0] else {
        panic!("expected user tool-result message second");
    };
    let RigUserContent::ToolResult(tool_result) = content.first_ref() else {
        panic!("expected user tool result content");
    };
    assert_eq!(tool_result.id, "call_read");
    let RigToolResultContent::Text(text) = tool_result.content.first_ref() else {
        panic!("expected text tool result");
    };
    assert_eq!(text.text, "Cargo.toml contents");
}

fn rig_events_from_stream_items<R>(items: Vec<StreamedAssistantContent<R>>) -> Vec<RigBackendEvent>
where
    R: Clone + std::marker::Unpin + rig_core::completion::GetTokenUsage,
{
    let mut mapper = RigStreamMapper::default();
    items
        .into_iter()
        .filter_map(|item| mapper.map_stream_item(item).unwrap())
        .collect()
}

#[derive(Clone, Debug)]
struct UsageResponse {
    usage: rig_core::completion::Usage,
}

impl UsageResponse {
    fn new(input_tokens: u64, output_tokens: u64, cached_input_tokens: u64) -> Self {
        Self {
            usage: rig_core::completion::Usage {
                input_tokens,
                output_tokens,
                total_tokens: input_tokens + output_tokens,
                cached_input_tokens,
                cache_creation_input_tokens: 0,
                reasoning_tokens: 0,
            },
        }
    }
}

impl rig_core::completion::GetTokenUsage for UsageResponse {
    fn token_usage(&self) -> Option<rig_core::completion::Usage> {
        Some(self.usage)
    }
}

struct RigStreamProvider {
    receiver:
        std::sync::Mutex<Option<async_channel::Receiver<Result<RigBackendEvent, ProviderError>>>>,
    capabilities: ModelCapabilities,
}

impl RigStreamProvider {
    fn queued(events: Vec<Result<RigBackendEvent, ProviderError>>) -> SharedProvider {
        let (sender, receiver) = async_channel::unbounded();
        for event in events {
            sender.try_send(event).unwrap();
        }
        drop(sender);
        Arc::new(Self {
            receiver: std::sync::Mutex::new(Some(receiver)),
            capabilities: ModelCapabilities::default(),
        })
    }

    fn controlled() -> (
        SharedProvider,
        async_channel::Sender<Result<RigBackendEvent, ProviderError>>,
    ) {
        let (sender, receiver) = async_channel::unbounded();
        (
            Arc::new(Self {
                receiver: std::sync::Mutex::new(Some(receiver)),
                capabilities: ModelCapabilities::default(),
            }),
            sender,
        )
    }
}

#[async_trait]
impl LlmProvider for RigStreamProvider {
    async fn chat(&self, _req: ChatRequest) -> Result<ChatResponse, ProviderError> {
        Err(ProviderError::UnsupportedModel(
            "RigStreamProvider only supports streaming tests".to_string(),
        ))
    }

    async fn chat_stream(&self, _req: ChatRequest) -> Result<ChatStream, ProviderError> {
        let receiver = self
            .receiver
            .lock()
            .unwrap()
            .take()
            .ok_or_else(|| ProviderError::StreamParse("stream already consumed".to_string()))?;
        let rig_stream: RigEventStream =
            Box::pin(futures::stream::unfold(receiver, |receiver| async move {
                receiver.recv().await.ok().map(|event| (event, receiver))
            }));
        Ok(super::rig_event_stream_to_chat_stream(rig_stream))
    }

    fn capabilities(&self) -> &ModelCapabilities {
        &self.capabilities
    }

    fn provider_kind(&self) -> ProviderKind {
        ProviderKind::OpenAI
    }

    fn with_base_url(self, _url: &str) -> Self {
        self
    }
}

#[derive(Default)]
struct FakeRigBackend {
    tool_call: Option<ToolCall>,
    final_text_after_tool_result: Option<String>,
    executed_tools: usize,
}

impl FakeRigBackend {
    fn new() -> Self {
        Self::default()
    }

    fn with_streamed_tool_call(mut self, id: &str, name: &str, input: &str) -> Self {
        self.tool_call = Some(ToolCall {
            id: id.to_string(),
            name: name.to_string(),
            input: serde_json::from_str(input).unwrap(),
        });
        self
    }

    fn with_final_text_after_tool_result(mut self, text: &str) -> Self {
        self.final_text_after_tool_result = Some(text.to_string());
        self
    }

    async fn collect_events_until_tool_call(&mut self) -> anyhow::Result<Vec<RigBackendEvent>> {
        let mut events = vec![RigBackendEvent::Start];
        if let Some(tool_call) = self.tool_call.clone() {
            events.push(RigBackendEvent::ToolCallReady(tool_call));
        }
        Ok(events)
    }

    async fn resume_with_tool_result(
        &mut self,
        tool_call_id: &str,
        _content: &str,
    ) -> anyhow::Result<Vec<RigBackendEvent>> {
        let Some(tool_call) = self.tool_call.as_ref() else {
            anyhow::bail!("no tool call is pending");
        };
        if tool_call.id != tool_call_id {
            anyhow::bail!("unknown tool call result id: {tool_call_id}");
        }

        Ok(self
            .final_text_after_tool_result
            .clone()
            .map(RigBackendEvent::TextChunk)
            .into_iter()
            .collect())
    }

    fn executed_tool_count(&self) -> usize {
        self.executed_tools
    }
}
