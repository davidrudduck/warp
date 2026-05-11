use super::{visibility_tooltip, ProviderType};

#[test]
fn api_key_placeholder_for_each_provider() {
    assert_eq!(ProviderType::OpenAI.api_key_placeholder(), "sk-...");
    assert_eq!(ProviderType::Anthropic.api_key_placeholder(), "sk-ant-...");
    assert_eq!(ProviderType::GoogleGemini.api_key_placeholder(), "AIza...");
    assert_eq!(ProviderType::Ollama.api_key_placeholder(), "Optional");
    assert_eq!(ProviderType::OpenRouter.api_key_placeholder(), "sk-or-...");
    assert_eq!(ProviderType::Custom.api_key_placeholder(), "Optional");
}

#[test]
fn base_url_placeholder_for_each_provider() {
    assert_eq!(ProviderType::OpenAI.base_url_placeholder(), "");
    assert_eq!(ProviderType::Anthropic.base_url_placeholder(), "");
    assert_eq!(ProviderType::GoogleGemini.base_url_placeholder(), "");
    assert_eq!(
        ProviderType::Ollama.base_url_placeholder(),
        "http://localhost:11434"
    );
    assert_eq!(
        ProviderType::OpenRouter.base_url_placeholder(),
        "https://openrouter.ai/api/v1"
    );
    assert_eq!(
        ProviderType::Custom.base_url_placeholder(),
        "https://api.example.com/v1"
    );
}

#[test]
fn default_base_url_only_prefilled_for_known_endpoints() {
    assert_eq!(
        ProviderType::Ollama.default_base_url(),
        "http://localhost:11434"
    );
    assert_eq!(
        ProviderType::OpenRouter.default_base_url(),
        "https://openrouter.ai/api/v1"
    );
    assert_eq!(ProviderType::Custom.default_base_url(), "");
}

#[test]
fn needs_base_url_only_for_ollama_openrouter_custom() {
    assert!(!ProviderType::OpenAI.needs_base_url());
    assert!(!ProviderType::Anthropic.needs_base_url());
    assert!(!ProviderType::GoogleGemini.needs_base_url());
    assert!(ProviderType::Ollama.needs_base_url());
    assert!(ProviderType::OpenRouter.needs_base_url());
    assert!(ProviderType::Custom.needs_base_url());
}

#[test]
fn from_str_as_str_roundtrip_for_each_provider() {
    for provider in ProviderType::all() {
        let label = provider.as_str();
        assert_eq!(
            ProviderType::from_str(label),
            Some(provider.clone()),
            "round-trip failed for {label}"
        );
    }
}

#[test]
fn from_str_returns_none_for_unknown_label() {
    assert_eq!(ProviderType::from_str(""), None);
    assert_eq!(ProviderType::from_str("Cohere"), None);
    assert_eq!(ProviderType::from_str("openai"), None); // case-sensitive
}

#[test]
fn validate_api_key_openai_requires_sk_prefix() {
    assert!(ProviderType::OpenAI.validate_api_key("sk-abc123").is_ok());
    assert_eq!(
        ProviderType::OpenAI.validate_api_key("").unwrap_err(),
        "OpenAI API key cannot be empty"
    );
    assert_eq!(
        ProviderType::OpenAI
            .validate_api_key("not-a-key")
            .unwrap_err(),
        "OpenAI API keys should start with 'sk-'"
    );
}

#[test]
fn validate_api_key_anthropic_requires_sk_ant_prefix() {
    assert!(ProviderType::Anthropic
        .validate_api_key("sk-ant-abc123")
        .is_ok());
    assert_eq!(
        ProviderType::Anthropic.validate_api_key("").unwrap_err(),
        "Anthropic API key cannot be empty"
    );
    // An OpenAI-shaped key should not pass Anthropic validation.
    assert_eq!(
        ProviderType::Anthropic
            .validate_api_key("sk-only")
            .unwrap_err(),
        "Anthropic API keys should start with 'sk-ant-'"
    );
}

#[test]
fn validate_api_key_google_gemini_requires_non_empty() {
    assert!(ProviderType::GoogleGemini
        .validate_api_key("AIzaSyAnything")
        .is_ok());
    assert_eq!(
        ProviderType::GoogleGemini.validate_api_key("").unwrap_err(),
        "Google Gemini API key cannot be empty"
    );
}

#[test]
fn validate_api_key_openrouter_requires_non_empty() {
    assert!(ProviderType::OpenRouter
        .validate_api_key("sk-or-anything")
        .is_ok());
    assert_eq!(
        ProviderType::OpenRouter.validate_api_key("").unwrap_err(),
        "OpenRouter API key cannot be empty"
    );
}

#[test]
fn validate_api_key_optional_for_ollama_and_custom() {
    assert!(ProviderType::Ollama.validate_api_key("").is_ok());
    assert!(ProviderType::Ollama.validate_api_key("anything").is_ok());
    assert!(ProviderType::Custom.validate_api_key("").is_ok());
    assert!(ProviderType::Custom.validate_api_key("anything").is_ok());
}

#[test]
fn visibility_tooltip_reflects_show_state() {
    assert_eq!(visibility_tooltip(false), "Show API key");
    assert_eq!(visibility_tooltip(true), "Hide API key");
}
