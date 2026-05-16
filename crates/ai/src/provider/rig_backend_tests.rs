use super::{RigBackendConfig, RigProviderKind};

#[test]
fn rig_backend_config_maps_openrouter() {
    let config = RigBackendConfig::new(
        RigProviderKind::OpenRouter,
        "moonshotai/kimi-k2.6",
        Some("test-key".to_string()),
        Some("https://openrouter.ai/api/v1".to_string()),
    );

    assert_eq!(config.provider_kind, RigProviderKind::OpenRouter);
    assert_eq!(config.model_id, "moonshotai/kimi-k2.6");
}

#[test]
fn rig_backend_config_rejects_missing_key_for_openrouter() {
    let err = RigBackendConfig::new(
        RigProviderKind::OpenRouter,
        "moonshotai/kimi-k2.6",
        None,
        Some("https://openrouter.ai/api/v1".to_string()),
    )
    .validate()
    .unwrap_err();

    assert!(err.to_string().contains("requires an API key"));
}

#[test]
fn rig_backend_config_rejects_empty_model_id() {
    let err = RigBackendConfig::new(RigProviderKind::Ollama, "  ", None, None)
        .validate()
        .unwrap_err();

    assert!(err.to_string().contains("requires a model"));
}

#[test]
fn rig_backend_config_allows_ollama_without_api_key() {
    RigBackendConfig::new(RigProviderKind::Ollama, "llama3.2", None, None)
        .validate()
        .unwrap();
}

#[test]
fn rig_backend_config_rejects_custom_endpoint_without_base_url() {
    let err = RigBackendConfig::new(
        RigProviderKind::CustomOpenAICompatible,
        "custom-model",
        Some("test-key".to_string()),
        None,
    )
    .validate()
    .unwrap_err();

    assert!(err.to_string().contains("requires a base URL"));
}
