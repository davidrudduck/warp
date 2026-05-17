//! OpenRouter provider implementation using raw reqwest.

use crate::model_registry::{ModelDescriptor, ModelListError, ModelListProvider, ProviderId};
use reqwest::Client;

/// Provider for OpenRouter (https://openrouter.ai/api/v1/models)
pub struct OpenRouterListProvider {
    api_key: String,
    base_url: String,
    client: Client,
}

impl OpenRouterListProvider {
    /// Create a new OpenRouter provider.
    pub fn new(api_key: String, base_url: Option<String>) -> Self {
        Self {
            api_key,
            base_url: base_url.unwrap_or_else(|| "https://openrouter.ai/api/v1".to_string()),
            client: Client::new(),
        }
    }
}

#[async_trait::async_trait]
impl ModelListProvider for OpenRouterListProvider {
    async fn list_models(&self) -> Result<Vec<ModelDescriptor>, ModelListError> {
        let url = format!("{}/models", self.base_url);

        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() || e.is_connect() {
                    ModelListError::Offline
                } else {
                    ModelListError::Network(e.to_string())
                }
            })?;

        let status = response.status();
        if status == 401 {
            return Err(ModelListError::AuthFailed);
        }
        if status == 429 {
            let retry_after = response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok());
            return Err(ModelListError::RateLimited {
                retry_after_secs: retry_after,
            });
        }
        if !status.is_success() {
            return Err(ModelListError::Network(format!("HTTP {status}")));
        }

        let body = response
            .text()
            .await
            .map_err(|e| ModelListError::Network(e.to_string()))?;

        // Parse OpenRouter response: { data: [{ id, name, context_length, ... }] }
        #[derive(serde::Deserialize)]
        struct OpenRouterResponse {
            data: Vec<OpenRouterModel>,
        }

        #[derive(serde::Deserialize)]
        struct OpenRouterModel {
            id: String,
            #[serde(default)]
            name: Option<String>,
            #[serde(default)]
            context_length: Option<u32>,
        }

        let parsed: OpenRouterResponse =
            serde_json::from_str(&body).map_err(|e| ModelListError::ParseFailed(e.to_string()))?;

        let descriptors = parsed
            .data
            .into_iter()
            .map(|m| ModelDescriptor {
                id: m.id,
                display_name: m.name,
                context_window: m.context_length,
                supports_tools: true, // Assume most OpenRouter models support tools
            })
            .collect();

        Ok(descriptors)
    }

    fn provider_id(&self) -> ProviderId {
        ProviderId::OpenRouter
    }
}
