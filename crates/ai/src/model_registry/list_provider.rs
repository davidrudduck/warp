//! Model list provider trait and types for fetching available models from LLM providers.

use crate::model_registry::ProviderId;

/// Descriptor for a single model returned by a provider's API.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ModelDescriptor {
    /// Model ID (e.g., "gpt-4o", "claude-3-5-sonnet-latest")
    pub id: String,
    /// Optional display name (may differ from ID for some providers)
    pub display_name: Option<String>,
    /// Optional context window size in tokens
    pub context_window: Option<u32>,
    /// Whether the model supports tool/function calling
    pub supports_tools: bool,
}

/// Errors that can occur when fetching model lists.
#[derive(Debug, Clone, thiserror::Error)]
pub enum ModelListError {
    #[error("network error: {0}")]
    Network(String),

    #[error("auth failed (HTTP 401/403)")]
    AuthFailed,

    #[error("rate limited (HTTP 429); retry after {retry_after_secs:?}s")]
    RateLimited { retry_after_secs: Option<u64> },

    #[error("provider does not support model listing")]
    Unsupported,

    #[error("provider unreachable (offline)")]
    Offline,

    #[error("failed to parse provider response: {0}")]
    ParseFailed(String),

    #[error("operation cancelled")]
    Cancelled,
}

/// Trait for fetching available models from a provider.
#[async_trait::async_trait]
pub trait ModelListProvider: Send + Sync {
    /// Fetch the list of available models from this provider.
    async fn list_models(&self) -> Result<Vec<ModelDescriptor>, ModelListError>;

    /// Get the provider ID this implementation serves.
    fn provider_id(&self) -> ProviderId;
}

#[cfg(test)]
pub mod mock {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// Mock implementation of ModelListProvider for testing.
    pub struct MockModelListProvider {
        provider_id: ProviderId,
        response: Arc<Mutex<Result<Vec<ModelDescriptor>, ModelListError>>>,
    }

    impl MockModelListProvider {
        /// Create a new mock that returns success with the given models.
        pub fn new_success(provider_id: ProviderId, models: Vec<ModelDescriptor>) -> Self {
            Self {
                provider_id,
                response: Arc::new(Mutex::new(Ok(models))),
            }
        }

        /// Create a new mock that returns the given error.
        pub fn new_error(provider_id: ProviderId, error: ModelListError) -> Self {
            Self {
                provider_id,
                response: Arc::new(Mutex::new(Err(error))),
            }
        }

        /// Update the response this mock will return.
        pub fn set_response(&self, response: Result<Vec<ModelDescriptor>, ModelListError>) {
            *self.response.lock().expect("lock poisoned") = response;
        }
    }

    #[async_trait::async_trait]
    impl ModelListProvider for MockModelListProvider {
        async fn list_models(&self) -> Result<Vec<ModelDescriptor>, ModelListError> {
            self.response.lock().expect("lock poisoned").clone()
        }

        fn provider_id(&self) -> ProviderId {
            self.provider_id
        }
    }
}
