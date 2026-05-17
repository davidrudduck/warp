use super::{
    provider_is_enabled, provider_model_list_error_message, provider_model_list_success_message,
    provider_preflight_message, provider_status_label, validate_provider_base_url_preflight,
    visibility_tooltip, DirectApiPageAction, DirectApiSettingsPageView, ProviderType,
};
use crate::appearance::Appearance;
use crate::auth::AuthStateProvider;
use crate::server::telemetry::context_provider::AppTelemetryContextProvider;
use crate::settings_view::keybindings::KeybindingChangedNotifier;
use crate::test_util::settings::initialize_settings_for_tests;
use ai::api_keys::ApiKeys;
use ai::model_registry::{ModelDescriptor, ModelListCache, ModelListError, ProviderId};
use ai::url_validation::validate_direct_api_base_url;
use std::sync::Arc;
use warp_core::settings::{DirectAPISettings, Setting};
use warpui::platform::WindowStyle;
use warpui::{App, SingletonEntity as _, TypedActionView};

fn is_safe_for_http(url: &str) -> bool {
    validate_direct_api_base_url(url).is_ok()
}

#[test]
fn api_key_placeholder_for_each_provider() {
    assert_eq!(ProviderType::OpenAI.api_key_placeholder(), "sk-...");
    assert_eq!(ProviderType::Anthropic.api_key_placeholder(), "sk-ant-...");
    assert_eq!(ProviderType::GoogleGemini.api_key_placeholder(), "AIza...");
    assert_eq!(ProviderType::Ollama.api_key_placeholder(), "Optional");
    assert_eq!(
        ProviderType::OpenRouter.api_key_placeholder(),
        "sk-or-v1-..."
    );
    assert_eq!(ProviderType::Custom.api_key_placeholder(), "Optional");
}

#[test]
fn provider_row_primary_status_labels_are_short() {
    assert_eq!(ProviderType::OpenRouter.as_str(), "OpenRouter");
    assert_eq!(
        ProviderType::OpenRouter.api_key_placeholder(),
        "sk-or-v1-..."
    );
    assert_eq!(
        ProviderType::OpenRouter.default_base_url(),
        "https://openrouter.ai/api/v1"
    );
    assert_eq!(provider_status_label(true), "Enabled");
    assert_eq!(provider_status_label(false), "Disabled");
}

#[test]
fn provider_rows_keep_custom_last_for_scanability() {
    assert_eq!(ProviderType::all().last(), Some(&ProviderType::Custom));
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
            Some(provider),
            "round-trip failed for {label}"
        );
    }
}

#[test]
fn providers_are_alphabetical_with_custom_last() {
    assert_eq!(
        ProviderType::all(),
        vec![
            ProviderType::Anthropic,
            ProviderType::GoogleGemini,
            ProviderType::Ollama,
            ProviderType::OpenAI,
            ProviderType::OpenRouter,
            ProviderType::Custom,
        ]
    );
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
        .validate_api_key("sk-or-v1-anything")
        .is_ok());
    assert_eq!(
        ProviderType::OpenRouter.validate_api_key("").unwrap_err(),
        "OpenRouter API key cannot be empty"
    );
    assert_eq!(
        ProviderType::OpenRouter
            .validate_api_key("sk-not-openrouter")
            .unwrap_err(),
        "OpenRouter API keys should start with 'sk-or-v1-'"
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

#[test]
fn remote_provider_test_result_is_not_reported_as_validated_until_network_probe_runs() {
    assert_eq!(
        provider_preflight_message(ProviderType::OpenRouter),
        "API key format valid. Run Refresh models to validate provider access."
    );
    assert_eq!(
        provider_preflight_message(ProviderType::OpenAI),
        "API key format valid. Run Refresh models to validate provider access."
    );
    assert_eq!(
        provider_preflight_message(ProviderType::Custom),
        "Custom provider format valid. Run Refresh models to validate provider access."
    );
    assert!(!provider_preflight_message(ProviderType::OpenRouter).contains("full test pending"));
}

#[test]
fn openrouter_required_config_requires_current_key_prefix() {
    let mut keys = ApiKeys {
        open_router: Some("sk-or-invalid".to_string()),
        ..ApiKeys::default()
    };

    assert!(!provider_is_enabled(&keys, ProviderId::OpenRouter));

    keys.open_router = Some("sk-or-v1-valid".to_string());

    assert!(provider_is_enabled(&keys, ProviderId::OpenRouter));
}

#[test]
fn openrouter_explicit_enabled_still_requires_current_key_prefix() {
    let mut keys = ApiKeys {
        open_router: Some("sk-or-invalid".to_string()),
        ..ApiKeys::default()
    };
    keys.enabled_providers.insert(ProviderId::OpenRouter, true);

    assert!(!provider_is_enabled(&keys, ProviderId::OpenRouter));
}

#[test]
fn openrouter_enable_rejects_invalid_saved_key_prefix() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        DirectAPISettings::register(&mut app);
        app.add_singleton_model(|_| AuthStateProvider::new_logged_out_for_test());
        app.add_singleton_model(|_| Appearance::mock());
        app.add_singleton_model(|_| KeybindingChangedNotifier::mock());

        DirectAPISettings::handle(&app).update(&mut app, |settings, ctx| {
            settings
                .api_key_openrouter
                .set_value(Some("sk-or-invalid".to_string()), ctx)
                .expect("OpenRouter API key should save");
        });

        let (_window_id, view) =
            app.add_window(WindowStyle::NotStealFocus, DirectApiSettingsPageView::new);
        view.update(&mut app, |view, ctx| {
            view.handle_toggle_provider_enabled(ProviderType::OpenRouter, ctx);

            let row = view
                .provider_row(ProviderType::OpenRouter)
                .expect("OpenRouter row should exist");
            assert_eq!(
                row.test_result.borrow().as_ref(),
                Some(&Err(
                    "OpenRouter is missing required configuration".to_string()
                ))
            );
        });

        app.read(|ctx| {
            let settings = DirectAPISettings::as_ref(ctx);
            assert_ne!(
                settings
                    .enabled_providers
                    .value()
                    .get("OpenRouter")
                    .copied(),
                Some(true)
            );
        });
    });
}

#[test]
fn model_refresh_success_reports_provider_access_validated() {
    assert_eq!(
        provider_model_list_success_message(ProviderType::OpenRouter, 356),
        "OpenRouter access validated. Fetched 356 models."
    );
}

#[test]
fn model_refresh_auth_failure_reports_saved_key_rejection() {
    assert_eq!(
        provider_model_list_error_message(ModelListError::AuthFailed),
        "Provider rejected the saved API key."
    );
}

#[test]
fn provider_preflight_rejects_invalid_base_urls_before_claiming_success() {
    assert_eq!(
        validate_provider_base_url_preflight(ProviderType::OpenRouter, "http://openrouter.ai/v1")
            .unwrap_err(),
        "Base URL must use https://, except http:// localhost or private LAN addresses"
    );
    assert_eq!(
        validate_provider_base_url_preflight(ProviderType::Custom, "").unwrap_err(),
        "Base URL is required for custom providers"
    );
    assert!(
        validate_provider_base_url_preflight(ProviderType::Custom, "http://10.0.0.2:8080/v1")
            .is_ok()
    );
}

#[test]
fn rig_backend_toggle_defaults_off() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        DirectAPISettings::register(&mut app);
        app.add_singleton_model(|_| AuthStateProvider::new_logged_out_for_test());
        app.add_singleton_model(|_| Appearance::mock());
        app.add_singleton_model(|_| KeybindingChangedNotifier::mock());

        let (_window_id, view) =
            app.add_window(WindowStyle::NotStealFocus, DirectApiSettingsPageView::new);

        view.read(&app, |view, ctx| {
            assert!(!view.rig_backend_enabled(ctx));
        });
    });
}

#[cfg(feature = "direct_api_rig_backend")]
#[test]
fn rig_backend_toggle_persists_setting() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        DirectAPISettings::register(&mut app);
        app.add_singleton_model(|_| AuthStateProvider::new_logged_out_for_test());
        app.add_singleton_model(|_| Appearance::mock());
        app.add_singleton_model(|_| KeybindingChangedNotifier::mock());

        let (_window_id, view) =
            app.add_window(WindowStyle::NotStealFocus, DirectApiSettingsPageView::new);

        view.update(&mut app, |view, ctx| {
            view.handle_action(&DirectApiPageAction::ToggleRigBackendEnabled, ctx);
            assert!(view.rig_backend_enabled(ctx));
        });

        app.read(|ctx| {
            let settings = DirectAPISettings::as_ref(ctx);
            assert!(*settings.rig_backend_enabled);
        });
    });
}

#[cfg(not(feature = "direct_api_rig_backend"))]
#[test]
fn rig_backend_toggle_is_noop_without_feature() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        DirectAPISettings::register(&mut app);
        app.add_singleton_model(|_| AuthStateProvider::new_logged_out_for_test());
        app.add_singleton_model(|_| Appearance::mock());
        app.add_singleton_model(|_| KeybindingChangedNotifier::mock());

        let (_window_id, view) =
            app.add_window(WindowStyle::NotStealFocus, DirectApiSettingsPageView::new);

        view.update(&mut app, |view, ctx| {
            view.handle_action(&DirectApiPageAction::ToggleRigBackendEnabled, ctx);
            assert!(!view.rig_backend_enabled(ctx));
        });

        app.read(|ctx| {
            let settings = DirectAPISettings::as_ref(ctx);
            assert!(!*settings.rig_backend_enabled);
        });
    });
}

#[cfg(not(feature = "direct_api_rig_backend"))]
#[test]
fn rig_backend_effective_value_is_off_without_feature_even_when_persisted() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        DirectAPISettings::register(&mut app);
        app.add_singleton_model(|_| AuthStateProvider::new_logged_out_for_test());
        app.add_singleton_model(|_| Appearance::mock());
        app.add_singleton_model(|_| KeybindingChangedNotifier::mock());

        DirectAPISettings::handle(&app).update(&mut app, |settings, ctx| {
            settings.rig_backend_enabled.set_value(true, ctx).unwrap();
        });

        let (_window_id, view) =
            app.add_window(WindowStyle::NotStealFocus, DirectApiSettingsPageView::new);

        view.read(&app, |view, ctx| {
            assert!(!view.rig_backend_enabled(ctx));
        });

        app.read(|ctx| {
            let settings = DirectAPISettings::as_ref(ctx);
            assert!(*settings.rig_backend_enabled);
        });
    });
}

// ============================================================================
// US-001: Provider Matrix Test
// ============================================================================

#[test]
fn validates_all_provider_variants() {
    // Walk ALL 6 ProviderType variants using ProviderType::all() (exhaustive, no _ wildcard)
    let all_providers = ProviderType::all();
    assert_eq!(
        all_providers.len(),
        6,
        "Expected exactly 6 provider variants"
    );

    for provider in all_providers {
        match provider {
            ProviderType::OpenAI => {
                assert!(
                    !provider.needs_base_url(),
                    "OpenAI should not need base URL"
                );
                assert_eq!(provider.api_key_placeholder(), "sk-...");
                assert_eq!(provider.base_url_placeholder(), "");
                assert_eq!(provider.default_base_url(), "");
                assert!(
                    provider.validate_api_key("sk-abc123").is_ok(),
                    "OpenAI should accept valid key"
                );
                assert!(
                    provider.validate_api_key("").is_err(),
                    "OpenAI should reject empty key"
                );
            }
            ProviderType::Anthropic => {
                assert!(
                    !provider.needs_base_url(),
                    "Anthropic should not need base URL"
                );
                assert_eq!(provider.api_key_placeholder(), "sk-ant-...");
                assert_eq!(provider.base_url_placeholder(), "");
                assert_eq!(provider.default_base_url(), "");
                assert!(
                    provider.validate_api_key("sk-ant-abc123").is_ok(),
                    "Anthropic should accept valid key"
                );
                assert!(
                    provider.validate_api_key("sk-only").is_err(),
                    "Anthropic should reject key without -ant- infix"
                );
            }
            ProviderType::GoogleGemini => {
                assert!(
                    !provider.needs_base_url(),
                    "GoogleGemini should not need base URL"
                );
                assert_eq!(provider.api_key_placeholder(), "AIza...");
                assert_eq!(provider.base_url_placeholder(), "");
                assert_eq!(provider.default_base_url(), "");
                assert!(
                    provider.validate_api_key("AIzaSyAnything").is_ok(),
                    "GoogleGemini should accept valid key"
                );
                assert!(
                    provider.validate_api_key("").is_err(),
                    "GoogleGemini should reject empty key"
                );
            }
            ProviderType::Ollama => {
                assert!(provider.needs_base_url(), "Ollama should need base URL");
                assert_eq!(provider.api_key_placeholder(), "Optional");
                assert_eq!(provider.base_url_placeholder(), "http://localhost:11434");
                assert_eq!(provider.default_base_url(), "http://localhost:11434");
                assert!(
                    provider.validate_api_key("").is_ok(),
                    "Ollama should accept empty key"
                );
                assert!(
                    provider.validate_api_key("anything").is_ok(),
                    "Ollama should accept any key"
                );
            }
            ProviderType::OpenRouter => {
                assert!(provider.needs_base_url(), "OpenRouter should need base URL");
                assert_eq!(provider.api_key_placeholder(), "sk-or-v1-...");
                assert_eq!(
                    provider.base_url_placeholder(),
                    "https://openrouter.ai/api/v1"
                );
                assert_eq!(provider.default_base_url(), "https://openrouter.ai/api/v1");
                assert!(
                    provider.validate_api_key("sk-or-v1-anything").is_ok(),
                    "OpenRouter should accept valid key"
                );
                assert!(
                    provider.validate_api_key("").is_err(),
                    "OpenRouter should reject empty key"
                );
            }
            ProviderType::Custom => {
                assert!(provider.needs_base_url(), "Custom should need base URL");
                assert_eq!(provider.api_key_placeholder(), "Optional");
                assert_eq!(
                    provider.base_url_placeholder(),
                    "https://api.example.com/v1"
                );
                assert_eq!(provider.default_base_url(), "");
                assert!(
                    provider.validate_api_key("").is_ok(),
                    "Custom should accept empty key"
                );
                assert!(
                    provider.validate_api_key("anything").is_ok(),
                    "Custom should accept any key"
                );
            }
        }
    }
}

#[test]
fn save_path_keeps_key_buffer_masked_for_follow_up_actions() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        DirectAPISettings::register(&mut app);
        app.add_singleton_model(|_| AuthStateProvider::new_logged_out_for_test());
        app.add_singleton_model(|_| Appearance::mock());
        app.add_singleton_model(|_| KeybindingChangedNotifier::mock());

        let (_window_id, view) =
            app.add_window(WindowStyle::NotStealFocus, DirectApiSettingsPageView::new);
        view.update(&mut app, |view, ctx| {
            let row = view
                .provider_row(ProviderType::OpenAI)
                .expect("OpenAI row should exist");
            row.api_key_editor.update(ctx, |editor, ctx| {
                editor.set_buffer_text("sk-test-key", ctx);
            });
            view.apply_api_key_visibility(ProviderType::OpenAI, true, ctx);

            view.handle_save_api_key(ProviderType::OpenAI, ctx);

            let row = view
                .provider_row(ProviderType::OpenAI)
                .expect("OpenAI row should exist");
            assert_eq!(
                row.api_key_editor.as_ref(ctx).buffer_text(ctx),
                "sk-test-key"
            );
            assert!(!row.show_api_key.get());
        });

        app.read(|ctx| {
            let settings = DirectAPISettings::as_ref(ctx);
            assert_eq!(
                settings.api_key_openai.value().as_deref(),
                Some("sk-test-key")
            );
        });
    });
}

#[test]
fn custom_save_with_blank_key_preserves_existing_key() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        DirectAPISettings::register(&mut app);
        app.add_singleton_model(|_| AuthStateProvider::new_logged_out_for_test());
        app.add_singleton_model(|_| Appearance::mock());
        app.add_singleton_model(|_| KeybindingChangedNotifier::mock());

        DirectAPISettings::handle(&app).update(&mut app, |settings, ctx| {
            settings
                .api_key_custom
                .set_value(Some("existing-custom-key".to_string()), ctx)
                .expect("custom API key should save");
        });

        let (_window_id, view) =
            app.add_window(WindowStyle::NotStealFocus, DirectApiSettingsPageView::new);
        view.update(&mut app, |view, ctx| {
            let row = view
                .provider_row(ProviderType::Custom)
                .expect("Custom row should exist");
            row.api_key_editor.update(ctx, |editor, ctx| {
                editor.set_buffer_text("", ctx);
            });
            row.base_url_editor
                .as_ref()
                .expect("Custom should have a base URL editor")
                .update(ctx, |editor, ctx| {
                    editor.set_buffer_text("https://custom.example.com/v1", ctx);
                });

            view.handle_save_api_key(ProviderType::Custom, ctx);
        });

        app.read(|ctx| {
            let settings = DirectAPISettings::as_ref(ctx);
            assert_eq!(
                settings.api_key_custom.value().as_deref(),
                Some("existing-custom-key")
            );
            assert_eq!(
                settings.base_url_custom.value().as_deref(),
                Some("https://custom.example.com/v1")
            );
        });
    });
}

#[test]
fn openrouter_save_with_blank_key_preserves_existing_key() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        DirectAPISettings::register(&mut app);
        app.add_singleton_model(|_| AuthStateProvider::new_logged_out_for_test());
        app.add_singleton_model(|_| Appearance::mock());
        app.add_singleton_model(|_| KeybindingChangedNotifier::mock());

        DirectAPISettings::handle(&app).update(&mut app, |settings, ctx| {
            settings
                .api_key_openrouter
                .set_value(Some("sk-or-v1-existing".to_string()), ctx)
                .expect("OpenRouter API key should save");
        });

        let (_window_id, view) =
            app.add_window(WindowStyle::NotStealFocus, DirectApiSettingsPageView::new);
        view.update(&mut app, |view, ctx| {
            let row = view
                .provider_row(ProviderType::OpenRouter)
                .expect("OpenRouter row should exist");
            row.api_key_editor.update(ctx, |editor, ctx| {
                editor.set_buffer_text("", ctx);
            });
            row.base_url_editor
                .as_ref()
                .expect("OpenRouter should have a base URL editor")
                .update(ctx, |editor, ctx| {
                    editor.set_buffer_text("https://openrouter.example/v1", ctx);
                });

            view.handle_save_api_key(ProviderType::OpenRouter, ctx);
        });

        app.read(|ctx| {
            let settings = DirectAPISettings::as_ref(ctx);
            assert_eq!(
                settings.api_key_openrouter.value().as_deref(),
                Some("sk-or-v1-existing")
            );
            assert_eq!(
                settings.base_url_openrouter.value().as_deref(),
                Some("https://openrouter.example/v1")
            );
        });
    });
}

#[test]
fn openrouter_test_with_blank_key_uses_existing_key() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        DirectAPISettings::register(&mut app);
        app.add_singleton_model(|_| AuthStateProvider::new_logged_out_for_test());
        app.add_singleton_model(|_| Appearance::mock());
        app.add_singleton_model(|_| KeybindingChangedNotifier::mock());

        DirectAPISettings::handle(&app).update(&mut app, |settings, ctx| {
            settings
                .api_key_openrouter
                .set_value(Some("sk-or-v1-existing".to_string()), ctx)
                .expect("OpenRouter API key should save");
        });

        let (_window_id, view) =
            app.add_window(WindowStyle::NotStealFocus, DirectApiSettingsPageView::new);
        view.update(&mut app, |view, ctx| {
            let row = view
                .provider_row(ProviderType::OpenRouter)
                .expect("OpenRouter row should exist");
            row.api_key_editor.update(ctx, |editor, ctx| {
                editor.set_buffer_text("", ctx);
            });

            view.handle_test_connection(ProviderType::OpenRouter, ctx);

            let row = view
                .provider_row(ProviderType::OpenRouter)
                .expect("OpenRouter row should exist");
            assert_eq!(
                row.test_result.borrow().as_ref(),
                Some(&Ok(
                    "API key format valid. Run Refresh models to validate provider access."
                        .to_string()
                ))
            );
        });
    });
}

#[test]
fn openrouter_test_rejects_invalid_base_url_before_preflight_success() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        DirectAPISettings::register(&mut app);
        app.add_singleton_model(|_| AuthStateProvider::new_logged_out_for_test());
        app.add_singleton_model(|_| Appearance::mock());
        app.add_singleton_model(|_| KeybindingChangedNotifier::mock());

        DirectAPISettings::handle(&app).update(&mut app, |settings, ctx| {
            settings
                .api_key_openrouter
                .set_value(Some("sk-or-v1-existing".to_string()), ctx)
                .expect("OpenRouter API key should save");
        });

        let (_window_id, view) =
            app.add_window(WindowStyle::NotStealFocus, DirectApiSettingsPageView::new);
        view.update(&mut app, |view, ctx| {
            let row = view
                .provider_row(ProviderType::OpenRouter)
                .expect("OpenRouter row should exist");
            row.base_url_editor
                .as_ref()
                .expect("OpenRouter should have a base URL editor")
                .update(ctx, |editor, ctx| {
                    editor.set_buffer_text("http://openrouter.ai/api/v1", ctx);
                });

            view.handle_test_connection(ProviderType::OpenRouter, ctx);

            let row = view
                .provider_row(ProviderType::OpenRouter)
                .expect("OpenRouter row should exist");
            assert_eq!(
                row.test_result.borrow().as_ref(),
                Some(&Err(
                    "Base URL must use https://, except http:// localhost or private LAN addresses"
                        .to_string()
                ))
            );
        });
    });
}

#[test]
fn openrouter_refresh_rejects_saved_key_with_invalid_prefix_before_fetch() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        DirectAPISettings::register(&mut app);
        app.add_singleton_model(|_| AuthStateProvider::new_logged_out_for_test());
        app.add_singleton_model(|_| Appearance::mock());
        app.add_singleton_model(|_| KeybindingChangedNotifier::mock());
        app.add_singleton_model(AppTelemetryContextProvider::new_context_provider);

        DirectAPISettings::handle(&app).update(&mut app, |settings, ctx| {
            settings
                .api_key_openrouter
                .set_value(Some("sk-or-invalid".to_string()), ctx)
                .expect("OpenRouter API key should save");
        });

        let (_window_id, view) =
            app.add_window(WindowStyle::NotStealFocus, DirectApiSettingsPageView::new);
        view.update(&mut app, |view, ctx| {
            view.handle_update_model_list(ProviderType::OpenRouter, ctx);

            let row = view
                .provider_row(ProviderType::OpenRouter)
                .expect("OpenRouter row should exist");
            assert_eq!(
                row.test_result.borrow().as_ref(),
                Some(&Err(
                    "OpenRouter API keys should start with 'sk-or-v1-'".to_string()
                ))
            );
            assert!(!row.fetch_in_flight.get());
        });
    });
}

#[test]
fn openrouter_select_model_rejects_saved_key_with_invalid_prefix() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        DirectAPISettings::register(&mut app);
        app.add_singleton_model(|_| AuthStateProvider::new_logged_out_for_test());
        app.add_singleton_model(|_| Appearance::mock());
        app.add_singleton_model(|_| KeybindingChangedNotifier::mock());
        app.add_singleton_model(AppTelemetryContextProvider::new_context_provider);

        DirectAPISettings::handle(&app).update(&mut app, |settings, ctx| {
            settings
                .api_key_openrouter
                .set_value(Some("sk-or-invalid".to_string()), ctx)
                .expect("OpenRouter API key should save");
        });

        let (_window_id, view) =
            app.add_window(WindowStyle::NotStealFocus, DirectApiSettingsPageView::new);
        view.update(&mut app, |view, ctx| {
            view.handle_select_model(
                ProviderType::OpenRouter,
                "openrouter/model".to_string(),
                ctx,
            );

            let row = view
                .provider_row(ProviderType::OpenRouter)
                .expect("OpenRouter row should exist");
            assert_eq!(
                row.test_result.borrow().as_ref(),
                Some(&Err(
                    "OpenRouter API keys should start with 'sk-or-v1-'".to_string()
                ))
            );
        });

        app.read(|ctx| {
            let settings = DirectAPISettings::as_ref(ctx);
            assert!(!settings.selected_models.value().contains_key("OpenRouter"));
        });
    });
}

#[test]
fn openrouter_dropdown_hides_cached_models_when_saved_key_has_invalid_prefix() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        DirectAPISettings::register(&mut app);
        app.add_singleton_model(|_| AuthStateProvider::new_logged_out_for_test());
        app.add_singleton_model(|_| Appearance::mock());
        app.add_singleton_model(|_| KeybindingChangedNotifier::mock());

        DirectAPISettings::handle(&app).update(&mut app, |settings, ctx| {
            settings
                .api_key_openrouter
                .set_value(Some("sk-or-invalid".to_string()), ctx)
                .expect("OpenRouter API key should save");
            let mut selected_models = settings.selected_models.value().clone();
            selected_models.insert("OpenRouter".to_string(), "openrouter/stale".to_string());
            settings
                .selected_models
                .set_value(selected_models, ctx)
                .expect("selected model should save");
        });

        let (_window_id, view) =
            app.add_window(WindowStyle::NotStealFocus, DirectApiSettingsPageView::new);
        view.update(&mut app, |view, ctx| {
            view.refresh_model_dropdown(ProviderType::OpenRouter, ctx);

            let row = view
                .provider_row(ProviderType::OpenRouter)
                .expect("OpenRouter row should exist");
            assert!(!row.model_dropdown_has_items.get());
            assert!(row.cached_models.borrow().is_empty());
        });
    });
}

#[test]
fn ollama_test_rejects_invalid_base_url_before_preflight_success() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        DirectAPISettings::register(&mut app);
        app.add_singleton_model(|_| AuthStateProvider::new_logged_out_for_test());
        app.add_singleton_model(|_| Appearance::mock());
        app.add_singleton_model(|_| KeybindingChangedNotifier::mock());

        let (_window_id, view) =
            app.add_window(WindowStyle::NotStealFocus, DirectApiSettingsPageView::new);
        view.update(&mut app, |view, ctx| {
            let row = view
                .provider_row(ProviderType::Ollama)
                .expect("Ollama row should exist");
            row.base_url_editor
                .as_ref()
                .expect("Ollama should have a base URL editor")
                .update(ctx, |editor, ctx| {
                    editor.set_buffer_text("http://8.8.8.8:11434", ctx);
                });

            view.handle_test_connection(ProviderType::Ollama, ctx);

            let row = view
                .provider_row(ProviderType::Ollama)
                .expect("Ollama row should exist");
            assert_eq!(
                row.test_result.borrow().as_ref(),
                Some(&Err(
                    "Base URL must use https://, except http:// localhost or private LAN addresses"
                        .to_string()
                ))
            );
        });
    });
}

#[test]
fn custom_test_rejects_missing_base_url_before_preflight_success() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        DirectAPISettings::register(&mut app);
        app.add_singleton_model(|_| AuthStateProvider::new_logged_out_for_test());
        app.add_singleton_model(|_| Appearance::mock());
        app.add_singleton_model(|_| KeybindingChangedNotifier::mock());

        let (_window_id, view) =
            app.add_window(WindowStyle::NotStealFocus, DirectApiSettingsPageView::new);
        view.update(&mut app, |view, ctx| {
            let row = view
                .provider_row(ProviderType::Custom)
                .expect("Custom row should exist");
            row.base_url_editor
                .as_ref()
                .expect("Custom should have a base URL editor")
                .update(ctx, |editor, ctx| {
                    editor.set_buffer_text("", ctx);
                });

            view.handle_test_connection(ProviderType::Custom, ctx);

            let row = view
                .provider_row(ProviderType::Custom)
                .expect("Custom row should exist");
            assert_eq!(
                row.test_result.borrow().as_ref(),
                Some(&Err("Base URL is required for custom providers".to_string()))
            );
        });
    });
}

#[test]
fn model_fetch_callback_reports_openrouter_auth_failure_as_saved_key_rejection() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        DirectAPISettings::register(&mut app);
        app.add_singleton_model(|_| AuthStateProvider::new_logged_out_for_test());
        app.add_singleton_model(|_| Appearance::mock());
        app.add_singleton_model(|_| KeybindingChangedNotifier::mock());
        app.add_singleton_model(AppTelemetryContextProvider::new_context_provider);

        let (_window_id, view) =
            app.add_window(WindowStyle::NotStealFocus, DirectApiSettingsPageView::new);
        view.update(&mut app, |view, ctx| {
            let cache_path = tempfile::tempdir()
                .expect("cache temp dir should be created")
                .path()
                .join("models.json");
            let cache = Arc::new(
                ModelListCache::new_with_path(cache_path).expect("test cache should be created"),
            );

            view.on_models_fetched(
                (
                    ProviderId::OpenRouter,
                    cache,
                    Err(ModelListError::AuthFailed),
                    42,
                ),
                ctx,
            );

            let row = view
                .provider_row(ProviderType::OpenRouter)
                .expect("OpenRouter row should exist");
            assert_eq!(
                row.test_result.borrow().as_ref(),
                Some(&Err("Provider rejected the saved API key.".to_string()))
            );
        });
    });
}

#[test]
fn model_fetch_callback_reports_openrouter_success_as_access_validated() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        DirectAPISettings::register(&mut app);
        app.add_singleton_model(|_| AuthStateProvider::new_logged_out_for_test());
        app.add_singleton_model(|_| Appearance::mock());
        app.add_singleton_model(|_| KeybindingChangedNotifier::mock());
        app.add_singleton_model(AppTelemetryContextProvider::new_context_provider);

        let (_window_id, view) =
            app.add_window(WindowStyle::NotStealFocus, DirectApiSettingsPageView::new);
        view.update(&mut app, |view, ctx| {
            let cache_path = tempfile::tempdir()
                .expect("cache temp dir should be created")
                .path()
                .join("models.json");
            let cache = Arc::new(
                ModelListCache::new_with_path(cache_path).expect("test cache should be created"),
            );

            view.on_models_fetched(
                (
                    ProviderId::OpenRouter,
                    cache,
                    Ok(vec![
                        ModelDescriptor {
                            id: "openrouter/model-a".to_string(),
                            display_name: None,
                            context_window: None,
                            supports_tools: true,
                        },
                        ModelDescriptor {
                            id: "openrouter/model-b".to_string(),
                            display_name: None,
                            context_window: None,
                            supports_tools: true,
                        },
                    ]),
                    42,
                ),
                ctx,
            );

            let row = view
                .provider_row(ProviderType::OpenRouter)
                .expect("OpenRouter row should exist");
            assert_eq!(
                row.test_result.borrow().as_ref(),
                Some(&Ok(
                    "OpenRouter access validated. Fetched 2 models.".to_string()
                ))
            );
        });
    });
}

#[test]
fn provider_rows_load_persisted_base_urls_on_startup() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        DirectAPISettings::register(&mut app);
        app.add_singleton_model(|_| AuthStateProvider::new_logged_out_for_test());
        app.add_singleton_model(|_| Appearance::mock());
        app.add_singleton_model(|_| KeybindingChangedNotifier::mock());

        DirectAPISettings::handle(&app).update(&mut app, |settings, ctx| {
            settings
                .base_url_custom
                .set_value(Some("https://custom.example.com/v1".to_string()), ctx)
                .expect("custom base URL should save");
            settings
                .base_url_openrouter
                .set_value(Some("https://openrouter.example/v1".to_string()), ctx)
                .expect("OpenRouter base URL should save");
            settings
                .base_url_ollama
                .set_value(Some("http://localhost:11434/v1".to_string()), ctx)
                .expect("Ollama base URL should save");
        });

        let (_window_id, view) =
            app.add_window(WindowStyle::NotStealFocus, DirectApiSettingsPageView::new);
        view.read(&app, |view, ctx| {
            let custom_row = view
                .provider_row(ProviderType::Custom)
                .expect("Custom row should exist");
            assert_eq!(
                custom_row
                    .base_url_editor
                    .as_ref()
                    .expect("Custom should have a base URL editor")
                    .as_ref(ctx)
                    .buffer_text(ctx),
                "https://custom.example.com/v1"
            );

            let openrouter_row = view
                .provider_row(ProviderType::OpenRouter)
                .expect("OpenRouter row should exist");
            assert_eq!(
                openrouter_row
                    .base_url_editor
                    .as_ref()
                    .expect("OpenRouter should have a base URL editor")
                    .as_ref(ctx)
                    .buffer_text(ctx),
                "https://openrouter.example/v1"
            );

            let ollama_row = view
                .provider_row(ProviderType::Ollama)
                .expect("Ollama row should exist");
            assert_eq!(
                ollama_row
                    .base_url_editor
                    .as_ref()
                    .expect("Ollama should have a base URL editor")
                    .as_ref(ctx)
                    .buffer_text(ctx),
                "http://localhost:11434/v1"
            );
        });
    });
}

// ============================================================================
// LAN Address HTTP Validation Tests (RFC 1918 Private IP Ranges)
// ============================================================================

#[test]
fn is_safe_for_http_allows_https_always() {
    assert!(is_safe_for_http("https://api.example.com"));
    assert!(is_safe_for_http("https://1.2.3.4:8080"));
    assert!(is_safe_for_http("https://192.0.2.1/path"));
}

#[test]
fn is_safe_for_http_allows_localhost() {
    assert!(is_safe_for_http("http://localhost"));
    assert!(is_safe_for_http("http://localhost:11434"));
    assert!(is_safe_for_http("http://localhost:11434/v1/chat"));
}

#[test]
fn is_safe_for_http_allows_127_loopback() {
    assert!(is_safe_for_http("http://127.0.0.1"));
    assert!(is_safe_for_http("http://127.0.0.1:8080"));
    assert!(is_safe_for_http("http://127.1.2.3"));
    assert!(is_safe_for_http("http://127.255.255.255/api"));
}

#[test]
fn is_safe_for_http_allows_rfc1918_10_dot() {
    // 10.0.0.0/8 - entire 10.x.x.x range
    assert!(is_safe_for_http("http://10.0.0.1"));
    assert!(is_safe_for_http("http://10.42.18.156:12345"));
    assert!(is_safe_for_http("http://10.255.255.254"));
    assert!(is_safe_for_http("http://10.1.1.1/v1/chat"));
}

#[test]
fn is_safe_for_http_allows_rfc1918_192_168() {
    // 192.168.0.0/16 - entire 192.168.x.x range
    assert!(is_safe_for_http("http://192.168.0.1"));
    assert!(is_safe_for_http("http://192.168.1.1:8080"));
    assert!(is_safe_for_http("http://192.168.255.254"));
    assert!(is_safe_for_http("http://192.168.100.50/api"));
}

#[test]
fn is_safe_for_http_allows_rfc1918_172_16_through_31() {
    // 172.16.0.0/12 - 172.16.0.0 through 172.31.255.255
    assert!(is_safe_for_http("http://172.16.0.1"));
    assert!(is_safe_for_http("http://172.16.255.254"));
    assert!(is_safe_for_http("http://172.20.1.1:8080"));
    assert!(is_safe_for_http("http://172.31.255.255"));
    assert!(is_safe_for_http("http://172.24.100.50/v1/chat"));
}

#[test]
fn is_safe_for_http_rejects_172_outside_16_through_31() {
    // 172.15.x.x and 172.32.x.x are NOT in RFC 1918 range
    assert!(!is_safe_for_http("http://172.15.0.1"));
    assert!(!is_safe_for_http("http://172.15.255.254"));
    assert!(!is_safe_for_http("http://172.32.0.1"));
    assert!(!is_safe_for_http("http://172.32.1.1"));
    assert!(!is_safe_for_http("http://172.0.0.1"));
    assert!(!is_safe_for_http("http://172.255.255.255"));
}

#[test]
fn is_safe_for_http_rejects_public_ips() {
    // Public IPv4 addresses require HTTPS
    assert!(!is_safe_for_http("http://1.2.3.4"));
    assert!(!is_safe_for_http("http://8.8.8.8:8080"));
    assert!(!is_safe_for_http("http://192.0.2.1")); // TEST-NET-1
    assert!(!is_safe_for_http("http://198.51.100.1")); // TEST-NET-2
    assert!(!is_safe_for_http("http://203.0.113.1")); // TEST-NET-3
}

#[test]
fn is_safe_for_http_rejects_non_http_schemes() {
    assert!(!is_safe_for_http("ftp://localhost"));
    assert!(!is_safe_for_http("ws://10.0.0.1"));
    assert!(!is_safe_for_http("file:///path/to/file"));
    assert!(!is_safe_for_http(""));
    assert!(!is_safe_for_http("not-a-url"));
}

#[test]
fn is_safe_for_http_rejects_query_fragment_and_userinfo() {
    assert!(!is_safe_for_http("https://api.example.com/v1?tenant=x"));
    assert!(!is_safe_for_http("https://api.example.com/v1#models"));
    assert!(!is_safe_for_http("https://user:pass@api.example.com/v1"));
}

#[test]
fn is_safe_for_http_handles_ports_and_paths() {
    // Verify that ports and paths don't interfere with IP detection
    assert!(is_safe_for_http("http://10.42.18.156:12345/api/v1"));
    assert!(is_safe_for_http("http://192.168.1.1:8080/chat"));
    assert!(is_safe_for_http("http://172.20.0.1:9000/completions"));
    assert!(!is_safe_for_http("http://8.8.8.8:53/dns"));
}
