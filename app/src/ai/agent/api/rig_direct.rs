#[cfg(feature = "direct_api_rig_backend")]
use ai::model_registry::ProviderId;
use ai::provider::ChatStream;

#[cfg(feature = "direct_api_rig_backend")]
use super::DirectApiRouteConfig;
use super::RequestParams;

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
        config.base_url.clone(),
    ))
}
