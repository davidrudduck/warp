use ai::model_registry::ProviderId;
use ai::provider::{
    ChatMessage, ChatOptions, ChatRequest, ChatStream, ContentBlock, GenaiAdapter, LlmProvider,
    Tool, ToolCall, ToolResultContent,
};
use anyhow::{bail, Context as _};
use serde::Deserialize;
use std::fmt::Debug;
use warp_multi_agent_api as api;

use super::{DirectApiRouteConfig, RequestParams};
use crate::ai::agent::{
    AIAgentActionResult, AIAgentActionResultType, AIAgentContext, AIAgentInput, GrepResult,
    ReadFilesResult,
};
use crate::ai::execution_profiles::DirectApiAgentBackend;

pub async fn run_provider_stream(params: RequestParams) -> anyhow::Result<ChatStream> {
    match select_direct_api_stream_backend(&params) {
        DirectApiStreamBackend::NativeGenai => run_native_provider_stream(params).await,
        DirectApiStreamBackend::RigAgent => {
            super::rig_direct::run_rig_provider_stream(params).await
        }
    }
}

async fn run_native_provider_stream(params: RequestParams) -> anyhow::Result<ChatStream> {
    let config = params
        .direct_api_route_config
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Direct API route config missing"))?;
    let request = build_chat_request(&params);
    provider_for_config(config)
        .chat_stream(request)
        .await
        .map_err(Into::into)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectApiStreamBackend {
    NativeGenai,
    #[cfg_attr(not(feature = "direct_api_rig_backend"), allow(dead_code))]
    RigAgent,
}

pub fn select_direct_api_stream_backend(params: &RequestParams) -> DirectApiStreamBackend {
    match params.direct_api_agent_backend.effective() {
        DirectApiAgentBackend::Native | DirectApiAgentBackend::Unknown => {
            DirectApiStreamBackend::NativeGenai
        }
        DirectApiAgentBackend::RigAgent => {
            #[cfg(feature = "direct_api_rig_backend")]
            {
                DirectApiStreamBackend::RigAgent
            }
            #[cfg(not(feature = "direct_api_rig_backend"))]
            {
                DirectApiStreamBackend::NativeGenai
            }
        }
    }
}

fn provider_for_config(config: &DirectApiRouteConfig) -> GenaiAdapter {
    let provider_name = provider_name(config.provider_id);
    let api_key = config.api_key.as_deref().unwrap_or_default();
    let adapter = GenaiAdapter::new(provider_name, api_key, &config.model_id);
    if let Some(base_url) = config.base_url.as_deref() {
        adapter.with_base_url(base_url)
    } else {
        adapter
    }
}

pub(super) fn build_chat_request(params: &RequestParams) -> ChatRequest {
    let mut messages = Vec::new();

    for task in &params.tasks {
        messages.extend(chat_messages_from_task_messages(&task.messages));
    }

    messages.extend(chat_messages_from_inputs(&params.input));

    if messages.is_empty() {
        messages.push(ChatMessage::User(vec![ContentBlock::Text(
            "Continue.".to_string(),
        )]));
    }

    ChatRequest {
        messages,
        tools: direct_tool_definitions_for_supported_tools(
            params.supported_tools_override.as_deref(),
        ),
        options: ChatOptions::default(),
    }
}

pub fn provider_tool_call_to_proto(tool_call: ToolCall) -> anyhow::Result<api::message::ToolCall> {
    let ToolCall { id, name, input } = tool_call;
    let tool = match name.as_str() {
        "ReadFiles" => {
            let input: ReadFilesToolInput =
                serde_json::from_value(input).context("Invalid Direct API ReadFiles input")?;
            if input.files.is_empty() {
                bail!("Direct API ReadFiles input requires at least one file");
            }
            let files = input
                .files
                .into_iter()
                .map(|file| {
                    if file.name.trim().is_empty() {
                        bail!("Direct API ReadFiles input contains an empty file name");
                    }
                    Ok(api::message::tool_call::read_files::File {
                        name: file.name,
                        line_ranges: Vec::new(),
                    })
                })
                .collect::<anyhow::Result<Vec<_>>>()?;
            api::message::tool_call::Tool::ReadFiles(api::message::tool_call::ReadFiles { files })
        }
        "Grep" => {
            let input: GrepToolInput =
                serde_json::from_value(input).context("Invalid Direct API Grep input")?;
            if input.queries.is_empty() {
                bail!("Direct API Grep input requires at least one query");
            }
            if input.queries.iter().any(|query| query.trim().is_empty()) {
                bail!("Direct API Grep input contains an empty query");
            }
            let path = input.path.unwrap_or_default();
            api::message::tool_call::Tool::Grep(api::message::tool_call::Grep {
                queries: input.queries,
                path,
            })
        }
        "RunShellCommand" => {
            let input: RunShellCommandToolInput = serde_json::from_value(input)
                .context("Invalid Direct API RunShellCommand input")?;
            if input.command.trim().is_empty() {
                bail!("Direct API RunShellCommand input requires a non-empty command");
            }
            api::message::tool_call::Tool::RunShellCommand(
                api::message::tool_call::RunShellCommand {
                    command: input.command,
                    is_read_only: false,
                    uses_pager: false,
                    citations: Vec::new(),
                    is_risky: true,
                    risk_category: 0,
                    wait_until_complete_value: None,
                },
            )
        }
        other => bail!("Unsupported Direct API tool: {other}"),
    };

    Ok(api::message::ToolCall {
        tool_call_id: id,
        tool: Some(tool),
    })
}

#[derive(Deserialize)]
struct ReadFilesToolInput {
    files: Vec<ReadFilesToolInputFile>,
}

#[derive(Deserialize)]
struct ReadFilesToolInputFile {
    name: String,
}

#[derive(Deserialize)]
struct GrepToolInput {
    queries: Vec<String>,
    path: Option<String>,
}

#[derive(Deserialize)]
struct RunShellCommandToolInput {
    command: String,
}

pub(super) fn direct_tool_definitions() -> Vec<Tool> {
    vec![
        Tool {
            name: "ReadFiles".to_string(),
            description: "Read one or more files from the current workspace.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "files": {
                        "type": "array",
                        "minItems": 1,
                        "items": {
                            "type": "object",
                            "properties": {
                                "name": {"type": "string", "minLength": 1}
                            },
                            "required": ["name"]
                        }
                    }
                },
                "required": ["files"]
            }),
        },
        Tool {
            name: "Grep".to_string(),
            description: "Search files for text or regex patterns.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "queries": {"type": "array", "minItems": 1, "items": {"type": "string", "minLength": 1}},
                    "path": {"type": "string"}
                },
                "required": ["queries"]
            }),
        },
        Tool {
            name: "RunShellCommand".to_string(),
            description: "Request execution of a shell command after Warp permission checks."
                .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {"type": "string", "minLength": 1}
                },
                "required": ["command"]
            }),
        },
    ]
}

fn direct_tool_definitions_for_supported_tools(
    supported_tools: Option<&[api::ToolType]>,
) -> Vec<Tool> {
    let tools = direct_tool_definitions();
    let Some(supported_tools) = supported_tools else {
        return tools;
    };

    tools
        .into_iter()
        .filter(|tool| direct_tool_is_supported(tool.name.as_str(), supported_tools))
        .collect()
}

fn direct_tool_is_supported(tool_name: &str, supported_tools: &[api::ToolType]) -> bool {
    if tool_name == "ReadFiles" {
        return supported_tools.contains(&api::ToolType::ReadFiles);
    }
    if tool_name == "Grep" {
        return supported_tools.contains(&api::ToolType::Grep);
    }
    if tool_name == "RunShellCommand" {
        return supported_tools.contains(&api::ToolType::RunShellCommand);
    }
    false
}

fn chat_messages_from_task_messages(messages: &[api::Message]) -> Vec<ChatMessage> {
    messages
        .iter()
        .filter_map(|message| match message.message.as_ref()? {
            api::message::Message::UserQuery(query) => {
                Some(ChatMessage::User(vec![ContentBlock::Text(
                    render_user_query(&query.query, query.context.as_ref()),
                )]))
            }
            api::message::Message::SystemQuery(query) => {
                Some(ChatMessage::User(vec![ContentBlock::Text(
                    render_system_query(query),
                )]))
            }
            api::message::Message::AgentOutput(output) => Some(ChatMessage::Assistant {
                text: Some(output.text.clone()),
                tool_calls: Vec::new(),
            }),
            api::message::Message::ToolCallResult(result) => {
                Some(chat_message_from_tool_call_result(result))
            }
            api::message::Message::ToolCall(tool_call) => provider_tool_call_from_proto(tool_call)
                .map(|tool_call| ChatMessage::Assistant {
                    text: None,
                    tool_calls: vec![tool_call],
                }),
            api::message::Message::ServerEvent(_)
            | api::message::Message::AgentReasoning(_)
            | api::message::Message::UpdateTodos(_)
            | api::message::Message::Summarization(_)
            | api::message::Message::CodeReview(_)
            | api::message::Message::WebSearch(_)
            | api::message::Message::WebFetch(_)
            | api::message::Message::UpdateReviewComments(_)
            | api::message::Message::DebugOutput(_)
            | api::message::Message::ArtifactEvent(_)
            | api::message::Message::InvokeSkill(_)
            | api::message::Message::MessagesReceivedFromAgents(_)
            | api::message::Message::EventsFromAgents(_)
            | api::message::Message::PassiveSuggestionResult(_)
            | api::message::Message::OrchestrationConfigSnapshot(_)
            | api::message::Message::ModelUsed(_) => None,
        })
        .collect()
}

fn provider_tool_call_from_proto(tool_call: &api::message::ToolCall) -> Option<ToolCall> {
    if let Some(api::message::tool_call::Tool::ReadFiles(read_files)) = tool_call.tool.as_ref() {
        return Some(ToolCall {
            id: tool_call.tool_call_id.clone(),
            name: "ReadFiles".to_string(),
            input: serde_json::json!({
                "files": read_files
                    .files
                    .iter()
                    .map(|file| serde_json::json!({"name": file.name.clone()}))
                    .collect::<Vec<_>>()
            }),
        });
    }

    if let Some(api::message::tool_call::Tool::Grep(grep)) = tool_call.tool.as_ref() {
        return Some(ToolCall {
            id: tool_call.tool_call_id.clone(),
            name: "Grep".to_string(),
            input: serde_json::json!({
                "queries": grep.queries.clone(),
                "path": grep.path.clone()
            }),
        });
    }

    if let Some(api::message::tool_call::Tool::RunShellCommand(command)) = tool_call.tool.as_ref() {
        return Some(ToolCall {
            id: tool_call.tool_call_id.clone(),
            name: "RunShellCommand".to_string(),
            input: serde_json::json!({
                "command": command.command.clone()
            }),
        });
    }

    None
}

fn chat_message_from_tool_call_result(result: &api::message::ToolCallResult) -> ChatMessage {
    ChatMessage::User(vec![ContentBlock::ToolResult {
        tool_use_id: result.tool_call_id.clone(),
        content: ToolResultContent::Text(render_tool_result(result)),
        is_error: proto_tool_call_result_is_error(result),
    }])
}

fn chat_messages_from_inputs(inputs: &[AIAgentInput]) -> Vec<ChatMessage> {
    let mut messages = Vec::new();
    let mut text_sections = Vec::new();

    for input in inputs {
        if let Some(action_result) = input.action_result() {
            if !text_sections.is_empty() {
                messages.push(ChatMessage::User(vec![ContentBlock::Text(
                    text_sections.join("\n\n"),
                )]));
                text_sections.clear();
            }

            messages.push(ChatMessage::User(vec![ContentBlock::ToolResult {
                tool_use_id: action_result.id.to_string(),
                content: ToolResultContent::Text(action_result.to_string()),
                is_error: action_result_is_error(action_result),
            }]));

            if let Some(rendered) = render_input_context_and_attachments_for_provider(input) {
                text_sections.push(rendered);
            }
        } else if let Some(rendered) = render_input_for_provider(input) {
            text_sections.push(rendered);
        }
    }

    if !text_sections.is_empty() {
        messages.push(ChatMessage::User(vec![ContentBlock::Text(
            text_sections.join("\n\n"),
        )]));
    }

    messages
}

fn action_result_is_error(action_result: &AIAgentActionResult) -> bool {
    if action_result.is_rejected() {
        return true;
    }

    if let AIAgentActionResultType::ReadFiles(result) = &action_result.result {
        return read_files_action_result_is_error(result);
    }

    if let AIAgentActionResultType::Grep(result) = &action_result.result {
        return grep_action_result_is_error(result);
    }

    if let AIAgentActionResultType::RequestCommandOutput(result) = &action_result.result {
        return result.failed();
    }

    false
}

fn read_files_action_result_is_error(result: &ReadFilesResult) -> bool {
    matches!(
        result,
        ReadFilesResult::Error(_) | ReadFilesResult::Cancelled
    )
}

fn grep_action_result_is_error(result: &GrepResult) -> bool {
    matches!(result, GrepResult::Error(_) | GrepResult::Cancelled)
}

fn proto_tool_call_result_is_error(result: &api::message::ToolCallResult) -> bool {
    let Some(result) = result.result.as_ref() else {
        return true;
    };

    if let api::message::tool_call_result::Result::Cancel(()) = result {
        return true;
    }

    if let api::message::tool_call_result::Result::ReadFiles(read_files) = result {
        return read_files_proto_result_is_error(read_files);
    }

    if let api::message::tool_call_result::Result::Grep(grep) = result {
        return grep_proto_result_is_error(grep);
    }

    if let api::message::tool_call_result::Result::RunShellCommand(command) = result {
        return run_shell_command_proto_result_is_error(command);
    }

    true
}

fn read_files_proto_result_is_error(result: &api::ReadFilesResult) -> bool {
    if result.result.is_none() {
        return true;
    }

    if let Some(api::read_files_result::Result::Error(..)) = result.result.as_ref() {
        return true;
    }

    false
}

fn grep_proto_result_is_error(result: &api::GrepResult) -> bool {
    if result.result.is_none() {
        return true;
    }

    if let Some(api::grep_result::Result::Error(..)) = result.result.as_ref() {
        return true;
    }

    false
}

fn run_shell_command_proto_result_is_error(result: &api::RunShellCommandResult) -> bool {
    let Some(result) = result.result.as_ref() else {
        return true;
    };

    if let api::run_shell_command_result::Result::PermissionDenied(..) = result {
        return true;
    }

    if let api::run_shell_command_result::Result::CommandFinished(finished) = result {
        return finished.exit_code != 0;
    }

    false
}

fn render_input_for_provider(input: &AIAgentInput) -> Option<String> {
    let mut sections = Vec::new();
    if let Some(query) = input.user_query() {
        sections.push(query);
    } else if let Some(action_result) = input.action_result() {
        sections.push(format!("Tool result: {action_result}"));
    } else if let Some(query) = input.auto_code_diff_query() {
        sections.push(query.to_string());
    }

    if let Some(context) = input.context() {
        let rendered_context = render_context(context);
        if !rendered_context.is_empty() {
            sections.push(rendered_context);
        }
    }

    if let Some(attachments) = input.attachments() {
        let rendered_attachments = attachments
            .into_iter()
            .map(|attachment| format!("{attachment:?}"))
            .collect::<Vec<_>>()
            .join("\n");
        if !rendered_attachments.is_empty() {
            sections.push(format!("Attachments:\n{rendered_attachments}"));
        }
    }

    (!sections.is_empty()).then(|| sections.join("\n\n"))
}

fn render_input_context_and_attachments_for_provider(input: &AIAgentInput) -> Option<String> {
    let mut sections = Vec::new();
    if let Some(context) = input.context() {
        let rendered_context = render_context(context);
        if !rendered_context.is_empty() {
            sections.push(rendered_context);
        }
    }

    if let Some(attachments) = input.attachments() {
        let rendered_attachments = attachments
            .into_iter()
            .map(|attachment| format!("{attachment:?}"))
            .collect::<Vec<_>>()
            .join("\n");
        if !rendered_attachments.is_empty() {
            sections.push(format!("Attachments:\n{rendered_attachments}"));
        }
    }

    (!sections.is_empty()).then(|| sections.join("\n\n"))
}

fn render_user_query<C: Debug>(query: &str, context: Option<&C>) -> String {
    let mut sections = vec![query.to_string()];
    if let Some(context) = context {
        sections.push(format!("Context:\n{context:?}"));
    }
    sections.join("\n\n")
}

fn render_system_query(query: &api::message::SystemQuery) -> String {
    let label = match query.r#type.as_ref() {
        Some(api::message::system_query::Type::CreateNewProject(project)) => {
            format!("Create new project: {}", project.query)
        }
        Some(api::message::system_query::Type::CloneRepository(repo)) => {
            format!("Clone repository: {}", repo.url)
        }
        Some(api::message::system_query::Type::AutoCodeDiff(diff)) => diff.query.clone(),
        Some(api::message::system_query::Type::FetchReviewComments(comments)) => {
            format!("Fetch review comments for {}", comments.repo_path)
        }
        Some(api::message::system_query::Type::SummarizeConversation(summary)) => {
            summary.prompt.clone()
        }
        Some(api::message::system_query::Type::ResumeConversation(_)) => {
            "Resume conversation".to_string()
        }
        Some(api::message::system_query::Type::GeneratePassiveSuggestions(_)) => {
            "Generate passive suggestions".to_string()
        }
        None => "System query".to_string(),
    };
    render_user_query(&label, query.context.as_ref())
}

fn render_tool_result(result: &api::message::ToolCallResult) -> String {
    format!("{:?}", result.result)
}

fn render_context(context: &[AIAgentContext]) -> String {
    let sections = context
        .iter()
        .map(render_context_item)
        .collect::<Vec<_>>()
        .join("\n");
    if sections.is_empty() {
        String::new()
    } else {
        format!("Context:\n{sections}")
    }
}

fn render_context_item(context: &AIAgentContext) -> String {
    match context {
        AIAgentContext::Directory { pwd, home_dir, .. } => {
            format!(
                "Directory: pwd={}, home={}",
                pwd.as_deref().unwrap_or("unknown"),
                home_dir.as_deref().unwrap_or("unknown")
            )
        }
        AIAgentContext::SelectedText(text) => format!("Selected text:\n{text}"),
        AIAgentContext::ExecutionEnvironment(environment) => {
            format!("Execution environment: {environment:?}")
        }
        AIAgentContext::CurrentTime { current_time } => {
            format!("Current time: {}", current_time.to_rfc3339())
        }
        AIAgentContext::Image(image) => format!("Image attachment: {}", image.file_name),
        AIAgentContext::Codebase { path, name } => format!("Codebase: {name} at {path}"),
        AIAgentContext::ProjectRules {
            root_path,
            active_rules,
            additional_rule_paths,
        } => format!(
            "Project rules: root={}, active={}, additional={}",
            root_path,
            active_rules
                .iter()
                .map(|rule| rule.file_name.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            additional_rule_paths.join(", ")
        ),
        AIAgentContext::File(file) => {
            format!("File context: {}\n{:?}", file.file_name, file.content)
        }
        AIAgentContext::Git { head, branch } => {
            format!(
                "Git: head={}, branch={}",
                head,
                branch.as_deref().unwrap_or("")
            )
        }
        AIAgentContext::Skills { skills } => format!(
            "Available skills: {}",
            skills
                .iter()
                .map(|skill| skill.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ),
        AIAgentContext::Block(block) => {
            format!(
                "Terminal block: command=`{}` exit_code={}\n{}",
                block.command, block.exit_code, block.output
            )
        }
    }
}

fn provider_name(provider_id: ProviderId) -> &'static str {
    match provider_id {
        ProviderId::OpenAI => "openai",
        ProviderId::Anthropic => "anthropic",
        ProviderId::GoogleGemini => "gemini",
        ProviderId::Ollama => "ollama",
        ProviderId::OpenRouter => "openrouter",
        ProviderId::Custom => "custom",
    }
}
