#[cfg(feature = "direct_api_rig_backend")]
use ai::model_registry::ProviderId;
use ai::provider::ChatStream;

#[cfg(feature = "direct_api_rig_backend")]
use super::DirectApiRouteConfig;
use super::RequestParams;
#[cfg(feature = "direct_api_rig_backend")]
use ai::url_validation::openai_compatible_base_url_with_v1;

#[cfg(feature = "direct_api_rig_backend")]
pub async fn run_rig_provider_stream(params: RequestParams) -> anyhow::Result<ChatStream> {
    let config = params
        .direct_api_route_config
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Direct API route config missing"))?;
    let request = super::direct_tools::build_chat_request(&params);
    let rig_config = rig_config_from_direct_api_config(config)?;
    ai::provider::rig_backend::RigDirectBackend::new(rig_config)?
        .stream_turn(request)
        .await
}

#[cfg(not(feature = "direct_api_rig_backend"))]
pub async fn run_rig_provider_stream(_params: RequestParams) -> anyhow::Result<ChatStream> {
    anyhow::bail!("Rig Direct API backend is not available in this build")
}

#[cfg(feature = "direct_api_rig_backend")]
fn rig_config_from_direct_api_config(
    config: &DirectApiRouteConfig,
) -> anyhow::Result<ai::provider::rig_backend::RigBackendConfig> {
    use ai::provider::rig_backend::{RigBackendConfig, RigProviderKind};

    let provider_kind = match config.provider_id {
        ProviderId::OpenAI => RigProviderKind::OpenAI,
        ProviderId::Anthropic => RigProviderKind::Anthropic,
        ProviderId::GoogleGemini => RigProviderKind::GoogleGemini,
        ProviderId::Ollama => RigProviderKind::Ollama,
        ProviderId::OpenRouter => RigProviderKind::OpenRouter,
        ProviderId::Custom => RigProviderKind::CustomOpenAICompatible,
    };

    Ok(RigBackendConfig::new(
        provider_kind,
        config.model_id.clone(),
        config.api_key.clone(),
        rig_base_url_from_direct_api_config(config),
    ))
}

#[cfg(feature = "direct_api_rig_backend")]
fn rig_base_url_from_direct_api_config(config: &DirectApiRouteConfig) -> Option<String> {
    match config.provider_id {
        ProviderId::Custom => config
            .base_url
            .as_deref()
            .map(openai_compatible_base_url_with_v1),
        ProviderId::OpenAI
        | ProviderId::Anthropic
        | ProviderId::GoogleGemini
        | ProviderId::Ollama
        | ProviderId::OpenRouter => config.base_url.clone(),
    }
}

#[cfg(all(test, feature = "direct_api_rig_backend"))]
mod tests {
    use super::*;

    #[test]
    fn rig_custom_openai_compatible_base_url_matches_native_v1_behavior() {
        let config = DirectApiRouteConfig {
            provider_id: ProviderId::Custom,
            model_id: "custom-model".to_string(),
            api_key: Some("sk-test".to_string()),
            base_url: Some("https://example.test".to_string()),
        };

        let rig_config = rig_config_from_direct_api_config(&config).unwrap();

        assert_eq!(
            rig_config.base_url.as_deref(),
            Some("https://example.test/v1")
        );
    }

    #[test]
    fn rig_custom_openai_compatible_base_url_does_not_duplicate_v1() {
        let config = DirectApiRouteConfig {
            provider_id: ProviderId::Custom,
            model_id: "custom-model".to_string(),
            api_key: Some("sk-test".to_string()),
            base_url: Some("https://example.test/v1/".to_string()),
        };

        let rig_config = rig_config_from_direct_api_config(&config).unwrap();

        assert_eq!(
            rig_config.base_url.as_deref(),
            Some("https://example.test/v1")
        );
    }
}
