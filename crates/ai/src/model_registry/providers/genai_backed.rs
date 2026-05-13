//! GenaiBackedListProvider wraps genai-supported providers for OpenAI/Anthropic/Gemini/Ollama.
//!
//! While `genai::Client::all_model_names` exists, it cannot be configured to use a
//! custom endpoint (it always resolves the adapter's default endpoint). To support
//! both Ollama's configurable host and mockito-driven unit tests, this provider
//! issues the underlying HTTP request directly via reqwest, mirroring the URLs and
//! authentication that each genai adapter uses internally.
//!
//! Returned model IDs are enriched against `known_capabilities` when present, with
//! a static curated fallback used when the provider returns an empty list.

use crate::model_registry::{
    known_capabilities::known_capabilities, ModelDescriptor, ModelListError, ModelListProvider,
    ProviderId,
};
use reqwest::Client;

/// Provider implementation that mirrors `genai::Client::all_model_names` for the
/// adapters genai natively supports: OpenAI, Anthropic, Google Gemini, and Ollama.
#[derive(Debug)]
pub struct GenaiBackedListProvider {
    provider_id: ProviderId,
    api_key: Option<String>,
    base_url: String,
    client: Client,
}

impl GenaiBackedListProvider {
    /// Create a new provider for the given provider ID.
    ///
    /// # Arguments
    /// * `provider_id` - Must be one of OpenAI, Anthropic, GoogleGemini, Ollama
    /// * `api_key` - Optional API key (required for remote providers, optional for Ollama)
    /// * `base_url` - Optional custom base URL (primarily for Ollama, but useful for
    ///   self-hosted gateways or test harnesses)
    pub fn new(
        provider_id: ProviderId,
        api_key: Option<String>,
        base_url: Option<String>,
    ) -> Result<Self, ModelListError> {
        // Ensure the provider is one that genai natively supports.
        provider_id
            .as_genai_adapter_kind()
            .ok_or(ModelListError::Unsupported)?;

        let base_url = base_url.unwrap_or_else(|| Self::default_base_url(provider_id).to_string());

        Ok(Self {
            provider_id,
            api_key,
            base_url,
            client: Client::new(),
        })
    }

    /// Default base URL for each genai-backed provider. These mirror the defaults
    /// used by the respective genai adapters.
    fn default_base_url(provider_id: ProviderId) -> &'static str {
        match provider_id {
            ProviderId::OpenAI => "https://api.openai.com/v1",
            ProviderId::Anthropic => "https://api.anthropic.com/v1",
            ProviderId::GoogleGemini => "https://generativelanguage.googleapis.com/v1beta",
            ProviderId::Ollama => "http://localhost:11434",
            ProviderId::OpenRouter => "",
            ProviderId::Custom => "",
        }
    }

    /// Static fallback models for offline / empty-response scenarios.
    fn fallback_models(provider_id: ProviderId) -> Vec<ModelDescriptor> {
        match provider_id {
            ProviderId::OpenAI => vec![
                ModelDescriptor {
                    id: "gpt-4o".to_string(),
                    display_name: Some("GPT-4o".to_string()),
                    context_window: Some(128_000),
                    supports_tools: true,
                },
                ModelDescriptor {
                    id: "gpt-4o-mini".to_string(),
                    display_name: Some("GPT-4o Mini".to_string()),
                    context_window: Some(128_000),
                    supports_tools: true,
                },
            ],
            ProviderId::Anthropic => vec![
                ModelDescriptor {
                    id: "claude-3-5-sonnet-latest".to_string(),
                    display_name: Some("Claude 3.5 Sonnet".to_string()),
                    context_window: Some(200_000),
                    supports_tools: true,
                },
                ModelDescriptor {
                    id: "claude-3-opus-latest".to_string(),
                    display_name: Some("Claude 3 Opus".to_string()),
                    context_window: Some(200_000),
                    supports_tools: true,
                },
            ],
            ProviderId::GoogleGemini => vec![
                ModelDescriptor {
                    id: "gemini-2.0-flash".to_string(),
                    display_name: Some("Gemini 2.0 Flash".to_string()),
                    context_window: Some(1_000_000),
                    supports_tools: true,
                },
                ModelDescriptor {
                    id: "gemini-1.5-pro".to_string(),
                    display_name: Some("Gemini 1.5 Pro".to_string()),
                    context_window: Some(2_000_000),
                    supports_tools: true,
                },
            ],
            ProviderId::Ollama => vec![ModelDescriptor {
                id: "llama3:latest".to_string(),
                display_name: Some("Llama 3".to_string()),
                context_window: Some(8_192),
                supports_tools: false,
            }],
            ProviderId::OpenRouter | ProviderId::Custom => vec![],
        }
    }

    /// Enrich a raw model ID into a `ModelDescriptor`, consulting the static
    /// `known_capabilities` table when an entry exists.
    fn enrich_model(id: String) -> ModelDescriptor {
        let known = known_capabilities();
        if let Some(caps) = known.get(id.as_str()) {
            ModelDescriptor {
                id: id.clone(),
                display_name: None,
                context_window: Some(caps.context_window),
                supports_tools: caps.supports_tools,
            }
        } else {
            ModelDescriptor {
                id,
                display_name: None,
                context_window: None,
                supports_tools: true,
            }
        }
    }

    /// Build the listing URL for the given provider's API.
    fn list_url(&self) -> String {
        match self.provider_id {
            ProviderId::Ollama => format!("{}/api/tags", self.base_url),
            ProviderId::GoogleGemini => {
                let key = self.api_key.clone().unwrap_or_default();
                format!("{}/models?key={key}", self.base_url)
            }
            ProviderId::OpenAI | ProviderId::Anthropic => format!("{}/models", self.base_url),
            ProviderId::OpenRouter | ProviderId::Custom => format!("{}/models", self.base_url),
        }
    }

    /// Apply authentication headers required by each provider's API.
    fn apply_auth(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match self.provider_id {
            ProviderId::OpenAI => match &self.api_key {
                Some(key) => request.header("Authorization", format!("Bearer {key}")),
                None => request,
            },
            ProviderId::Anthropic => {
                let mut req = request.header("anthropic-version", "2023-06-01");
                if let Some(key) = &self.api_key {
                    req = req.header("x-api-key", key.clone());
                }
                req
            }
            ProviderId::GoogleGemini => request,
            ProviderId::Ollama => request,
            ProviderId::OpenRouter | ProviderId::Custom => request,
        }
    }

    /// Parse the provider-specific response payload into raw model IDs.
    fn parse_ids(provider_id: ProviderId, body: &str) -> Result<Vec<String>, ModelListError> {
        match provider_id {
            ProviderId::OpenAI | ProviderId::Anthropic => {
                #[derive(serde::Deserialize)]
                struct OpenAILikeResponse {
                    data: Vec<OpenAILikeModel>,
                }

                #[derive(serde::Deserialize)]
                struct OpenAILikeModel {
                    id: String,
                }

                let parsed: OpenAILikeResponse = serde_json::from_str(body)
                    .map_err(|e| ModelListError::ParseFailed(e.to_string()))?;
                Ok(parsed.data.into_iter().map(|m| m.id).collect())
            }
            ProviderId::GoogleGemini => {
                #[derive(serde::Deserialize)]
                struct GeminiResponse {
                    #[serde(default)]
                    models: Vec<GeminiModel>,
                }

                #[derive(serde::Deserialize)]
                struct GeminiModel {
                    name: String,
                }

                let parsed: GeminiResponse = serde_json::from_str(body)
                    .map_err(|e| ModelListError::ParseFailed(e.to_string()))?;
                Ok(parsed
                    .models
                    .into_iter()
                    .map(|m| {
                        // Gemini returns "models/gemini-2.0-flash"; strip the prefix
                        // to match the IDs callers and `known_capabilities` use.
                        m.name
                            .strip_prefix("models/")
                            .map(|s| s.to_string())
                            .unwrap_or(m.name)
                    })
                    .collect())
            }
            ProviderId::Ollama => {
                #[derive(serde::Deserialize)]
                struct OllamaResponse {
                    #[serde(default)]
                    models: Vec<OllamaModel>,
                }

                #[derive(serde::Deserialize)]
                struct OllamaModel {
                    name: String,
                }

                let parsed: OllamaResponse = serde_json::from_str(body)
                    .map_err(|e| ModelListError::ParseFailed(e.to_string()))?;
                Ok(parsed.models.into_iter().map(|m| m.name).collect())
            }
            ProviderId::OpenRouter | ProviderId::Custom => Err(ModelListError::Unsupported),
        }
    }
}

#[async_trait::async_trait]
impl ModelListProvider for GenaiBackedListProvider {
    async fn list_models(&self) -> Result<Vec<ModelDescriptor>, ModelListError> {
        let url = self.list_url();
        let request = self.apply_auth(self.client.get(&url));

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

        let ids = Self::parse_ids(self.provider_id, &body)?;

        if ids.is_empty() {
            return Ok(Self::fallback_models(self.provider_id));
        }

        Ok(ids.into_iter().map(Self::enrich_model).collect())
    }

    fn provider_id(&self) -> ProviderId {
        self.provider_id
    }
}
