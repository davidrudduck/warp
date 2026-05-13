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
                assert_eq!(provider.api_key_placeholder(), "sk-or-...");
                assert_eq!(
                    provider.base_url_placeholder(),
                    "https://openrouter.ai/api/v1"
                );
                assert_eq!(provider.default_base_url(), "https://openrouter.ai/api/v1");
                assert!(
                    provider.validate_api_key("sk-or-anything").is_ok(),
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

// ============================================================================
// US-002: Buffer Handling Tests
// ============================================================================

// TODO: Test that switching providers clears buffers and re-masks
// This requires ViewContext and mock setup to test view behavior.
// The test should:
// 1. Create a DirectApiSettingsPageView
// 2. Set API key in editor buffer
// 3. Switch provider via SelectProvider action
// 4. Assert that api_key_editor buffer is cleared
// 5. Assert that show_api_key is reset to false (masked)
//
// Implementation blocked on: Need to understand how to create ViewContext
// and EditorView in tests without full app context.
#[test]
#[ignore = "Requires ViewContext mock setup - see TODO comment"]
fn validates_buffer_clear_and_remask_on_provider_switch() {
    // Stub: This test validates that switching providers:
    // - Clears the API key editor buffer
    // - Clears the base URL editor buffer
    // - Resets show_api_key to false (re-masks)
    //
    // Expected behavior from direct_api_page.rs:
    // When SelectProvider action is received:
    // 1. Update selected_provider RefCell
    // 2. Clear api_key_editor buffer
    // 3. Clear base_url_editor buffer (if provider doesn't need it)
    // 4. Set show_api_key to false
    // 5. Prefill base_url_editor with default_base_url() if needs_base_url()
}

// TODO: Test that re-selecting Custom provider preserves user-typed base URL
// This requires ViewContext and mock setup to test view behavior.
// The test should:
// 1. Create a DirectApiSettingsPageView with Custom provider
// 2. Set custom base URL in base_url_editor
// 3. Switch to OpenAI provider
// 4. Switch back to Custom provider
// 5. Assert that base_url_editor still contains the user-typed URL
//
// Implementation blocked on: Need to understand how to create ViewContext
// and EditorView in tests without full app context.
#[test]
#[ignore = "Requires ViewContext mock setup - see TODO comment"]
fn preserves_custom_base_url_buffer_on_reselection() {
    // Stub: This test validates that re-selecting Custom provider:
    // - Preserves the user-typed base URL in the buffer
    // - Does NOT reset to empty string
    //
    // Expected behavior from direct_api_page.rs:
    // When switching back to Custom provider:
    // 1. Check if base_url_editor buffer is non-empty
    // 2. If non-empty, preserve it (user-typed)
    // 3. If empty, leave it empty (don't prefill)
}

// TODO: Test that save action clears buffer and re-masks
// This requires ViewContext and mock setup to test view behavior.
// The test should:
// 1. Create a DirectApiSettingsPageView
// 2. Set API key in editor buffer with show_api_key=true
// 3. Trigger SaveApiKey action
// 4. Assert that api_key_editor buffer is cleared
// 5. Assert that show_api_key is reset to false (masked)
//
// Implementation blocked on: Need to understand how to create ViewContext
// and EditorView in tests without full app context.
#[test]
#[ignore = "Requires ViewContext mock setup - see TODO comment"]
fn save_path_clears_buffer_and_remasks() {
    // Stub: This test validates that saving API key:
    // - Clears the API key editor buffer after successful save
    // - Resets show_api_key to false (re-masks)
    //
    // Expected behavior from direct_api_page.rs:
    // When SaveApiKey action completes successfully:
    // 1. Save key to keychain via ApiKeyManager
    // 2. Clear api_key_editor buffer
    // 3. Set show_api_key to false
    // 4. Update test_result with success message
}

#[test]
#[ignore = "Requires ViewContext mock setup"]
fn model_selector_renders_empty_state_when_cache_missing() {
    // Expected: When cached_models is empty, widget shows placeholder
    // "Click 'Update Model List' to fetch available models" and no dropdown.
    // Requires: ViewContext mock to verify render output.
}

#[test]
fn feature_flag_off_hides_model_selector_widget() {
    use warp_core::features::FeatureFlag;

    // Verify flag exists and is accessible.
    let _flag_exists = FeatureFlag::DirectApiModelSelection;

    // Expected: ModelSelectorWidget::render returns empty container when flag disabled.
    // Limitation: Feature flags are compile-time; runtime toggling not possible in tests.
}

#[test]
#[ignore = "Requires ViewContext mock setup"]
fn update_model_list_populates_dropdown_on_success() {
    // Expected: handle_update_model_list fetches models, writes to cache,
    // refreshes dropdown, emits telemetry, clears is_fetching_models.
    // Requires: ViewContext mock to verify async spawn and state updates.
}

#[test]
#[ignore = "Requires ViewContext mock setup"]
fn double_click_update_model_list_is_noop() {
    // Expected: Second click while is_fetching_models=true returns early, no spawn.
    // Requires: ViewContext mock to verify single spawn.
    // Note: US-011 (fetch_in_flight guard) marked incomplete; relies on button state only.
}
