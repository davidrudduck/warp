use super::*;
use crate::ai::agent::api::DirectApiRouteConfig;

#[test]
fn openrouter_provider_config_uses_openrouter_adapter_label_and_base_url() {
    let config = DirectApiRouteConfig {
        provider_id: ProviderId::OpenRouter,
        model_id: "moonshotai/kimi-k2.6".to_string(),
        api_key: Some("sk-or-v1-test".to_string()),
        base_url: Some("https://openrouter.ai/api/v1".to_string()),
    };

    let provider = provider_for_config(&config);

    assert_eq!(provider.diagnostic_provider_label(), "openrouter");
    assert_eq!(
        provider.diagnostic_base_url(),
        Some("https://openrouter.ai/api/v1/")
    );
}
