use std::collections::HashMap;
use std::sync::Arc;

use ai::provider::{ChatMessage, ContentBlock};

use crate::ai::agent::api::{DirectApiRouteConfig, RequestParams};
use crate::ai::agent::{
    AIAgentActionResult, AIAgentActionResultType, AIAgentContext, AIAgentInput, ReadFilesResult,
    TaskId, UserQueryMode,
};
use crate::ai::blocklist::SessionContext;
use crate::ai::execution_profiles::ModelRouting;
use crate::ai::llms::LLMId;
use warp_core::features::FeatureFlag;
use warp_multi_agent_api as api;

use super::get_supported_tools;

fn request_params_with_ask_user_question_enabled(ask_user_question_enabled: bool) -> RequestParams {
    let model = LLMId::from("test-model");

    RequestParams {
        input: vec![],
        conversation_token: None,
        forked_from_conversation_token: None,
        ambient_agent_task_id: None,
        tasks: vec![],
        existing_suggestions: None,
        metadata: None,
        session_context: SessionContext::new_for_test(),
        model: model.clone(),
        model_routing: ModelRouting::WarpProvider,
        direct_api_route_config: None,
        direct_api_route_error: None,
        coding_model: model.clone(),
        cli_agent_model: model.clone(),
        computer_use_model: model,
        is_memory_enabled: false,
        warp_drive_context_enabled: false,
        context_window_limit: None,
        mcp_context: None,
        planning_enabled: true,
        should_redact_secrets: false,
        api_keys: None,
        allow_use_of_warp_credits_with_byok: false,
        autonomy_level: api::AutonomyLevel::Supervised,
        isolation_level: api::IsolationLevel::None,
        web_search_enabled: false,
        computer_use_enabled: false,
        ask_user_question_enabled,
        research_agent_enabled: false,
        orchestration_enabled: false,
        supported_tools_override: None,
        parent_agent_id: None,
        agent_name: None,
    }
}

#[test]
fn direct_api_routing_requires_route_config() {
    let mut params = request_params_with_ask_user_question_enabled(false);
    params.model_routing = ModelRouting::DirectApi;
    params.direct_api_route_config = None;

    let err = super::super::direct::validate_direct_route(&params).unwrap_err();

    assert_eq!(
        err.to_string(),
        "Direct API routing is selected but no Direct API model is configured"
    );
}

#[test]
fn direct_api_routing_reports_route_resolution_error() {
    let mut params = request_params_with_ask_user_question_enabled(false);
    params.model_routing = ModelRouting::DirectApi;
    params.direct_api_route_config = None;
    params.direct_api_route_error = Some("Direct API provider OpenAI is disabled".to_string());

    let err = super::super::direct::validate_direct_route(&params).unwrap_err();

    assert_eq!(err.to_string(), "Direct API provider OpenAI is disabled");
}

#[test]
fn direct_api_routing_rejects_empty_model() {
    let mut params = request_params_with_ask_user_question_enabled(false);
    params.model_routing = ModelRouting::DirectApi;
    params.direct_api_route_config = Some(DirectApiRouteConfig {
        provider_id: ai::model_registry::ProviderId::Ollama,
        model_id: " ".to_string(),
        api_key: None,
        base_url: Some("http://localhost:11434".to_string()),
    });

    let err = super::super::direct::validate_direct_route(&params).unwrap_err();

    assert_eq!(
        err.to_string(),
        "Direct API routing is selected but the selected model is empty"
    );
}

#[test]
fn direct_api_routing_rejects_invalid_base_url() {
    let mut params = request_params_with_ask_user_question_enabled(false);
    params.model_routing = ModelRouting::DirectApi;
    params.direct_api_route_config = Some(DirectApiRouteConfig {
        provider_id: ai::model_registry::ProviderId::Custom,
        model_id: "custom-model".to_string(),
        api_key: None,
        base_url: Some("http://8.8.8.8:8080".to_string()),
    });

    let err = super::super::direct::validate_direct_route(&params).unwrap_err();

    assert!(err.to_string().contains("Invalid Direct API base URL"));
}

#[test]
fn direct_api_routing_requires_openrouter_base_url() {
    let mut params = request_params_with_ask_user_question_enabled(false);
    params.model_routing = ModelRouting::DirectApi;
    params.direct_api_route_config = Some(DirectApiRouteConfig {
        provider_id: ai::model_registry::ProviderId::OpenRouter,
        model_id: "openai/gpt-4o-mini".to_string(),
        api_key: Some("sk-or-test".to_string()),
        base_url: None,
    });

    let err = super::super::direct::validate_direct_route(&params).unwrap_err();

    assert_eq!(err.to_string(), "Direct API provider requires a base URL");
}

#[test]
fn warp_provider_routing_keeps_server_request_path() {
    let mut params = request_params_with_ask_user_question_enabled(false);
    params.model_routing = ModelRouting::WarpProvider;
    params.direct_api_route_config = None;

    assert!(!params.model_routing.is_direct_api());
}

#[test]
fn direct_api_initial_actions_create_fresh_task_and_stream_message() {
    let create_action = super::super::direct::create_task_action("task-local".to_string());
    let add_action = super::super::direct::add_initial_agent_output_action(
        "task-local".to_string(),
        "request-local".to_string(),
        "message-local".to_string(),
    );
    let append_action = super::super::direct::append_agent_output_chunk_action(
        "task-local".to_string(),
        "request-local".to_string(),
        "message-local".to_string(),
        "hello".to_string(),
    );

    match create_action {
        api::client_action::Action::CreateTask(create) => {
            let task = create.task.expect("fresh direct stream creates a task");
            assert_eq!(task.id, "task-local");
            assert!(task.messages.is_empty());
        }
        api::client_action::Action::BeginTransaction(_)
        | api::client_action::Action::CommitTransaction(_)
        | api::client_action::Action::RollbackTransaction(_)
        | api::client_action::Action::UpdateTaskDescription(_)
        | api::client_action::Action::AddMessagesToTask(_)
        | api::client_action::Action::ShowSuggestions(_)
        | api::client_action::Action::UpdateTaskSummary(_)
        | api::client_action::Action::StartNewConversation(_)
        | api::client_action::Action::UpdateTaskServerData(_)
        | api::client_action::Action::MoveMessagesToNewTask(_)
        | api::client_action::Action::UpdateTaskMessage(_)
        | api::client_action::Action::AppendToMessageContent(_) => {
            panic!("expected CreateTask action")
        }
    }

    match add_action {
        api::client_action::Action::AddMessagesToTask(add) => {
            assert_eq!(add.task_id, "task-local");
            assert_eq!(add.messages.len(), 1);
        }
        api::client_action::Action::BeginTransaction(_)
        | api::client_action::Action::CommitTransaction(_)
        | api::client_action::Action::RollbackTransaction(_)
        | api::client_action::Action::CreateTask(_)
        | api::client_action::Action::UpdateTaskDescription(_)
        | api::client_action::Action::ShowSuggestions(_)
        | api::client_action::Action::UpdateTaskSummary(_)
        | api::client_action::Action::StartNewConversation(_)
        | api::client_action::Action::UpdateTaskServerData(_)
        | api::client_action::Action::MoveMessagesToNewTask(_)
        | api::client_action::Action::UpdateTaskMessage(_)
        | api::client_action::Action::AppendToMessageContent(_) => {
            panic!("expected AddMessagesToTask action")
        }
    }

    match append_action {
        api::client_action::Action::AppendToMessageContent(append) => {
            assert_eq!(append.task_id, "task-local");
            assert_eq!(
                append.mask.expect("append mask").paths,
                vec!["message.agent_output.text".to_string()]
            );
        }
        api::client_action::Action::BeginTransaction(_)
        | api::client_action::Action::CommitTransaction(_)
        | api::client_action::Action::RollbackTransaction(_)
        | api::client_action::Action::CreateTask(_)
        | api::client_action::Action::UpdateTaskDescription(_)
        | api::client_action::Action::AddMessagesToTask(_)
        | api::client_action::Action::ShowSuggestions(_)
        | api::client_action::Action::UpdateTaskSummary(_)
        | api::client_action::Action::StartNewConversation(_)
        | api::client_action::Action::UpdateTaskServerData(_)
        | api::client_action::Action::MoveMessagesToNewTask(_)
        | api::client_action::Action::UpdateTaskMessage(_) => {
            panic!("expected AppendToMessageContent action")
        }
    }
}

#[test]
fn direct_api_chat_request_uses_user_query_text() {
    let mut params = request_params_with_ask_user_question_enabled(false);
    params.input = vec![AIAgentInput::UserQuery {
        query: "explain the failing test".to_string(),
        context: Arc::<[AIAgentContext]>::from([]),
        static_query_type: None,
        referenced_attachments: HashMap::new(),
        user_query_mode: UserQueryMode::Normal,
        running_command: None,
        intended_agent: None,
    }];

    let request = super::super::direct_tools::build_chat_request(&params);

    let Some(ChatMessage::User(blocks)) = request.messages.last() else {
        panic!("expected final user message");
    };
    let Some(ContentBlock::Text(text)) = blocks.first() else {
        panic!("expected text block");
    };
    assert_eq!(text, "explain the failing test");
}

#[test]
fn direct_api_chat_request_preserves_persisted_tool_call_and_result() {
    let mut params = request_params_with_ask_user_question_enabled(false);
    params.tasks = vec![api::Task {
        id: "task-local".to_string(),
        messages: vec![
            api::Message {
                id: "message-tool-call".to_string(),
                task_id: "task-local".to_string(),
                request_id: "request-local".to_string(),
                timestamp: None,
                server_message_data: String::new(),
                citations: Vec::new(),
                message: Some(api::message::Message::ToolCall(api::message::ToolCall {
                    tool_call_id: "call-read".to_string(),
                    tool: Some(api::message::tool_call::Tool::ReadFiles(
                        api::message::tool_call::ReadFiles {
                            files: vec![api::message::tool_call::read_files::File {
                                name: "Cargo.toml".to_string(),
                                line_ranges: Vec::new(),
                            }],
                        },
                    )),
                })),
            },
            api::Message {
                id: "message-tool-result".to_string(),
                task_id: "task-local".to_string(),
                request_id: "request-local".to_string(),
                timestamp: None,
                server_message_data: String::new(),
                citations: Vec::new(),
                message: Some(api::message::Message::ToolCallResult(
                    api::message::ToolCallResult {
                        tool_call_id: "call-read".to_string(),
                        context: None,
                        result: Some(api::message::tool_call_result::Result::ReadFiles(
                            api::ReadFilesResult {
                                result: Some(api::read_files_result::Result::Error(
                                    api::read_files_result::Error {
                                        message: "permission denied".to_string(),
                                    },
                                )),
                            },
                        )),
                    },
                )),
            },
        ],
        dependencies: None,
        description: String::new(),
        summary: String::new(),
        server_data: String::new(),
    }];

    let request = super::super::direct_tools::build_chat_request(&params);

    let Some(ChatMessage::Assistant { text, tool_calls }) = request.messages.first() else {
        panic!("expected assistant tool-call message");
    };
    assert!(text.is_none());
    assert_eq!(tool_calls.len(), 1);
    assert_eq!(tool_calls[0].id, "call-read");
    assert_eq!(tool_calls[0].name, "ReadFiles");
    assert_eq!(tool_calls[0].input["files"][0]["name"], "Cargo.toml");

    let Some(ChatMessage::User(blocks)) = request.messages.get(1) else {
        panic!("expected user tool-result message");
    };
    let Some(ContentBlock::ToolResult {
        tool_use_id,
        content,
        is_error,
    }) = blocks.first()
    else {
        panic!("expected structured tool result");
    };
    assert_eq!(tool_use_id, "call-read");
    assert!(matches!(content, ai::provider::ToolResultContent::Text(_)));
    assert!(*is_error);
}

#[test]
fn direct_api_chat_request_marks_successful_shell_result_as_non_error() {
    let mut params = request_params_with_ask_user_question_enabled(false);
    params.tasks = vec![api::Task {
        id: "task-local".to_string(),
        messages: vec![api::Message {
            id: "message-tool-result".to_string(),
            task_id: "task-local".to_string(),
            request_id: "request-local".to_string(),
            timestamp: None,
            server_message_data: String::new(),
            citations: Vec::new(),
            message: Some(api::message::Message::ToolCallResult(
                api::message::ToolCallResult {
                    tool_call_id: "call-shell".to_string(),
                    context: None,
                    result: Some(api::message::tool_call_result::Result::RunShellCommand(
                        #[allow(deprecated)]
                        api::RunShellCommandResult {
                            command: "echo ok".to_string(),
                            output: String::new(),
                            exit_code: 0,
                            result: Some(api::run_shell_command_result::Result::CommandFinished(
                                api::ShellCommandFinished {
                                    command_id: "command-local".to_string(),
                                    output: "ok".to_string(),
                                    exit_code: 0,
                                },
                            )),
                        },
                    )),
                },
            )),
        }],
        dependencies: None,
        description: String::new(),
        summary: String::new(),
        server_data: String::new(),
    }];

    let request = super::super::direct_tools::build_chat_request(&params);

    let Some(ChatMessage::User(blocks)) = request.messages.first() else {
        panic!("expected user tool-result message");
    };
    let Some(ContentBlock::ToolResult { is_error, .. }) = blocks.first() else {
        panic!("expected structured tool result");
    };
    assert!(!*is_error);
}

#[test]
fn direct_api_chat_request_preserves_current_action_result_as_tool_result() {
    let mut params = request_params_with_ask_user_question_enabled(false);
    params.input = vec![AIAgentInput::ActionResult {
        result: AIAgentActionResult {
            id: "call-read".to_string().into(),
            task_id: TaskId::new("task-local".to_string()),
            result: AIAgentActionResultType::ReadFiles(ReadFilesResult::Error(
                "missing file".to_string(),
            )),
        },
        context: Arc::<[AIAgentContext]>::from([]),
    }];

    let request = super::super::direct_tools::build_chat_request(&params);

    let Some(ChatMessage::User(blocks)) = request.messages.first() else {
        panic!("expected user tool-result message");
    };
    let Some(ContentBlock::ToolResult {
        tool_use_id,
        content,
        is_error,
    }) = blocks.first()
    else {
        panic!("expected structured tool result");
    };
    assert_eq!(tool_use_id, "call-read");
    assert!(matches!(content, ai::provider::ToolResultContent::Text(_)));
    assert!(*is_error);
}

#[test]
fn maps_direct_read_files_tool_call_to_proto_tool_call() {
    let tool_call = ai::provider::ToolCall {
        id: "call-read".to_string(),
        name: "ReadFiles".to_string(),
        input: serde_json::json!({
            "files": [{"name": "Cargo.toml"}]
        }),
    };

    let proto =
        super::super::direct_tools::provider_tool_call_to_proto(tool_call).expect("tool maps");

    assert_eq!(proto.tool_call_id, "call-read");
    assert!(matches!(
        proto.tool,
        Some(api::message::tool_call::Tool::ReadFiles(_))
    ));
}

#[test]
fn direct_read_files_tool_call_rejects_missing_files() {
    let tool_call = ai::provider::ToolCall {
        id: "call-read".to_string(),
        name: "ReadFiles".to_string(),
        input: serde_json::json!({}),
    };

    let err = super::super::direct_tools::provider_tool_call_to_proto(tool_call).unwrap_err();

    assert!(err
        .to_string()
        .contains("Invalid Direct API ReadFiles input"));
}

#[test]
fn direct_grep_tool_call_rejects_empty_queries() {
    let tool_call = ai::provider::ToolCall {
        id: "call-grep".to_string(),
        name: "Grep".to_string(),
        input: serde_json::json!({"queries": []}),
    };

    let err = super::super::direct_tools::provider_tool_call_to_proto(tool_call).unwrap_err();

    assert!(err
        .to_string()
        .contains("Direct API Grep input requires at least one query"));
}

#[test]
fn direct_run_shell_command_tool_call_rejects_empty_command() {
    let tool_call = ai::provider::ToolCall {
        id: "call-shell".to_string(),
        name: "RunShellCommand".to_string(),
        input: serde_json::json!({"command": " "}),
    };

    let err = super::super::direct_tools::provider_tool_call_to_proto(tool_call).unwrap_err();

    assert!(err
        .to_string()
        .contains("Direct API RunShellCommand input requires a non-empty command"));
}

#[test]
fn maps_direct_tool_call_to_add_messages_action() {
    let tool_call = api::message::ToolCall {
        tool_call_id: "call-read".to_string(),
        tool: Some(api::message::tool_call::Tool::ReadFiles(
            api::message::tool_call::ReadFiles { files: Vec::new() },
        )),
    };

    let action = super::super::direct::add_tool_call_action(
        "task-local".to_string(),
        "request-local".to_string(),
        "message-tool".to_string(),
        tool_call,
    );

    match action {
        api::client_action::Action::AddMessagesToTask(add) => {
            assert_eq!(add.task_id, "task-local");
            assert_eq!(add.messages.len(), 1);
            assert!(matches!(
                add.messages[0].message,
                Some(api::message::Message::ToolCall(_))
            ));
        }
        api::client_action::Action::BeginTransaction(_)
        | api::client_action::Action::CommitTransaction(_)
        | api::client_action::Action::RollbackTransaction(_)
        | api::client_action::Action::CreateTask(_)
        | api::client_action::Action::UpdateTaskDescription(_)
        | api::client_action::Action::ShowSuggestions(_)
        | api::client_action::Action::UpdateTaskSummary(_)
        | api::client_action::Action::StartNewConversation(_)
        | api::client_action::Action::UpdateTaskServerData(_)
        | api::client_action::Action::MoveMessagesToNewTask(_)
        | api::client_action::Action::UpdateTaskMessage(_)
        | api::client_action::Action::AppendToMessageContent(_) => {
            panic!("expected AddMessagesToTask action")
        }
    }
}

#[test]
fn unknown_direct_tool_call_maps_to_error() {
    let tool_call = ai::provider::ToolCall {
        id: "call-unknown".to_string(),
        name: "UnknownTool".to_string(),
        input: serde_json::json!({}),
    };

    let err = super::super::direct_tools::provider_tool_call_to_proto(tool_call).unwrap_err();

    assert!(err.to_string().contains("Unsupported Direct API tool"));
}

#[test]
fn supported_tools_omits_ask_user_question_when_disabled() {
    let params = request_params_with_ask_user_question_enabled(false);
    let supported_tools = get_supported_tools(&params);

    assert!(!supported_tools.contains(&api::ToolType::AskUserQuestion));
}

#[test]
fn supported_tools_includes_ask_user_question_when_enabled_and_feature_flag_is_enabled() {
    if !FeatureFlag::AskUserQuestion.is_enabled() {
        return;
    }

    let params = request_params_with_ask_user_question_enabled(true);
    let supported_tools = get_supported_tools(&params);

    assert!(supported_tools.contains(&api::ToolType::AskUserQuestion));
}

#[test]
fn supported_tools_include_upload_artifact_when_feature_flag_is_enabled() {
    let _flag = FeatureFlag::ArtifactCommand.override_enabled(true);
    let params = request_params_with_ask_user_question_enabled(false);
    let supported_tools = get_supported_tools(&params);

    assert!(supported_tools.contains(&api::ToolType::UploadFileArtifact));
}

#[test]
fn supported_tools_omit_upload_artifact_when_feature_flag_is_disabled() {
    let _flag = FeatureFlag::ArtifactCommand.override_enabled(false);
    let params = request_params_with_ask_user_question_enabled(false);
    let supported_tools = get_supported_tools(&params);

    assert!(!supported_tools.contains(&api::ToolType::UploadFileArtifact));
}
