use ai::model_registry::ProviderId;
use ai::provider::{
    ChatMessage, ChatOptions, ChatRequest, ChatStream, ContentBlock, GenaiAdapter, LlmProvider,
};
use std::fmt::Debug;
use warp_multi_agent_api as api;

use super::{DirectApiRouteConfig, RequestParams};
use crate::ai::agent::{AIAgentContext, AIAgentInput};

pub async fn run_provider_stream(params: RequestParams) -> anyhow::Result<ChatStream> {
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

    let user_prompt = render_inputs_for_provider(&params.input);
    if !user_prompt.trim().is_empty() {
        messages.push(ChatMessage::User(vec![ContentBlock::Text(user_prompt)]));
    }

    if messages.is_empty() {
        messages.push(ChatMessage::User(vec![ContentBlock::Text(
            "Continue.".to_string(),
        )]));
    }

    ChatRequest {
        messages,
        tools: Vec::new(),
        options: ChatOptions::default(),
    }
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
                Some(ChatMessage::User(vec![ContentBlock::Text(format!(
                    "Tool result: {}",
                    render_tool_result(result)
                ))]))
            }
            api::message::Message::ToolCall(_)
            | api::message::Message::ServerEvent(_)
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

fn render_inputs_for_provider(inputs: &[AIAgentInput]) -> String {
    inputs
        .iter()
        .filter_map(render_input_for_provider)
        .collect::<Vec<_>>()
        .join("\n\n")
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
