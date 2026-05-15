pub use crate::aws_credentials::{AwsCredentials, AwsCredentialsState};
use crate::model_registry::{ModelListCache, ProviderId};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use warp_multi_agent_api as api;
use warpui::{Entity, ModelContext, SingletonEntity};

/// Emitted when user-provided API keys are updated in-memory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiKeyManagerEvent {
    KeysUpdated,
}

/// User-provided API keys for AI providers.
///
/// These are used for "Bring Your Own API Key" functionality, allowing
/// users to use their own API keys instead of Warp's.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ApiKeys {
    pub google: Option<String>,
    pub anthropic: Option<String>,
    pub openai: Option<String>,
    pub open_router: Option<String>,
    #[serde(default)]
    pub custom: Option<String>,

    // ---- Phase 2 additions (all #[serde(default)] so old payloads parse) ----
    #[serde(default)]
    pub selected_provider: Option<ProviderId>,
    #[serde(default)]
    pub custom_base_url: Option<String>,
    #[serde(default)]
    pub openrouter_base_url: Option<String>,
    #[serde(default)]
    pub ollama_base_url: Option<String>,
    #[serde(default)]
    pub selected_models: std::collections::BTreeMap<ProviderId, String>,
}

impl ApiKeys {
    pub fn has_any_key(&self) -> bool {
        self.openai.is_some()
            || self.anthropic.is_some()
            || self.google.is_some()
            || self.open_router.is_some()
            || self.custom.is_some()
    }
}

/// Controls how AWS credentials are refreshed by [`ApiKeyManager`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum AwsCredentialsRefreshStrategy {
    /// Load credentials from the local AWS credential chain (~/.aws). This is the default.
    #[default]
    LocalChain,
    /// Credentials are managed externally via OIDC/STS.
    /// The task ID is used to scope the STS AssumeRoleWithWebIdentity session.
    /// The role ARN is the IAM role to assume via STS.
    OidcManaged {
        task_id: Option<String>,
        role_arn: String,
    },
}

/// A structure that manages API keys for AI providers.
pub struct ApiKeyManager {
    /// Lazy-loaded cache of API keys. None = not loaded yet.
    /// Uses RefCell for interior mutability to support lazy loading with immutable references.
    keys_cache: RefCell<Option<ApiKeys>>,
    pub(crate) aws_credentials_state: AwsCredentialsState,
    aws_credentials_refresh_strategy: AwsCredentialsRefreshStrategy,
}

impl ApiKeyManager {
    pub fn new(_ctx: &mut ModelContext<Self>) -> Self {
        // Don't load keys on init - use lazy loading instead
        Self {
            keys_cache: RefCell::new(None),
            aws_credentials_state: AwsCredentialsState::Missing,
            aws_credentials_refresh_strategy: AwsCredentialsRefreshStrategy::default(),
        }
    }

    /// Check if the cache is empty (keys not yet loaded).
    pub fn is_cache_empty(&self) -> bool {
        self.keys_cache.borrow().is_none()
    }

    /// Internal method to ensure keys are loaded into cache.
    /// Uses &AppContext for read-only settings access.
    fn ensure_keys_loaded(&self, ctx: &warpui::AppContext) {
        let mut cache = self.keys_cache.borrow_mut();
        if cache.is_none() {
            // Lazy load from DirectAPISettings (settings.toml) on first access
            *cache = Some(Self::load_keys_from_settings(ctx));
        }
    }

    /// Get API keys, loading from settings.toml on first access (lazy load).
    pub fn keys(&self, ctx: &warpui::AppContext) -> ApiKeys {
        self.ensure_keys_loaded(ctx);
        self.keys_cache.borrow().as_ref().unwrap().clone()
    }

    pub fn set_google_key(&mut self, key: Option<String>, ctx: &mut ModelContext<Self>) {
        // Ensure cache is loaded
        self.ensure_keys_loaded(ctx);

        // Update cache
        if let Some(ref mut keys) = self.keys_cache.borrow_mut().as_mut() {
            keys.google = key;
        }

        // Invalidate model list cache for this provider
        if let Ok(cache) = ModelListCache::new() {
            let _ = cache.invalidate(ProviderId::GoogleGemini);
        }

        ctx.emit(ApiKeyManagerEvent::KeysUpdated);
        self.write_keys_to_settings(ctx);
    }

    pub fn set_anthropic_key(&mut self, key: Option<String>, ctx: &mut ModelContext<Self>) {
        // Ensure cache is loaded
        self.ensure_keys_loaded(ctx);

        // Update cache
        if let Some(ref mut keys) = self.keys_cache.borrow_mut().as_mut() {
            keys.anthropic = key;
        }

        // Invalidate model list cache for this provider
        if let Ok(cache) = ModelListCache::new() {
            let _ = cache.invalidate(ProviderId::Anthropic);
        }

        ctx.emit(ApiKeyManagerEvent::KeysUpdated);
        self.write_keys_to_settings(ctx);
    }

    pub fn set_openai_key(&mut self, key: Option<String>, ctx: &mut ModelContext<Self>) {
        // Ensure cache is loaded
        self.ensure_keys_loaded(ctx);

        // Update cache
        if let Some(ref mut keys) = self.keys_cache.borrow_mut().as_mut() {
            keys.openai = key;
        }

        // Invalidate model list cache for this provider
        if let Ok(cache) = ModelListCache::new() {
            let _ = cache.invalidate(ProviderId::OpenAI);
        }

        ctx.emit(ApiKeyManagerEvent::KeysUpdated);
        self.write_keys_to_settings(ctx);
    }

    pub fn set_open_router_key(&mut self, key: Option<String>, ctx: &mut ModelContext<Self>) {
        // Ensure cache is loaded
        self.ensure_keys_loaded(ctx);

        // Update cache
        if let Some(ref mut keys) = self.keys_cache.borrow_mut().as_mut() {
            keys.open_router = key;
        }

        // Invalidate model list cache for this provider
        if let Ok(cache) = ModelListCache::new() {
            let _ = cache.invalidate(ProviderId::OpenRouter);
        }

        ctx.emit(ApiKeyManagerEvent::KeysUpdated);
        self.write_keys_to_settings(ctx);
    }

    pub fn set_custom_key(&mut self, key: Option<String>, ctx: &mut ModelContext<Self>) {
        self.ensure_keys_loaded(ctx);

        if let Some(ref mut keys) = self.keys_cache.borrow_mut().as_mut() {
            keys.custom = key;
        }

        if let Ok(cache) = ModelListCache::new() {
            let _ = cache.invalidate(ProviderId::Custom);
        }

        ctx.emit(ApiKeyManagerEvent::KeysUpdated);
        self.write_keys_to_settings(ctx);
    }

    pub fn set_selected_provider(
        &mut self,
        provider: Option<ProviderId>,
        ctx: &mut ModelContext<Self>,
    ) {
        self.ensure_keys_loaded(ctx);

        if let Some(ref mut keys) = self.keys_cache.borrow_mut().as_mut() {
            keys.selected_provider = provider;
        }

        ctx.emit(ApiKeyManagerEvent::KeysUpdated);
        self.write_keys_to_settings(ctx);
    }

    pub fn set_custom_base_url(&mut self, url: Option<String>, ctx: &mut ModelContext<Self>) {
        self.ensure_keys_loaded(ctx);

        if let Some(ref mut keys) = self.keys_cache.borrow_mut().as_mut() {
            keys.custom_base_url = url;
        }

        if let Ok(cache) = ModelListCache::new() {
            let _ = cache.invalidate(ProviderId::Custom);
        }

        ctx.emit(ApiKeyManagerEvent::KeysUpdated);
        self.write_keys_to_settings(ctx);
    }

    pub fn set_openrouter_base_url(&mut self, url: Option<String>, ctx: &mut ModelContext<Self>) {
        self.ensure_keys_loaded(ctx);

        if let Some(ref mut keys) = self.keys_cache.borrow_mut().as_mut() {
            keys.openrouter_base_url = url;
        }

        if let Ok(cache) = ModelListCache::new() {
            let _ = cache.invalidate(ProviderId::OpenRouter);
        }

        ctx.emit(ApiKeyManagerEvent::KeysUpdated);
        self.write_keys_to_settings(ctx);
    }

    pub fn set_ollama_base_url(&mut self, url: Option<String>, ctx: &mut ModelContext<Self>) {
        self.ensure_keys_loaded(ctx);

        if let Some(ref mut keys) = self.keys_cache.borrow_mut().as_mut() {
            keys.ollama_base_url = url;
        }

        if let Ok(cache) = ModelListCache::new() {
            let _ = cache.invalidate(ProviderId::Ollama);
        }

        ctx.emit(ApiKeyManagerEvent::KeysUpdated);
        self.write_keys_to_settings(ctx);
    }

    /// Persist the user's selected model for a given provider. The selection
    /// is stored alongside the API keys in settings.toml so it survives restarts.
    pub fn set_selected_model(
        &mut self,
        provider: ProviderId,
        model_id: String,
        ctx: &mut ModelContext<Self>,
    ) {
        // Ensure cache is loaded
        self.ensure_keys_loaded(ctx);

        // Update cache
        if let Some(ref mut keys) = self.keys_cache.borrow_mut().as_mut() {
            keys.selected_models.insert(provider, model_id);
        }

        ctx.emit(ApiKeyManagerEvent::KeysUpdated);
        self.write_keys_to_settings(ctx);
    }

    /// Returns the selected model for a given provider, falling back to per-provider defaults.
    ///
    /// Call when creating a Direct API conversation to determine the model_id for
    /// `ConversationRepository::create_conversation()`. Returns `None` for providers
    /// without sensible defaults (Ollama, OpenRouter, Custom).
    ///
    /// **Note**: Conversation-starting UI is pending (Phase 3). This API is ready to use.
    pub fn get_selected_model_for_provider(
        &self,
        provider: ProviderId,
        ctx: &warpui::AppContext,
    ) -> Option<String> {
        let keys = self.keys(ctx);

        // Check if user has explicitly selected a model
        if let Some(model_id) = keys.selected_models.get(&provider) {
            return Some(model_id.clone());
        }

        // Fall back to per-provider defaults
        match provider {
            ProviderId::OpenAI => Some("gpt-4o-mini".to_string()),
            ProviderId::Anthropic => Some("claude-3-5-sonnet-20241022".to_string()),
            ProviderId::GoogleGemini => Some("gemini-2.0-flash".to_string()),
            ProviderId::Ollama => None, // No default - user must configure local model
            ProviderId::OpenRouter => None, // No default - too many options
            ProviderId::Custom => None, // No default - unknown endpoint
        }
    }

    pub fn set_aws_credentials_state(
        &mut self,
        state: AwsCredentialsState,
        ctx: &mut ModelContext<Self>,
    ) {
        self.aws_credentials_state = state;
        ctx.emit(ApiKeyManagerEvent::KeysUpdated);
    }

    pub fn aws_credentials_state(&self) -> &AwsCredentialsState {
        &self.aws_credentials_state
    }

    pub fn aws_credentials_refresh_strategy(&self) -> AwsCredentialsRefreshStrategy {
        self.aws_credentials_refresh_strategy.clone()
    }

    pub fn set_aws_credentials_refresh_strategy(
        &mut self,
        strategy: AwsCredentialsRefreshStrategy,
    ) {
        self.aws_credentials_refresh_strategy = strategy;
    }

    pub fn api_keys_for_request(
        &self,
        include_byo_keys: bool,
        include_aws_bedrock_credentials: bool,
        ctx: &warpui::AppContext,
    ) -> Option<api::request::settings::ApiKeys> {
        // Lazy load keys on first request
        let keys = self.keys(ctx);

        let anthropic = include_byo_keys
            .then(|| keys.anthropic.clone())
            .flatten()
            .unwrap_or_default();
        let openai = include_byo_keys
            .then(|| keys.openai.clone())
            .flatten()
            .unwrap_or_default();
        let google = include_byo_keys
            .then(|| keys.google.clone())
            .flatten()
            .unwrap_or_default();
        let open_router = include_byo_keys
            .then(|| keys.open_router.clone())
            .flatten()
            .unwrap_or_default();
        // Also include credentials when running with OIDC-managed Bedrock inference, regardless
        // of the per-user setting flag (which only applies to the local credential chain path).
        let include_aws = include_aws_bedrock_credentials
            || matches!(
                self.aws_credentials_refresh_strategy,
                AwsCredentialsRefreshStrategy::OidcManaged { .. }
            );
        let aws_credentials = include_aws
            .then(|| match self.aws_credentials_state {
                AwsCredentialsState::Loaded {
                    ref credentials, ..
                } => Some(credentials.clone().into()),
                _ => None,
            })
            .flatten();

        if anthropic.is_empty()
            && openai.is_empty()
            && google.is_empty()
            && open_router.is_empty()
            && aws_credentials.is_none()
        {
            None
        } else {
            Some(api::request::settings::ApiKeys {
                anthropic,
                openai,
                google,
                open_router,
                allow_use_of_warp_credits: false,
                aws_credentials,
            })
        }
    }

    /// Load API keys from DirectAPISettings (settings.toml).
    fn load_keys_from_settings(ctx: &warpui::AppContext) -> ApiKeys {
        use warp_core::settings::{DirectAPISettings, Setting};
        use warpui::SingletonEntity;

        let settings = DirectAPISettings::as_ref(ctx);

        // Map provider string to ProviderId for selected_provider
        let selected_provider =
            settings
                .selected_provider
                .value()
                .as_ref()
                .and_then(|s| match s.as_str() {
                    "OpenAI" => Some(ProviderId::OpenAI),
                    "Anthropic" => Some(ProviderId::Anthropic),
                    "GoogleGemini" => Some(ProviderId::GoogleGemini),
                    "Ollama" => Some(ProviderId::Ollama),
                    "OpenRouter" => Some(ProviderId::OpenRouter),
                    "Custom" => Some(ProviderId::Custom),
                    _ => None,
                });

        // Parse selected_models from HashMap<String, String> to BTreeMap<ProviderId, String>
        let selected_models = settings
            .selected_models
            .value()
            .iter()
            .filter_map(|(provider_str, model)| {
                let provider_id = match provider_str.as_str() {
                    "OpenAI" => ProviderId::OpenAI,
                    "Anthropic" => ProviderId::Anthropic,
                    "GoogleGemini" => ProviderId::GoogleGemini,
                    "Ollama" => ProviderId::Ollama,
                    "OpenRouter" => ProviderId::OpenRouter,
                    "Custom" => ProviderId::Custom,
                    _ => return None,
                };
                Some((provider_id, model.clone()))
            })
            .collect();

        ApiKeys {
            openai: settings.api_key_openai.value().clone(),
            anthropic: settings.api_key_anthropic.value().clone(),
            google: settings.api_key_google.value().clone(),
            open_router: settings.api_key_openrouter.value().clone(),
            custom: settings.api_key_custom.value().clone(),
            selected_provider,
            custom_base_url: settings.base_url_custom.value().clone(),
            openrouter_base_url: settings.base_url_openrouter.value().clone(),
            ollama_base_url: settings.base_url_ollama.value().clone(),
            selected_models,
        }
    }

    /// Write API keys to DirectAPISettings (settings.toml).
    fn write_keys_to_settings(&self, ctx: &mut ModelContext<Self>) {
        use warp_core::settings::{DirectAPISettings, Setting};
        use warpui::SingletonEntity;

        // Only write if cache is loaded
        let cache = self.keys_cache.borrow();
        let Some(ref keys) = *cache else {
            return;
        };

        // Update DirectAPISettings with the new values
        DirectAPISettings::handle(ctx).update(ctx, |settings, ctx| {
            // Update selected provider
            let selected_provider = keys.selected_provider.as_ref().map(|provider| {
                match provider {
                    ProviderId::OpenAI => "OpenAI",
                    ProviderId::Anthropic => "Anthropic",
                    ProviderId::GoogleGemini => "GoogleGemini",
                    ProviderId::Ollama => "Ollama",
                    ProviderId::OpenRouter => "OpenRouter",
                    ProviderId::Custom => "Custom",
                }
                .to_string()
            });
            if let Err(e) = settings.selected_provider.set_value(selected_provider, ctx) {
                log::error!("Failed to save selected_provider: {e:#}");
            }

            // Update API keys
            if let Err(e) = settings.api_key_openai.set_value(keys.openai.clone(), ctx) {
                log::error!("Failed to save OpenAI API key: {e:#}");
            }
            if let Err(e) = settings
                .api_key_anthropic
                .set_value(keys.anthropic.clone(), ctx)
            {
                log::error!("Failed to save Anthropic API key: {e:#}");
            }
            if let Err(e) = settings.api_key_google.set_value(keys.google.clone(), ctx) {
                log::error!("Failed to save Google API key: {e:#}");
            }
            if let Err(e) = settings
                .api_key_openrouter
                .set_value(keys.open_router.clone(), ctx)
            {
                log::error!("Failed to save OpenRouter API key: {e:#}");
            }
            if let Err(e) = settings.api_key_custom.set_value(keys.custom.clone(), ctx) {
                log::error!("Failed to save custom API key: {e:#}");
            }

            // Update base URLs
            if let Err(e) = settings
                .base_url_custom
                .set_value(keys.custom_base_url.clone(), ctx)
            {
                log::error!("Failed to save custom base URL: {e:#}");
            }
            if let Err(e) = settings
                .base_url_openrouter
                .set_value(keys.openrouter_base_url.clone(), ctx)
            {
                log::error!("Failed to save OpenRouter base URL: {e:#}");
            }
            if let Err(e) = settings
                .base_url_ollama
                .set_value(keys.ollama_base_url.clone(), ctx)
            {
                log::error!("Failed to save Ollama base URL: {e:#}");
            }

            // Update selected models - convert BTreeMap to HashMap
            let selected_models_map: std::collections::HashMap<String, String> = keys
                .selected_models
                .iter()
                .map(|(provider_id, model)| {
                    let provider_str = match provider_id {
                        ProviderId::OpenAI => "OpenAI",
                        ProviderId::Anthropic => "Anthropic",
                        ProviderId::GoogleGemini => "GoogleGemini",
                        ProviderId::Ollama => "Ollama",
                        ProviderId::OpenRouter => "OpenRouter",
                        ProviderId::Custom => "Custom",
                    };
                    (provider_str.to_string(), model.clone())
                })
                .collect();

            if let Err(e) = settings.selected_models.set_value(selected_models_map, ctx) {
                log::error!("Failed to save selected models: {e:#}");
            }
        });
    }
}

impl Entity for ApiKeyManager {
    type Event = ApiKeyManagerEvent;
}

impl SingletonEntity for ApiKeyManager {}

#[cfg(test)]
#[path = "api_keys_tests.rs"]
mod tests;
