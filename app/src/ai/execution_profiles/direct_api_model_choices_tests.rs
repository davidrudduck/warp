use std::collections::BTreeMap;
use std::time::{Duration, SystemTime};

use ai::api_keys::ApiKeys;
use ai::model_registry::{CacheEntry, ModelDescriptor, ModelListCache, ProviderId};

use super::*;

fn descriptor(id: &str) -> ModelDescriptor {
    ModelDescriptor {
        id: id.to_string(),
        display_name: None,
        context_window: None,
        supports_tools: true,
    }
}

fn cache_path() -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "warp-direct-api-model-choices-{}.json",
        uuid::Uuid::new_v4()
    ))
}

#[test]
fn direct_api_choices_include_safe_default_model_without_cache() {
    let keys = ApiKeys {
        openai: Some("sk-test".to_string()),
        ..ApiKeys::default()
    };

    let choices = direct_api_model_choices_from_parts(&keys, None);

    assert_eq!(choices.len(), 1);
    assert_eq!(choices[0].label, "OpenAI / gpt-4o-mini");
    assert!(!choices[0].is_stale_or_manual);
}

#[test]
fn direct_api_choices_ignore_disabled_configured_provider() {
    let mut keys = ApiKeys {
        openai: Some("sk-test".to_string()),
        open_router: Some("sk-or-v1-test".to_string()),
        ..ApiKeys::default()
    };
    keys.enabled_providers.insert(ProviderId::OpenAI, false);
    keys.enabled_providers.insert(ProviderId::OpenRouter, true);
    keys.selected_models
        .insert(ProviderId::OpenRouter, "openrouter/model".to_string());

    let choices = direct_api_model_choices_from_parts(&keys, None);

    assert_eq!(
        choices
            .iter()
            .map(|choice| choice.label.as_str())
            .collect::<Vec<_>>(),
        vec!["OpenRouter / openrouter/model"]
    );
}

#[test]
fn direct_api_choices_include_saved_manual_model_without_cache() {
    let mut keys = ApiKeys {
        openai: Some("sk-test".to_string()),
        ..ApiKeys::default()
    };
    keys.selected_models
        .insert(ProviderId::OpenAI, "gpt-4o-mini".to_string());

    let choices = direct_api_model_choices_from_parts(&keys, None);

    assert_eq!(choices.len(), 1);
    assert_eq!(choices[0].label, "OpenAI / gpt-4o-mini");
    assert!(choices[0].is_stale_or_manual);
}

#[test]
fn direct_api_choices_include_cached_models_and_dedupe_manual_selection() {
    let path = cache_path();
    let cache = ModelListCache::new_with_path(path.clone()).expect("cache should initialize");
    cache
        .set(
            ProviderId::OpenAI,
            vec![descriptor("gpt-4o-mini"), descriptor("gpt-4.1-mini")],
        )
        .expect("cache should write");
    let mut keys = ApiKeys {
        openai: Some("sk-test".to_string()),
        ..ApiKeys::default()
    };
    keys.selected_models
        .insert(ProviderId::OpenAI, "gpt-4o-mini".to_string());

    let choices = direct_api_model_choices_from_parts(&keys, Some(&cache));

    assert_eq!(
        choices
            .iter()
            .map(|choice| choice.label.as_str())
            .collect::<Vec<_>>(),
        vec!["OpenAI / gpt-4o-mini", "OpenAI / gpt-4.1-mini"]
    );
    assert!(!choices[0].is_stale_or_manual);
    assert!(!choices[1].is_stale_or_manual);

    let _ = std::fs::remove_file(path);
}

#[test]
fn direct_api_choices_ignore_stale_cached_models_but_keep_safe_default() {
    let path = cache_path();
    let cache = ModelListCache::new_with_path(path.clone()).expect("cache should initialize");
    let mut entries = BTreeMap::new();
    entries.insert(
        ProviderId::OpenAI,
        CacheEntry {
            fetched_at: SystemTime::now() - Duration::from_secs(86_401),
            models: vec![descriptor("stale-model")],
        },
    );
    std::fs::write(
        &path,
        serde_json::to_string(&entries).expect("cache should serialize"),
    )
    .expect("cache should write");
    let keys = ApiKeys {
        openai: Some("sk-test".to_string()),
        ..ApiKeys::default()
    };

    let choices = direct_api_model_choices_from_parts(&keys, Some(&cache));

    assert_eq!(choices.len(), 1);
    assert_eq!(choices[0].label, "OpenAI / gpt-4o-mini");

    let _ = std::fs::remove_file(path);
}

#[test]
fn direct_api_choices_ignore_providers_without_key_or_base_url() {
    let keys = ApiKeys::default();

    let choices = direct_api_model_choices_from_parts(&keys, None);

    assert!(choices.is_empty());
}

#[test]
fn direct_api_choices_ignore_whitespace_only_key_or_base_url() {
    let keys = ApiKeys {
        openai: Some("   ".to_string()),
        custom_base_url: Some("   ".to_string()),
        ollama_base_url: Some("\n".to_string()),
        ..ApiKeys::default()
    };

    let choices = direct_api_model_choices_from_parts(&keys, None);

    assert!(choices.is_empty());
}

#[test]
fn direct_api_choices_allow_custom_and_ollama_with_base_urls() {
    let mut keys = ApiKeys {
        custom_base_url: Some("https://custom.example.com/v1".to_string()),
        ollama_base_url: Some("http://localhost:11434".to_string()),
        ..ApiKeys::default()
    };
    keys.selected_models
        .insert(ProviderId::Custom, "custom-model".to_string());
    keys.selected_models
        .insert(ProviderId::Ollama, "llama3.2".to_string());

    let choices = direct_api_model_choices_from_parts(&keys, None);

    assert_eq!(
        choices
            .iter()
            .map(|choice| choice.label.as_str())
            .collect::<Vec<_>>(),
        vec![
            "Ollama / llama3.2",
            "Custom (OpenAI-compatible) / custom-model"
        ]
    );
}

#[test]
fn direct_api_choices_allow_openrouter_with_key_without_cached_models() {
    let mut keys = ApiKeys {
        open_router: Some("sk-or-v1-test".to_string()),
        ..ApiKeys::default()
    };
    keys.selected_models
        .insert(ProviderId::OpenRouter, "openrouter/model".to_string());

    let choices = direct_api_model_choices_from_parts(&keys, None);

    assert_eq!(choices.len(), 1);
    assert_eq!(choices[0].label, "OpenRouter / openrouter/model");
}

#[test]
fn direct_api_choices_ignore_openrouter_key_with_invalid_prefix() {
    let mut keys = ApiKeys {
        open_router: Some("sk-or-test".to_string()),
        ..ApiKeys::default()
    };
    keys.selected_models
        .insert(ProviderId::OpenRouter, "openrouter/model".to_string());

    let choices = direct_api_model_choices_from_parts(&keys, None);

    assert!(choices.is_empty());
}
