//! Custom provider implementation for OpenAI-compatible endpoints.

use crate::model_registry::{ModelDescriptor, ModelListError, ModelListProvider, ProviderId};
use crate::url_validation::{normalize_openai_compatible_base_url, BaseUrlValidationError};
use reqwest::Client;

/// Provider for custom OpenAI-compatible endpoints.
pub struct CustomListProvider {
    api_key: Option<String>,
    base_url: String,
    client: Client,
}

impl CustomListProvider {
    /// Create a new custom provider.
    pub fn new(base_url: String, api_key: Option<String>) -> Result<Self, BaseUrlValidationError> {
        let base_url = normalize_openai_compatible_base_url(&base_url)?;
        Ok(Self {
            api_key,
            base_url,
            client: Client::new(),
        })
    }
}

#[async_trait::async_trait]
impl ModelListProvider for CustomListProvider {
    async fn list_models(&self) -> Result<Vec<ModelDescriptor>, ModelListError> {
        let url = format!("{}/v1/models", self.base_url);

        let mut request = self.client.get(&url);

        if let Some(key) = &self.api_key {
            request = request.header("Authorization", format!("Bearer {key}"));
        }

        let response = request.send().await.map_err(|e| {
            if e.is_timeout() || e.is_connect() {
                ModelListError::Offline
            } else {
                ModelListError::Network(e.to_string())
            }
        })?;

        let status = response.status();
        if status == 401 || status == 403 {
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

        // Parse OpenAI-compatible response: { data: [{ id, ... }] }
        #[derive(serde::Deserialize)]
        struct CustomResponse {
            data: Vec<CustomModel>,
        }

        #[derive(serde::Deserialize)]
        struct CustomModel {
            id: String,
        }

        let parsed: CustomResponse =
            serde_json::from_str(&body).map_err(|_e| ModelListError::Unsupported)?;

        let descriptors = parsed
            .data
            .into_iter()
            .map(|m| ModelDescriptor {
                id: m.id,
                display_name: None,
                context_window: None,
                supports_tools: false, // Unknown for custom endpoints
            })
            .collect();

        Ok(descriptors)
    }

    fn provider_id(&self) -> ProviderId {
        ProviderId::Custom
    }
}
