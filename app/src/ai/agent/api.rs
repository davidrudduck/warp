pub(crate) mod convert_conversation;
mod convert_from;
mod convert_to;
mod direct;
mod direct_tools;
mod r#impl;
mod rig_direct;

pub use ai::agent::convert::ConvertToAPITypeError;
use ai::api_keys::{ApiKeyManager, ApiKeys};
use ai::model_registry::ProviderId;
use ai::url_validation::normalize_direct_api_base_url;
pub use convert_from::{
    user_inputs_from_messages, ConversionParams, ConvertAPIMessageToClientOutputMessage,
    MaybeAIAgentOutputMessage, MessageToAIAgentOutputMessageError,
};

pub use r#impl::generate_multi_agent_output;

use futures_lite::Stream;
use serde::Serialize;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use warp_core::channel::ChannelState;
use warp_core::execution_mode::AppExecutionMode;
use warp_core::features::FeatureFlag;
use warp_core::settings::DirectAPISettings;

use crate::ai::agent::conversation::AIConversationId;
use crate::ai::ambient_agents::AmbientAgentTaskId;
use crate::{
    ai::{blocklist::SessionContext, llms::LLMId},
    server::server_api::AIApiError,
};

use super::{AIAgentInput, MCPContext, MCPServer, RequestMetadata, Suggestions};
use crate::ai::blocklist::{BlocklistAIPermissions, RequestInput};
use crate::ai::execution_profiles::profiles::AIExecutionProfilesModel;
use crate::ai::execution_profiles::{
    DirectApiAgentBackend, DirectApiProfileModelSelection, ModelRouting,
};
use crate::ai::mcp::templatable_manager::TemplatableMCPServerInfo;
use crate::ai::mcp::TemplatableMCPServerManager;
use crate::settings::AISettings;
use crate::terminal::safe_mode_settings::get_secret_obfuscation_mode;
use crate::workspaces::user_workspaces::UserWorkspaces;
use warp_core::user_preferences::GetUserPreferences;
use warpui::{AppContext, EntityId, SingletonEntity as _};

const OPENROUTER_DEFAULT_BASE_URL: &str = "https://openrouter.ai/api/v1";

/// Unique, server-generated conversation-scoped token to be roundtripped to the API when sending
/// requests that follow-up within a given conversation.
#[derive(Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ServerConversationToken(String);

impl ServerConversationToken {
    pub fn new(id: String) -> Self {
        Self(id)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn debug_link(&self) -> String {
        format!(
            "{}/debug/maa/{}",
            ChannelState::server_root_url(),
            self.as_str()
        )
    }

    pub fn conversation_link(&self) -> String {
        format!(
            "{}/conversation/{}",
            ChannelState::server_root_url(),
            self.as_str()
        )
    }
}

impl From<ServerConversationToken> for String {
    fn from(value: ServerConversationToken) -> Self {
        value.0
    }
}

// Conversions between AI ServerConversationToken and protocol ServerConversationToken
impl From<session_sharing_protocol::common::ServerConversationToken> for ServerConversationToken {
    fn from(token: session_sharing_protocol::common::ServerConversationToken) -> Self {
        Self(token.to_string())
    }
}

impl TryFrom<ServerConversationToken>
    for session_sharing_protocol::common::ServerConversationToken
{
    type Error = uuid::Error;

    fn try_from(token: ServerConversationToken) -> Result<Self, Self::Error> {
        token.as_str().parse()
    }
}

#[derive(Clone)]
pub struct DirectApiRouteConfig {
    pub provider_id: ProviderId,
    pub model_id: String,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
}

impl std::fmt::Debug for DirectApiRouteConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DirectApiRouteConfig")
            .field("provider_id", &self.provider_id)
            .field("model_id", &self.model_id)
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .field("base_url", &self.base_url)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirectApiRouteConfigError {
    ProviderDisabled(ProviderId),
    MissingApiKey(ProviderId),
    InvalidApiKey(ProviderId),
    MissingBaseUrl(ProviderId),
    InvalidBaseUrl(ProviderId),
}

impl std::fmt::Display for DirectApiRouteConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DirectApiRouteConfigError::ProviderDisabled(provider_id) => {
                write!(
                    f,
                    "Direct API provider {} is disabled",
                    provider_id.display_name()
                )
            }
            DirectApiRouteConfigError::MissingApiKey(provider_id) => {
                write!(
                    f,
                    "Direct API provider {} requires an API key",
                    provider_id.display_name()
                )
            }
            DirectApiRouteConfigError::InvalidApiKey(provider_id) => {
                write!(
                    f,
                    "Direct API provider {} has an invalid API key",
                    provider_id.display_name()
                )
            }
            DirectApiRouteConfigError::MissingBaseUrl(provider_id) => {
                write!(
                    f,
                    "Direct API provider {} requires a base URL",
                    provider_id.display_name()
                )
            }
            DirectApiRouteConfigError::InvalidBaseUrl(provider_id) => {
                write!(
                    f,
                    "Direct API provider {} has an invalid base URL",
                    provider_id.display_name()
                )
            }
        }
    }
}

impl DirectApiRouteConfig {
    pub fn from_selection(
        selection: &DirectApiProfileModelSelection,
        keys: &ApiKeys,
    ) -> Result<Option<Self>, DirectApiRouteConfigError> {
        if !direct_api_provider_is_enabled(keys, selection.provider_id) {
            return Err(DirectApiRouteConfigError::ProviderDisabled(
                selection.provider_id,
            ));
        }

        let api_key = match selection.provider_id {
            ProviderId::OpenAI => match non_empty_string(keys.openai.clone()) {
                Some(key) => Some(key),
                None => {
                    return Err(DirectApiRouteConfigError::MissingApiKey(
                        selection.provider_id,
                    ));
                }
            },
            ProviderId::Anthropic => match non_empty_string(keys.anthropic.clone()) {
                Some(key) => Some(key),
                None => {
                    return Err(DirectApiRouteConfigError::MissingApiKey(
                        selection.provider_id,
                    ));
                }
            },
            ProviderId::GoogleGemini => match non_empty_string(keys.google.clone()) {
                Some(key) => Some(key),
                None => {
                    return Err(DirectApiRouteConfigError::MissingApiKey(
                        selection.provider_id,
                    ));
                }
            },
            ProviderId::OpenRouter => match non_empty_string(keys.open_router.clone()) {
                Some(key) if key.starts_with("sk-or-v1-") => Some(key),
                Some(_key) => {
                    return Err(DirectApiRouteConfigError::InvalidApiKey(
                        selection.provider_id,
                    ));
                }
                None => {
                    return Err(DirectApiRouteConfigError::MissingApiKey(
                        selection.provider_id,
                    ));
                }
            },
            ProviderId::Custom => non_empty_string(keys.custom.clone()),
            ProviderId::Ollama => None,
        };
        let base_url = match selection.provider_id {
            ProviderId::OpenRouter => {
                let url = non_empty_string(keys.openrouter_base_url.clone())
                    .unwrap_or_else(|| OPENROUTER_DEFAULT_BASE_URL.to_string());
                match normalize_direct_api_base_url(&url).ok() {
                    Some(url) => Some(url),
                    None => {
                        return Err(DirectApiRouteConfigError::InvalidBaseUrl(
                            selection.provider_id,
                        ));
                    }
                }
            }
            ProviderId::Ollama => {
                let Some(url) = non_empty_string(keys.ollama_base_url.clone()) else {
                    return Err(DirectApiRouteConfigError::MissingBaseUrl(
                        selection.provider_id,
                    ));
                };
                match normalize_direct_api_base_url(&url).ok() {
                    Some(url) => Some(url),
                    None => {
                        return Err(DirectApiRouteConfigError::InvalidBaseUrl(
                            selection.provider_id,
                        ));
                    }
                }
            }
            ProviderId::Custom => {
                let Some(url) = non_empty_string(keys.custom_base_url.clone()) else {
                    return Err(DirectApiRouteConfigError::MissingBaseUrl(
                        selection.provider_id,
                    ));
                };
                match normalize_direct_api_base_url(&url).ok() {
                    Some(url) => Some(url),
                    None => {
                        return Err(DirectApiRouteConfigError::InvalidBaseUrl(
                            selection.provider_id,
                        ));
                    }
                }
            }
            ProviderId::OpenAI | ProviderId::Anthropic | ProviderId::GoogleGemini => None,
        };

        Ok(Some(Self {
            provider_id: selection.provider_id,
            model_id: selection.model_id.clone(),
            api_key,
            base_url,
        }))
    }
}

fn direct_api_provider_is_enabled(keys: &ApiKeys, provider_id: ProviderId) -> bool {
    keys.enabled_providers
        .get(&provider_id)
        .copied()
        .unwrap_or(true)
}

fn non_empty_string(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim().to_string();
        (!trimmed.is_empty()).then_some(trimmed)
    })
}

#[derive(Debug, Clone)]
pub struct RequestParams {
    pub input: Vec<AIAgentInput>,
    pub conversation_token: Option<ServerConversationToken>,
    pub forked_from_conversation_token: Option<ServerConversationToken>,
    pub ambient_agent_task_id: Option<AmbientAgentTaskId>,
    pub tasks: Vec<warp_multi_agent_api::Task>,
    pub existing_suggestions: Option<Suggestions>,
    pub metadata: Option<RequestMetadata>,
    pub session_context: SessionContext,
    pub model: LLMId,
    pub model_routing: ModelRouting,
    pub direct_api_agent_backend: DirectApiAgentBackend,
    pub direct_api_route_config: Option<DirectApiRouteConfig>,
    pub direct_api_route_error: Option<String>,
    #[allow(unused)]
    pub coding_model: LLMId,
    pub cli_agent_model: LLMId,
    pub computer_use_model: LLMId,
    pub is_memory_enabled: bool,
    pub warp_drive_context_enabled: bool,
    pub context_window_limit: Option<u32>,
    pub mcp_context: Option<MCPContext>,
    pub planning_enabled: bool,
    should_redact_secrets: bool,

    /// User-provided API keys for AI providers (BYO API Key).
    pub api_keys: Option<warp_multi_agent_api::request::settings::ApiKeys>,
    pub allow_use_of_warp_credits_with_byok: bool,
    pub autonomy_level: warp_multi_agent_api::AutonomyLevel,
    pub isolation_level: warp_multi_agent_api::IsolationLevel,
    pub web_search_enabled: bool,
    pub computer_use_enabled: bool,
    pub ask_user_question_enabled: bool,
    pub research_agent_enabled: bool,
    pub orchestration_enabled: bool,
    pub supported_tools_override: Option<Vec<warp_multi_agent_api::ToolType>>,
    /// The conversation ID of the parent agent that spawned this child agent, if any.
    pub parent_agent_id: Option<String>,
    /// The display name for this agent (e.g. "Agent 1"), assigned by the orchestrator.
    pub agent_name: Option<String>,
}

pub type Event = Result<warp_multi_agent_api::ResponseEvent, Arc<AIApiError>>;

#[cfg(not(target_family = "wasm"))]
pub type ResponseStream = Pin<Box<dyn Stream<Item = Event> + Send + 'static>>;

// The WASM version of this type has no bound on `Send`, which is an unnecessary bound when
// targeting wasm because the browser is single-threaded (and we don't leverage WebWorkers for async
// execution in WoW).
#[cfg(target_family = "wasm")]
pub type ResponseStream = Pin<Box<dyn Stream<Item = Event>>>;

#[derive(Debug, Clone)]
pub struct ConversationData {
    pub id: AIConversationId,
    pub tasks: Vec<warp_multi_agent_api::Task>,
    pub server_conversation_token: Option<ServerConversationToken>,
    pub forked_from_conversation_token: Option<ServerConversationToken>,
    pub ambient_agent_task_id: Option<AmbientAgentTaskId>,
    pub existing_suggestions: Option<Suggestions>,
}

impl RequestParams {
    pub fn new(
        terminal_view_id: Option<EntityId>,
        session_context: SessionContext,
        request_input: &RequestInput,
        conversation: ConversationData,
        metadata: Option<RequestMetadata>,
        app: &AppContext,
    ) -> Self {
        let ai_settings = AISettings::as_ref(app);
        let is_memory_enabled = ai_settings.is_memory_enabled(app);
        let warp_drive_context_enabled = ai_settings.is_warp_drive_context_enabled(app);

        // Build MCP context - either grouped by server or flat lists based on feature flag
        let mcp_context = if FeatureFlag::MCPGroupedServerContext.is_enabled() {
            // Group MCP tools and resources by server
            let templatable_manager = TemplatableMCPServerManager::as_ref(app);

            let mut active_servers: Vec<&TemplatableMCPServerInfo> = templatable_manager
                .get_active_templatable_servers()
                .values()
                .copied()
                .collect();

            // If file-based MCP servers are enabled, add active servers in scope of
            // the user's current working directory
            if let Some(cwd) = session_context.current_working_directory() {
                active_servers.extend(
                    templatable_manager
                        .get_active_file_based_servers(Path::new(cwd), app)
                        .values(),
                );
            }

            // Include any ephemeral MCP servers started via the Oz CLI.
            active_servers.extend(
                templatable_manager
                    .get_active_cli_spawned_servers()
                    .values(),
            );

            let servers: Vec<MCPServer> = active_servers
                .into_iter()
                .map(|server| MCPServer {
                    name: server.name().to_string(),
                    description: server.description().unwrap_or_default().to_string(),
                    id: server.installation_id().to_string(),
                    resources: server.resources().to_vec(),
                    tools: server.tools().to_vec(),
                })
                .collect();

            if servers.is_empty() {
                None
            } else {
                #[allow(deprecated)]
                Some(MCPContext {
                    resources: vec![],
                    tools: vec![],
                    servers,
                })
            }
        } else {
            // Flat lists of resources and tools
            let templatable_mcp_manager = TemplatableMCPServerManager::as_ref(app);
            let resources = templatable_mcp_manager
                .resources()
                .cloned()
                .collect::<Vec<_>>();
            let tools = templatable_mcp_manager.tools().cloned().collect::<Vec<_>>();

            #[allow(deprecated)]
            (!resources.is_empty() || !tools.is_empty()).then_some(MCPContext {
                resources,
                tools,
                servers: vec![],
            })
        };

        let should_redact_secrets = get_secret_obfuscation_mode(app).should_redact_secret();

        let profile_data = AIExecutionProfilesModel::as_ref(app)
            .active_profile(terminal_view_id, app)
            .data()
            .clone();
        let requested_model_routing = profile_data.model_routing.effective();
        let mut direct_api_route_error = None;
        let direct_api_route_config = if requested_model_routing.is_direct_api() {
            profile_data
                .direct_api_model
                .as_ref()
                .and_then(|selection| {
                    let keys = ApiKeyManager::as_ref(app).keys(app);
                    match DirectApiRouteConfig::from_selection(selection, &keys) {
                        Ok(config) => config,
                        Err(err) => {
                            direct_api_route_error = Some(err.to_string());
                            None
                        }
                    }
                })
        } else {
            None
        };
        let model_routing = requested_model_routing;
        let direct_api_agent_backend =
            resolve_direct_api_agent_backend(model_routing, &profile_data, app);

        let user_workspaces = UserWorkspaces::as_ref(app);
        let api_keys = if model_routing.is_direct_api() {
            None
        } else {
            ApiKeyManager::as_ref(app).api_keys_for_request(
                user_workspaces.is_byo_api_key_enabled(),
                user_workspaces.is_aws_bedrock_credentials_enabled(app),
                app,
            )
        };
        let allow_use_of_warp_credits_with_byok =
            *AISettings::as_ref(app).can_use_warp_credits_with_byok;

        let app_execution_mode = AppExecutionMode::as_ref(app);
        let autonomy_level = if app_execution_mode.is_autonomous() {
            warp_multi_agent_api::AutonomyLevel::Unsupervised
        } else {
            warp_multi_agent_api::AutonomyLevel::Supervised
        };

        let isolation_level = if app_execution_mode.is_sandboxed() {
            warp_multi_agent_api::IsolationLevel::Sandbox
        } else {
            warp_multi_agent_api::IsolationLevel::None
        };

        let web_search_enabled =
            BlocklistAIPermissions::as_ref(app).get_web_search_enabled(app, terminal_view_id);
        let research_agent_enabled = app
            .private_user_preferences()
            .read_value("ResearchAgentEnabled")
            .ok()
            .flatten()
            .and_then(|s| s.parse().ok())
            .unwrap_or_default();
        let is_ambient_agent = conversation.ambient_agent_task_id.is_some();
        let computer_use_enabled = FeatureFlag::AgentModeComputerUse.is_enabled()
            && BlocklistAIPermissions::as_ref(app)
                .get_computer_use_setting(app, terminal_view_id)
                .is_enabled()
            && computer_use::is_supported_on_current_platform()
            && (FeatureFlag::LocalComputerUse.is_enabled() || is_ambient_agent);
        let ask_user_question_enabled = BlocklistAIPermissions::as_ref(app)
            .get_ask_user_question_setting(app, terminal_view_id)
            != crate::ai::execution_profiles::AskUserQuestionPermission::Never;

        let orchestration_enabled = ai_settings.is_orchestration_enabled(app)
            && session_context
                .session_type()
                .as_ref()
                .is_none_or(|t| matches!(t, crate::terminal::model::session::SessionType::Local));

        // Reconcile the persisted override against the active base model's
        // current `LLMContextWindow` instead of trusting whatever was stored
        // last. If the active model isn't configurable or has been removed
        // server-side, drop the override; otherwise clamp it to the model's
        // current `[min, max]` range. This closes the window between an
        // in-flight model metadata refresh and the next request.
        let context_window_limit = {
            profile_data
                .configurable_context_window(app)
                .and_then(|cw| {
                    profile_data
                        .context_window_limit
                        .map(|v| v.clamp(cw.min, cw.max))
                })
        };

        Self {
            input: request_input.all_inputs().cloned().collect(),
            conversation_token: conversation.server_conversation_token,
            forked_from_conversation_token: conversation.forked_from_conversation_token,
            ambient_agent_task_id: conversation.ambient_agent_task_id,
            tasks: conversation.tasks,
            existing_suggestions: conversation.existing_suggestions,
            context_window_limit,
            metadata,
            session_context,
            model: request_input.model_id.clone(),
            model_routing,
            direct_api_agent_backend,
            direct_api_route_config,
            direct_api_route_error,
            coding_model: request_input.coding_model_id.clone(),
            cli_agent_model: request_input.cli_agent_model_id.clone(),
            computer_use_model: request_input.computer_use_model_id.clone(),
            is_memory_enabled,
            warp_drive_context_enabled,
            mcp_context,
            planning_enabled: true,
            should_redact_secrets,
            api_keys,
            allow_use_of_warp_credits_with_byok,
            autonomy_level,
            isolation_level,
            web_search_enabled,
            computer_use_enabled,
            ask_user_question_enabled,
            research_agent_enabled,
            orchestration_enabled,
            supported_tools_override: request_input.supported_tools_override.clone(),
            parent_agent_id: None,
            agent_name: None,
        }
    }
}

fn resolve_direct_api_agent_backend(
    model_routing: ModelRouting,
    profile_data: &crate::ai::execution_profiles::AIExecutionProfile,
    app: &AppContext,
) -> DirectApiAgentBackend {
    if !model_routing.is_direct_api() || !*DirectAPISettings::as_ref(app).rig_backend_enabled {
        return DirectApiAgentBackend::Native;
    }

    if !cfg!(feature = "direct_api_rig_backend") {
        return DirectApiAgentBackend::Native;
    }

    profile_data.direct_api_agent_backend.effective()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use ai::api_keys::ApiKeyManager;
    use ai::model_registry::ProviderId;
    use chrono::Local;
    use warp_core::features::FeatureFlag;
    use warp_core::settings::{DirectAPISettings, Setting};
    use warpui::{App, EntityId, SingletonEntity};

    use super::*;
    use crate::ai::agent::conversation::AIConversationId;
    use crate::ai::agent::task::TaskId;
    use crate::ai::blocklist::{BlocklistAIPermissions, RequestInput, SessionContext};
    use crate::ai::execution_profiles::profiles::AIExecutionProfilesModel;
    use crate::ai::execution_profiles::{
        DirectApiAgentBackend, DirectApiProfileModelSelection, ModelRouting,
    };
    use crate::ai::llms::{LLMId, LLMPreferences};
    use crate::ai::mcp::TemplatableMCPServerManager;
    use crate::auth::{AuthManager, AuthStateProvider};
    use crate::cloud_object::model::persistence::CloudModel;
    use crate::network::NetworkStatus;
    use crate::server::cloud_objects::update_manager::UpdateManager;
    use crate::server::server_api::ServerApiProvider;
    use crate::server::sync_queue::SyncQueue;
    use crate::settings::PrivacySettings;
    use crate::test_util::settings::initialize_settings_for_tests;
    use crate::workspaces::team_tester::TeamTesterStatus;
    use crate::workspaces::user_workspaces::UserWorkspaces;
    use crate::LaunchMode;

    fn install_request_params_singletons(app: &mut App) -> EntityId {
        initialize_settings_for_tests(app);
        DirectAPISettings::register(app);
        app.add_singleton_model(|_| AuthStateProvider::new_logged_out_for_test());
        app.add_singleton_model(|_| ServerApiProvider::new_for_test());
        app.add_singleton_model(AuthManager::new_for_test);
        app.add_singleton_model(SyncQueue::mock);
        app.add_singleton_model(|_| NetworkStatus::new());
        app.add_singleton_model(TeamTesterStatus::mock);
        app.add_singleton_model(UpdateManager::mock);
        app.add_singleton_model(CloudModel::mock);
        app.add_singleton_model(|_| TemplatableMCPServerManager::default());
        app.add_singleton_model(PrivacySettings::mock);
        app.add_singleton_model(UserWorkspaces::default_mock);
        app.add_singleton_model(BlocklistAIPermissions::new);
        app.add_singleton_model(|ctx| {
            AIExecutionProfilesModel::new(&LaunchMode::new_for_unit_test(), ctx)
        });
        app.add_singleton_model(LLMPreferences::new);

        EntityId::new()
    }

    fn request_input() -> RequestInput {
        let model_id = LLMId::from("warp-provider-model");
        RequestInput {
            conversation_id: AIConversationId::new(),
            input_messages: HashMap::from([(TaskId::new("task".to_string()), vec![])]),
            working_directory: None,
            model_id: model_id.clone(),
            coding_model_id: model_id.clone(),
            cli_agent_model_id: model_id.clone(),
            computer_use_model_id: model_id,
            shared_session_response_initiator: None,
            request_start_ts: Local::now(),
            supported_tools_override: None,
        }
    }

    fn conversation_data() -> ConversationData {
        ConversationData {
            id: AIConversationId::new(),
            tasks: vec![],
            server_conversation_token: None,
            forked_from_conversation_token: None,
            ambient_agent_task_id: None,
            existing_suggestions: None,
        }
    }

    #[test]
    fn direct_api_route_config_debug_redacts_api_key() {
        let config = DirectApiRouteConfig {
            provider_id: ProviderId::OpenAI,
            model_id: "gpt-4o-mini".to_string(),
            api_key: Some("sk-secret".to_string()),
            base_url: None,
        };

        let debug = format!("{config:?}");

        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("sk-secret"));
    }

    #[test]
    fn request_params_use_direct_api_route_without_server_api_keys() {
        App::test((), |mut app| async move {
            let terminal_view_id = install_request_params_singletons(&mut app);
            let _solo_byok = FeatureFlag::SoloUserByok.override_enabled(true);

            ApiKeyManager::handle(&app).update(&mut app, |manager, ctx| {
                manager.set_openai_key(Some("sk-direct".to_string()), ctx);
            });

            AIExecutionProfilesModel::handle(&app).update(&mut app, |model, ctx| {
                let profile_id = *model.active_profile(Some(terminal_view_id), ctx).id();
                model.set_model_routing(profile_id, ModelRouting::DirectApi, ctx);
                model.set_direct_api_model(
                    profile_id,
                    Some(DirectApiProfileModelSelection {
                        provider_id: ProviderId::OpenAI,
                        model_id: "gpt-4o-mini".to_string(),
                    }),
                    ctx,
                );
            });

            let params = app.update(|ctx| {
                let request_input = request_input();
                RequestParams::new(
                    Some(terminal_view_id),
                    SessionContext::new_for_test(),
                    &request_input,
                    conversation_data(),
                    None,
                    ctx,
                )
            });

            assert_eq!(params.model_routing, ModelRouting::DirectApi);
            assert_eq!(params.api_keys, None);
            let route = params
                .direct_api_route_config
                .expect("direct route config should be present");
            assert_eq!(route.provider_id, ProviderId::OpenAI);
            assert_eq!(route.model_id, "gpt-4o-mini");
            assert_eq!(route.api_key.as_deref(), Some("sk-direct"));
        });
    }

    #[test]
    fn request_params_use_native_backend_when_rig_gate_disabled() {
        App::test((), |mut app| async move {
            let terminal_view_id = install_request_params_singletons(&mut app);

            AIExecutionProfilesModel::handle(&app).update(&mut app, |model, ctx| {
                let profile_id = *model.active_profile(Some(terminal_view_id), ctx).id();
                model.set_model_routing(profile_id, ModelRouting::DirectApi, ctx);
                model.set_direct_api_agent_backend(
                    profile_id,
                    DirectApiAgentBackend::RigAgent,
                    ctx,
                );
            });

            let params = app.update(|ctx| {
                let request_input = request_input();
                RequestParams::new(
                    Some(terminal_view_id),
                    SessionContext::new_for_test(),
                    &request_input,
                    conversation_data(),
                    None,
                    ctx,
                )
            });

            assert_eq!(params.model_routing, ModelRouting::DirectApi);
            assert_eq!(
                params.direct_api_agent_backend,
                DirectApiAgentBackend::Native
            );
        });
    }

    #[cfg(not(feature = "direct_api_rig_backend"))]
    #[test]
    fn request_params_use_native_backend_when_rig_feature_disabled() {
        App::test((), |mut app| async move {
            let terminal_view_id = install_request_params_singletons(&mut app);

            DirectAPISettings::handle(&app).update(&mut app, |settings, ctx| {
                settings
                    .rig_backend_enabled
                    .set_value(true, ctx)
                    .expect("test can enable Rig backend setting");
            });
            AIExecutionProfilesModel::handle(&app).update(&mut app, |model, ctx| {
                let profile_id = *model.active_profile(Some(terminal_view_id), ctx).id();
                model.set_model_routing(profile_id, ModelRouting::DirectApi, ctx);
                model.set_direct_api_agent_backend(
                    profile_id,
                    DirectApiAgentBackend::RigAgent,
                    ctx,
                );
            });

            let params = app.update(|ctx| {
                let request_input = request_input();
                RequestParams::new(
                    Some(terminal_view_id),
                    SessionContext::new_for_test(),
                    &request_input,
                    conversation_data(),
                    None,
                    ctx,
                )
            });

            assert_eq!(params.model_routing, ModelRouting::DirectApi);
            assert_eq!(
                params.direct_api_agent_backend,
                DirectApiAgentBackend::Native
            );
        });
    }

    #[cfg(feature = "direct_api_rig_backend")]
    #[test]
    fn request_params_use_rig_backend_when_profile_gate_and_feature_enable_it() {
        App::test((), |mut app| async move {
            let terminal_view_id = install_request_params_singletons(&mut app);

            DirectAPISettings::handle(&app).update(&mut app, |settings, ctx| {
                settings
                    .rig_backend_enabled
                    .set_value(true, ctx)
                    .expect("test can enable Rig backend setting");
            });
            AIExecutionProfilesModel::handle(&app).update(&mut app, |model, ctx| {
                let profile_id = *model.active_profile(Some(terminal_view_id), ctx).id();
                model.set_model_routing(profile_id, ModelRouting::DirectApi, ctx);
                model.set_direct_api_agent_backend(
                    profile_id,
                    DirectApiAgentBackend::RigAgent,
                    ctx,
                );
            });

            let params = app.update(|ctx| {
                let request_input = request_input();
                RequestParams::new(
                    Some(terminal_view_id),
                    SessionContext::new_for_test(),
                    &request_input,
                    conversation_data(),
                    None,
                    ctx,
                )
            });

            assert_eq!(params.model_routing, ModelRouting::DirectApi);
            assert_eq!(
                params.direct_api_agent_backend,
                DirectApiAgentBackend::RigAgent
            );
        });
    }

    #[test]
    fn request_params_keep_direct_api_routing_when_direct_api_model_is_missing() {
        App::test((), |mut app| async move {
            let terminal_view_id = install_request_params_singletons(&mut app);

            AIExecutionProfilesModel::handle(&app).update(&mut app, |model, ctx| {
                let profile_id = *model.active_profile(Some(terminal_view_id), ctx).id();
                model.set_model_routing(profile_id, ModelRouting::DirectApi, ctx);
                model.set_direct_api_model(profile_id, None, ctx);
            });

            let params = app.update(|ctx| {
                let request_input = request_input();
                RequestParams::new(
                    Some(terminal_view_id),
                    SessionContext::new_for_test(),
                    &request_input,
                    conversation_data(),
                    None,
                    ctx,
                )
            });

            assert_eq!(params.model_routing, ModelRouting::DirectApi);
            assert!(params.direct_api_route_config.is_none());
            assert_eq!(params.direct_api_route_error, None);
            assert_eq!(params.api_keys, None);
        });
    }

    #[test]
    fn request_params_keep_direct_api_routing_when_direct_api_key_is_missing() {
        App::test((), |mut app| async move {
            let terminal_view_id = install_request_params_singletons(&mut app);

            AIExecutionProfilesModel::handle(&app).update(&mut app, |model, ctx| {
                let profile_id = *model.active_profile(Some(terminal_view_id), ctx).id();
                model.set_model_routing(profile_id, ModelRouting::DirectApi, ctx);
                model.set_direct_api_model(
                    profile_id,
                    Some(DirectApiProfileModelSelection {
                        provider_id: ProviderId::OpenAI,
                        model_id: "gpt-4o-mini".to_string(),
                    }),
                    ctx,
                );
            });

            let params = app.update(|ctx| {
                let request_input = request_input();
                RequestParams::new(
                    Some(terminal_view_id),
                    SessionContext::new_for_test(),
                    &request_input,
                    conversation_data(),
                    None,
                    ctx,
                )
            });

            assert_eq!(params.model_routing, ModelRouting::DirectApi);
            assert!(params.direct_api_route_config.is_none());
            assert_eq!(
                params.direct_api_route_error.as_deref(),
                Some("Direct API provider OpenAI requires an API key")
            );
            assert_eq!(params.api_keys, None);
        });
    }

    #[test]
    fn request_params_do_not_route_to_disabled_direct_api_provider() {
        App::test((), |mut app| async move {
            let terminal_view_id = install_request_params_singletons(&mut app);

            ApiKeyManager::handle(&app).update(&mut app, |manager, ctx| {
                manager.set_openai_key(Some("sk-direct".to_string()), ctx);
                manager.set_provider_enabled(ProviderId::OpenAI, false, ctx);
            });

            AIExecutionProfilesModel::handle(&app).update(&mut app, |model, ctx| {
                let profile_id = *model.active_profile(Some(terminal_view_id), ctx).id();
                model.set_model_routing(profile_id, ModelRouting::DirectApi, ctx);
                model.set_direct_api_model(
                    profile_id,
                    Some(DirectApiProfileModelSelection {
                        provider_id: ProviderId::OpenAI,
                        model_id: "gpt-4o-mini".to_string(),
                    }),
                    ctx,
                );
            });

            let params = app.update(|ctx| {
                let request_input = request_input();
                RequestParams::new(
                    Some(terminal_view_id),
                    SessionContext::new_for_test(),
                    &request_input,
                    conversation_data(),
                    None,
                    ctx,
                )
            });

            assert_eq!(params.model_routing, ModelRouting::DirectApi);
            assert!(params.direct_api_route_config.is_none());
            assert_eq!(
                params.direct_api_route_error.as_deref(),
                Some("Direct API provider OpenAI is disabled")
            );
            assert_eq!(params.api_keys, None);
        });
    }

    #[test]
    fn request_params_keep_direct_api_routing_when_direct_api_base_url_is_missing() {
        App::test((), |mut app| async move {
            let terminal_view_id = install_request_params_singletons(&mut app);

            ApiKeyManager::handle(&app).update(&mut app, |manager, ctx| {
                manager.set_custom_key(Some("custom-key".to_string()), ctx);
            });
            AIExecutionProfilesModel::handle(&app).update(&mut app, |model, ctx| {
                let profile_id = *model.active_profile(Some(terminal_view_id), ctx).id();
                model.set_model_routing(profile_id, ModelRouting::DirectApi, ctx);
                model.set_direct_api_model(
                    profile_id,
                    Some(DirectApiProfileModelSelection {
                        provider_id: ProviderId::Custom,
                        model_id: "custom-model".to_string(),
                    }),
                    ctx,
                );
            });

            let params = app.update(|ctx| {
                let request_input = request_input();
                RequestParams::new(
                    Some(terminal_view_id),
                    SessionContext::new_for_test(),
                    &request_input,
                    conversation_data(),
                    None,
                    ctx,
                )
            });

            assert_eq!(params.model_routing, ModelRouting::DirectApi);
            assert!(params.direct_api_route_config.is_none());
            assert_eq!(
                params.direct_api_route_error.as_deref(),
                Some("Direct API provider Custom (OpenAI-compatible) requires a base URL")
            );
            assert_eq!(params.api_keys, None);
        });
    }

    #[test]
    fn request_params_keep_direct_api_routing_when_direct_api_base_url_is_invalid() {
        App::test((), |mut app| async move {
            let terminal_view_id = install_request_params_singletons(&mut app);

            ApiKeyManager::handle(&app).update(&mut app, |manager, ctx| {
                manager.set_custom_key(Some("custom-key".to_string()), ctx);
                manager.set_custom_base_url(Some("not a url".to_string()), ctx);
            });
            AIExecutionProfilesModel::handle(&app).update(&mut app, |model, ctx| {
                let profile_id = *model.active_profile(Some(terminal_view_id), ctx).id();
                model.set_model_routing(profile_id, ModelRouting::DirectApi, ctx);
                model.set_direct_api_model(
                    profile_id,
                    Some(DirectApiProfileModelSelection {
                        provider_id: ProviderId::Custom,
                        model_id: "custom-model".to_string(),
                    }),
                    ctx,
                );
            });

            let params = app.update(|ctx| {
                let request_input = request_input();
                RequestParams::new(
                    Some(terminal_view_id),
                    SessionContext::new_for_test(),
                    &request_input,
                    conversation_data(),
                    None,
                    ctx,
                )
            });

            assert_eq!(params.model_routing, ModelRouting::DirectApi);
            assert!(params.direct_api_route_config.is_none());
            assert_eq!(
                params.direct_api_route_error.as_deref(),
                Some("Direct API provider Custom (OpenAI-compatible) has an invalid base URL")
            );
            assert_eq!(params.api_keys, None);
        });
    }

    #[test]
    fn openrouter_direct_api_route_uses_default_base_url() {
        App::test((), |mut app| async move {
            let terminal_view_id = install_request_params_singletons(&mut app);

            ApiKeyManager::handle(&app).update(&mut app, |manager, ctx| {
                manager.set_open_router_key(Some("sk-or-v1-direct".to_string()), ctx);
            });
            AIExecutionProfilesModel::handle(&app).update(&mut app, |model, ctx| {
                let profile_id = *model.active_profile(Some(terminal_view_id), ctx).id();
                model.set_model_routing(profile_id, ModelRouting::DirectApi, ctx);
                model.set_direct_api_model(
                    profile_id,
                    Some(DirectApiProfileModelSelection {
                        provider_id: ProviderId::OpenRouter,
                        model_id: "openai/gpt-4o-mini".to_string(),
                    }),
                    ctx,
                );
            });

            let params = app.update(|ctx| {
                let request_input = request_input();
                RequestParams::new(
                    Some(terminal_view_id),
                    SessionContext::new_for_test(),
                    &request_input,
                    conversation_data(),
                    None,
                    ctx,
                )
            });

            assert_eq!(params.model_routing, ModelRouting::DirectApi);
            let route = params
                .direct_api_route_config
                .expect("OpenRouter direct route should be configured");
            assert_eq!(route.provider_id, ProviderId::OpenRouter);
            assert_eq!(route.api_key.as_deref(), Some("sk-or-v1-direct"));
            assert_eq!(route.base_url.as_deref(), Some(OPENROUTER_DEFAULT_BASE_URL));
        });
    }

    #[test]
    fn request_params_reject_openrouter_key_with_invalid_prefix() {
        App::test((), |mut app| async move {
            let terminal_view_id = install_request_params_singletons(&mut app);

            ApiKeyManager::handle(&app).update(&mut app, |manager, ctx| {
                manager.set_open_router_key(Some("sk-or-direct".to_string()), ctx);
            });
            AIExecutionProfilesModel::handle(&app).update(&mut app, |model, ctx| {
                let profile_id = *model.active_profile(Some(terminal_view_id), ctx).id();
                model.set_model_routing(profile_id, ModelRouting::DirectApi, ctx);
                model.set_direct_api_model(
                    profile_id,
                    Some(DirectApiProfileModelSelection {
                        provider_id: ProviderId::OpenRouter,
                        model_id: "openai/gpt-4o-mini".to_string(),
                    }),
                    ctx,
                );
            });

            let params = app.update(|ctx| {
                let request_input = request_input();
                RequestParams::new(
                    Some(terminal_view_id),
                    SessionContext::new_for_test(),
                    &request_input,
                    conversation_data(),
                    None,
                    ctx,
                )
            });

            assert_eq!(params.model_routing, ModelRouting::DirectApi);
            assert!(params.direct_api_route_config.is_none());
            assert_eq!(
                params.direct_api_route_error.as_deref(),
                Some("Direct API provider OpenRouter has an invalid API key")
            );
            assert_eq!(params.api_keys, None);
        });
    }
}
