use super::*;
use warp_core::settings::{
    DirectAPISettings, PrivatePreferences, PublicPreferences, SettingsManager,
};
use warpui::App;
use warpui_extras::user_preferences;
use warpui_extras::user_preferences::toml_backed::TomlBackedUserPreferences;

fn register_settings(app: &mut App) {
    app.update(|ctx| {
        ctx.add_singleton_model(|_| {
            PublicPreferences::new(
                Box::<user_preferences::in_memory::InMemoryPreferences>::default(),
            )
        });
        ctx.add_singleton_model(|_| {
            PrivatePreferences::new(
                Box::<user_preferences::in_memory::InMemoryPreferences>::default(),
            )
        });
    });
    app.add_singleton_model(|_| SettingsManager::default());
    DirectAPISettings::register(app);
}

fn register_toml_settings(app: &mut App, settings_path: std::path::PathBuf) {
    app.update(|ctx| {
        ctx.add_singleton_model(move |_| {
            let (prefs, parse_error) = TomlBackedUserPreferences::new(settings_path);
            assert!(parse_error.is_none());
            PublicPreferences::new(Box::new(prefs))
        });
        ctx.add_singleton_model(|_| {
            PrivatePreferences::new(
                Box::<user_preferences::in_memory::InMemoryPreferences>::default(),
            )
        });
    });
    app.add_singleton_model(|_| SettingsManager::default());
    DirectAPISettings::register(app);
}

#[test]
fn api_key_manager_does_not_load_on_init() {
    App::test((), |app| async move {
        let manager = app.add_singleton_model(ApiKeyManager::new);

        manager.read(&app, |manager, _ctx| {
            // Verify no keys cached
            assert!(manager.is_cache_empty());
        });
    });
}

#[test]
fn api_key_manager_loads_on_first_keys_access() {
    App::test((), |mut app| async move {
        register_settings(&mut app);

        let manager = app.add_singleton_model(ApiKeyManager::new);

        manager.read(&app, |manager, ctx| {
            // First call triggers load from settings.
            let _keys = manager.keys(ctx);

            // Verify loaded (cache populated)
            assert!(!manager.is_cache_empty());
        });
    });
}

#[test]
fn api_key_manager_uses_cache_on_subsequent_calls() {
    App::test((), |mut app| async move {
        register_settings(&mut app);

        let manager = app.add_singleton_model(ApiKeyManager::new);

        manager.read(&app, |manager, ctx| {
            // First call loads
            let keys1 = manager.keys(ctx);

            // Second call uses cache (no storage access)
            let keys2 = manager.keys(ctx);

            assert_eq!(keys1.openai, keys2.openai);
            assert_eq!(keys1.anthropic, keys2.anthropic);
            assert_eq!(keys1.google, keys2.google);
            assert!(!manager.is_cache_empty());
        });
    });
}

#[test]
fn api_key_manager_cache_cleared_on_drop() {
    App::test((), |mut app| async move {
        register_settings(&mut app);

        {
            let manager = app.add_singleton_model(ApiKeyManager::new);
            manager.read(&app, |manager, ctx| {
                let _keys = manager.keys(ctx);
                assert!(!manager.is_cache_empty());
            });
        } // manager dropped when app scope ends
    });

    // New app/instance has no cache
    App::test((), |app| async move {
        let manager2 = app.add_singleton_model(ApiKeyManager::new);
        manager2.read(&app, |manager, _ctx| {
            assert!(manager.is_cache_empty());
        });
    });
}

#[test]
fn set_key_updates_cache_and_storage() {
    App::test((), |mut app| async move {
        register_settings(&mut app);

        let manager = app.add_singleton_model(ApiKeyManager::new);

        manager.update(&mut app, |manager, ctx| {
            // Set a key (should update cache + storage)
            manager.set_openai_key(Some("test-key".to_string()), ctx);

            // Cache should be populated
            assert!(!manager.is_cache_empty());

            // Key should be accessible
            assert_eq!(manager.keys(ctx).openai.as_deref(), Some("test-key"));
        });
    });
}

#[test]
fn parses_legacy_payload_without_new_fields() {
    let legacy_json = r#"{
        "google": "test-google-key",
        "anthropic": "test-anthropic-key",
        "openai": "test-openai-key",
        "open_router": "test-openrouter-key"
    }"#;

    let keys: ApiKeys = serde_json::from_str(legacy_json).expect("failed to parse legacy JSON");

    assert_eq!(keys.google, Some("test-google-key".to_string()));
    assert_eq!(keys.anthropic, Some("test-anthropic-key".to_string()));
    assert_eq!(keys.openai, Some("test-openai-key".to_string()));
    assert_eq!(keys.open_router, Some("test-openrouter-key".to_string()));
    assert_eq!(keys.custom, None);

    // New fields should default
    assert_eq!(keys.selected_provider, None);
    assert_eq!(keys.custom_base_url, None);
    assert_eq!(keys.openrouter_base_url, None);
    assert_eq!(keys.ollama_base_url, None);
    assert!(keys.selected_models.is_empty());
    assert!(keys.enabled_providers.is_empty());
}

#[test]
fn roundtrips_full_payload() {
    use crate::model_registry::ProviderId;
    use std::collections::BTreeMap;

    let mut selected_models = BTreeMap::new();
    selected_models.insert(ProviderId::OpenAI, "gpt-4o".to_string());
    selected_models.insert(
        ProviderId::Anthropic,
        "claude-3-5-sonnet-latest".to_string(),
    );
    let mut enabled_providers = BTreeMap::new();
    enabled_providers.insert(ProviderId::OpenAI, true);
    enabled_providers.insert(ProviderId::OpenRouter, false);

    let original = ApiKeys {
        google: Some("google-key".to_string()),
        anthropic: Some("anthropic-key".to_string()),
        openai: Some("openai-key".to_string()),
        open_router: Some("openrouter-key".to_string()),
        custom: Some("custom-key".to_string()),
        selected_provider: Some(ProviderId::OpenAI),
        custom_base_url: Some("https://custom.example.com".to_string()),
        openrouter_base_url: Some("https://openrouter.ai/api/v1".to_string()),
        ollama_base_url: Some("http://localhost:11434".to_string()),
        selected_models,
        enabled_providers,
    };

    let json = serde_json::to_string(&original).expect("failed to serialize");
    let roundtripped: ApiKeys = serde_json::from_str(&json).expect("failed to deserialize");

    assert_eq!(original, roundtripped);
}

#[test]
fn cache_invalidation_signal_emitted_when_api_key_changes() {
    use crate::model_registry::{ModelDescriptor, ModelListCache, ProviderId};
    use std::time::Duration;

    App::test((), |mut app| async move {
        register_settings(&mut app);

        // Create cache and populate it for OpenAI
        let cache = ModelListCache::new().expect("failed to create cache");
        let models = vec![ModelDescriptor {
            id: "test-model".to_string(),
            display_name: None,
            context_window: None,
            supports_tools: false,
        }];

        cache
            .set(ProviderId::OpenAI, models.clone())
            .expect("failed to set cache");

        // Verify cache is populated
        assert!(cache
            .get(ProviderId::OpenAI, Duration::from_secs(60))
            .is_some());

        // Create ApiKeyManager and change OpenAI key
        let manager = app.add_singleton_model(ApiKeyManager::new);
        manager.update(&mut app, |manager, ctx| {
            manager.set_openai_key(Some("new-key".to_string()), ctx);
        });

        // Verify cache is now empty for OpenAI provider (invalidated by set_openai_key)
        assert!(cache
            .get(ProviderId::OpenAI, Duration::from_secs(60))
            .is_none());
    });
}

#[test]
fn get_selected_model_returns_user_selection_when_set() {
    use crate::model_registry::ProviderId;

    App::test((), |mut app| async move {
        register_settings(&mut app);

        let manager = app.add_singleton_model(ApiKeyManager::new);

        // Set a custom model selection
        manager.update(&mut app, |manager, ctx| {
            manager.set_selected_model(ProviderId::OpenAI, "gpt-4-turbo".to_string(), ctx);
        });

        // Verify the selected model is returned
        manager.read(&app, |manager, ctx| {
            assert_eq!(
                manager.get_selected_model_for_provider(ProviderId::OpenAI, ctx),
                Some("gpt-4-turbo".to_string())
            );
        });
    });
}

#[test]
fn get_selected_model_falls_back_to_defaults() {
    use crate::model_registry::ProviderId;

    App::test((), |mut app| async move {
        register_settings(&mut app);

        let manager = app.add_singleton_model(ApiKeyManager::new);

        manager.read(&app, |manager, ctx| {
            // Providers with defaults
            assert_eq!(
                manager.get_selected_model_for_provider(ProviderId::OpenAI, ctx),
                Some("gpt-4o-mini".to_string())
            );
            assert_eq!(
                manager.get_selected_model_for_provider(ProviderId::Anthropic, ctx),
                Some("claude-3-5-sonnet-20241022".to_string())
            );
            assert_eq!(
                manager.get_selected_model_for_provider(ProviderId::GoogleGemini, ctx),
                Some("gemini-2.0-flash".to_string())
            );

            // Providers without defaults
            assert_eq!(
                manager.get_selected_model_for_provider(ProviderId::Ollama, ctx),
                None
            );
            assert_eq!(
                manager.get_selected_model_for_provider(ProviderId::OpenRouter, ctx),
                None
            );
            assert_eq!(
                manager.get_selected_model_for_provider(ProviderId::Custom, ctx),
                None
            );
        });
    });
}

#[test]
fn direct_api_configuration_writes_to_settings_without_secure_storage() {
    use crate::model_registry::ProviderId;
    use warp_core::settings::Setting;
    use warpui::SingletonEntity;

    let _guard = warp_core::features::FeatureFlag::SettingsFile.override_enabled(true);
    let dir = tempfile::tempdir().unwrap();
    let settings_path = dir.path().join("settings.toml");

    App::test((), |mut app| async move {
        register_toml_settings(&mut app, settings_path.clone());

        let manager = app.add_singleton_model(ApiKeyManager::new);

        manager.update(&mut app, |manager, ctx| {
            manager.set_selected_provider(Some(ProviderId::Custom), ctx);
            manager.set_custom_key(Some("custom-key".to_string()), ctx);
            manager.set_custom_base_url(Some("https://api.example.com/v1".to_string()), ctx);
            manager.set_openrouter_base_url(Some("https://openrouter.example/v1".to_string()), ctx);
            manager.set_ollama_base_url(Some("http://localhost:11434".to_string()), ctx);
            manager.set_provider_enabled(ProviderId::Custom, true, ctx);
            manager.set_provider_enabled(ProviderId::OpenRouter, false, ctx);
        });

        app.read(|ctx| {
            let settings = DirectAPISettings::as_ref(ctx);
            assert_eq!(
                settings.selected_provider.value().as_deref(),
                Some("Custom")
            );
            assert_eq!(
                settings.api_key_custom.value().as_deref(),
                Some("custom-key")
            );
            assert_eq!(
                settings.base_url_custom.value().as_deref(),
                Some("https://api.example.com/v1")
            );
            assert_eq!(
                settings.base_url_openrouter.value().as_deref(),
                Some("https://openrouter.example/v1")
            );
            assert_eq!(
                settings.base_url_ollama.value().as_deref(),
                Some("http://localhost:11434")
            );
        });

        let settings_toml =
            std::fs::read_to_string(&settings_path).expect("failed to read settings TOML");
        assert!(settings_toml.contains("[agents.direct_api.api_keys]"));
        assert!(settings_toml.contains("custom = \"custom-key\""));
        assert!(settings_toml.contains("[agents.direct_api]"));
        assert!(settings_toml.contains("selected_provider = \"Custom\""));
        assert!(settings_toml.contains("[agents.direct_api.base_urls]"));
        assert!(settings_toml.contains("custom = \"https://api.example.com/v1\""));
        assert!(settings_toml.contains("openrouter = \"https://openrouter.example/v1\""));
        assert!(settings_toml.contains("ollama = \"http://localhost:11434\""));
        assert!(settings_toml.contains("[agents.direct_api.enabled_providers]"));
        assert!(settings_toml.contains("Custom = true"));
        assert!(settings_toml.contains("OpenRouter = false"));

        manager.read(&app, |manager, ctx| {
            let keys = manager.keys(ctx);
            assert_eq!(keys.selected_provider, Some(ProviderId::Custom));
            assert_eq!(keys.custom.as_deref(), Some("custom-key"));
            assert_eq!(
                keys.custom_base_url.as_deref(),
                Some("https://api.example.com/v1")
            );
            assert_eq!(
                keys.openrouter_base_url.as_deref(),
                Some("https://openrouter.example/v1")
            );
            assert_eq!(
                keys.ollama_base_url.as_deref(),
                Some("http://localhost:11434")
            );
            assert_eq!(keys.enabled_providers.get(&ProviderId::Custom), Some(&true));
            assert_eq!(
                keys.enabled_providers.get(&ProviderId::OpenRouter),
                Some(&false)
            );
        });
    });
}
