pub use crate::aws_credentials::{AwsCredentials, AwsCredentialsState};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use warp_multi_agent_api as api;
use warpui::{Entity, ModelContext, SingletonEntity};
use warpui_extras::secure_storage::{self, AppContextExt};

const SECURE_STORAGE_KEY: &str = "AiApiKeys";

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
}

impl ApiKeys {
    pub fn has_any_key(&self) -> bool {
        self.openai.is_some()
            || self.anthropic.is_some()
            || self.google.is_some()
            || self.open_router.is_some()
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
    /// Uses &AppContext for read-only secure storage access.
    fn ensure_keys_loaded(&self, ctx: &warpui::AppContext) {
        let mut cache = self.keys_cache.borrow_mut();
        if cache.is_none() {
            // Lazy load from secure storage on first access
            *cache = Some(Self::load_keys_from_secure_storage_readonly(ctx));
        }
    }

    /// Get API keys, loading from secure storage on first access (lazy load).
    /// Works with read-only AppContext since secure_storage.read_value takes &self.
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

        ctx.emit(ApiKeyManagerEvent::KeysUpdated);
        self.write_keys_to_secure_storage(ctx);
    }

    pub fn set_anthropic_key(&mut self, key: Option<String>, ctx: &mut ModelContext<Self>) {
        // Ensure cache is loaded
        self.ensure_keys_loaded(ctx);

        // Update cache
        if let Some(ref mut keys) = self.keys_cache.borrow_mut().as_mut() {
            keys.anthropic = key;
        }

        ctx.emit(ApiKeyManagerEvent::KeysUpdated);
        self.write_keys_to_secure_storage(ctx);
    }

    pub fn set_openai_key(&mut self, key: Option<String>, ctx: &mut ModelContext<Self>) {
        // Ensure cache is loaded
        self.ensure_keys_loaded(ctx);

        // Update cache
        if let Some(ref mut keys) = self.keys_cache.borrow_mut().as_mut() {
            keys.openai = key;
        }

        ctx.emit(ApiKeyManagerEvent::KeysUpdated);
        self.write_keys_to_secure_storage(ctx);
    }

    pub fn set_open_router_key(&mut self, key: Option<String>, ctx: &mut ModelContext<Self>) {
        // Ensure cache is loaded
        self.ensure_keys_loaded(ctx);

        // Update cache
        if let Some(ref mut keys) = self.keys_cache.borrow_mut().as_mut() {
            keys.open_router = key;
        }

        ctx.emit(ApiKeyManagerEvent::KeysUpdated);
        self.write_keys_to_secure_storage(ctx);
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

    fn load_keys_from_secure_storage_readonly(ctx: &warpui::AppContext) -> ApiKeys {
        use warpui::SingletonEntity;

        let storage = <secure_storage::Model as SingletonEntity>::as_ref(ctx);
        let key_json = match storage.as_ref().read_value(SECURE_STORAGE_KEY) {
            Ok(json) => json,
            Err(e) => {
                if !matches!(e, secure_storage::Error::NotFound) {
                    log::error!("Failed to read API keys from secure storage: {e:#}");
                }
                return ApiKeys::default();
            }
        };

        let keys = match serde_json::from_str(&key_json) {
            Ok(keys) => keys,
            Err(e) => {
                log::error!("Failed to deserialize API keys: {e:#}");
                ApiKeys::default()
            }
        };

        keys
    }

    fn write_keys_to_secure_storage(&self, ctx: &mut ModelContext<Self>) {
        // Only write if cache is loaded
        let cache = self.keys_cache.borrow();
        let Some(ref keys) = *cache else {
            return;
        };

        let json = match serde_json::to_string(keys) {
            Ok(json) => json,
            Err(e) => {
                log::error!("Failed to serialize API keys: {e:#}");
                return;
            }
        };

        if let Err(e) = ctx.secure_storage().write_value(SECURE_STORAGE_KEY, &json) {
            log::error!("Failed to write API keys to secure storage: {e:#}");
        }
    }
}

impl Entity for ApiKeyManager {
    type Event = ApiKeyManagerEvent;
}

impl SingletonEntity for ApiKeyManager {}

#[cfg(test)]
#[path = "api_keys_tests.rs"]
mod tests;
