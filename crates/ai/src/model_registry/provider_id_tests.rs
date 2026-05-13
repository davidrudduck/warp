use super::*;

#[test]
fn as_genai_adapter_kind_returns_some_for_genai_providers() {
    assert_eq!(
        ProviderId::OpenAI.as_genai_adapter_kind(),
        Some(genai::adapter::AdapterKind::OpenAI)
    );
    assert_eq!(
        ProviderId::Anthropic.as_genai_adapter_kind(),
        Some(genai::adapter::AdapterKind::Anthropic)
    );
    assert_eq!(
        ProviderId::GoogleGemini.as_genai_adapter_kind(),
        Some(genai::adapter::AdapterKind::Gemini)
    );
    assert_eq!(
        ProviderId::Ollama.as_genai_adapter_kind(),
        Some(genai::adapter::AdapterKind::Ollama)
    );
}

#[test]
fn as_genai_adapter_kind_returns_none_for_non_genai_providers() {
    assert_eq!(ProviderId::OpenRouter.as_genai_adapter_kind(), None);
    assert_eq!(ProviderId::Custom.as_genai_adapter_kind(), None);
}

#[test]
fn from_provider_type_str_roundtrips() {
    let providers = [
        ProviderId::OpenAI,
        ProviderId::Anthropic,
        ProviderId::GoogleGemini,
        ProviderId::Ollama,
        ProviderId::OpenRouter,
        ProviderId::Custom,
    ];

    for provider in providers {
        let display_name = provider.display_name();
        let parsed = ProviderId::from_provider_type_str(display_name);
        assert_eq!(parsed, Some(provider), "Failed to roundtrip {display_name}");
    }
}

#[test]
fn from_provider_type_str_rejects_invalid() {
    assert_eq!(ProviderId::from_provider_type_str(""), None);
    assert_eq!(ProviderId::from_provider_type_str("InvalidProvider"), None);
    assert_eq!(ProviderId::from_provider_type_str("openai"), None); // case-sensitive
    assert_eq!(ProviderId::from_provider_type_str("Custom"), None); // missing suffix
}
