// Direct API Settings (OSS fork - local-only storage in settings.toml)
//
// This settings group stores Direct API configuration in ~/.warp/settings.toml
// instead of macOS Keychain, eliminating password prompts and rebuild reset issues
// in development builds.

use crate::define_settings_group;
use crate::{SupportedPlatforms, SyncToCloud};
use std::collections::HashMap;

define_settings_group!(DirectAPISettings, settings: [
    selected_provider: DirectAPISelectedProvider {
        type: Option<String>,
        default: None,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Never,
        private: false,
        toml_path: "agents.direct_api.selected_provider",
        description: "Selected Direct API provider (OpenAI, Anthropic, GoogleGemini, Ollama, OpenRouter, Custom)",
    },

    api_key_openai: DirectAPIKeyOpenAI {
        type: Option<String>,
        default: None,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Never,
        private: false,
        toml_path: "agents.direct_api.api_keys.openai",
        description: "OpenAI API key for direct API access (stored in plaintext)",
    },

    api_key_anthropic: DirectAPIKeyAnthropic {
        type: Option<String>,
        default: None,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Never,
        private: false,
        toml_path: "agents.direct_api.api_keys.anthropic",
        description: "Anthropic API key for direct API access (stored in plaintext)",
    },

    api_key_google: DirectAPIKeyGoogle {
        type: Option<String>,
        default: None,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Never,
        private: false,
        toml_path: "agents.direct_api.api_keys.google",
        description: "Google Gemini API key for direct API access (stored in plaintext)",
    },

    api_key_openrouter: DirectAPIKeyOpenRouter {
        type: Option<String>,
        default: None,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Never,
        private: false,
        toml_path: "agents.direct_api.api_keys.open_router",
        description: "OpenRouter API key for direct API access (stored in plaintext)",
    },

    api_key_custom: DirectAPIKeyCustom {
        type: Option<String>,
        default: None,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Never,
        private: false,
        toml_path: "agents.direct_api.api_keys.custom",
        description: "Custom provider API key (optional, stored in plaintext)",
    },

    base_url_custom: DirectAPIBaseURLCustom {
        type: Option<String>,
        default: None,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Never,
        private: false,
        toml_path: "agents.direct_api.base_urls.custom",
        description: "Custom provider base URL",
    },

    base_url_openrouter: DirectAPIBaseURLOpenRouter {
        type: Option<String>,
        default: Some("https://openrouter.ai/api/v1".to_string()),
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Never,
        private: false,
        toml_path: "agents.direct_api.base_urls.openrouter",
        description: "OpenRouter base URL",
    },

    base_url_ollama: DirectAPIBaseURLOllama {
        type: Option<String>,
        default: Some("http://localhost:11434".to_string()),
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Never,
        private: false,
        toml_path: "agents.direct_api.base_urls.ollama",
        description: "Ollama base URL",
    },

    selected_models: DirectAPISelectedModels {
        type: HashMap<String, String>,
        default: HashMap::new(),
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Never,
        private: false,
        toml_path: "agents.direct_api.selected_models",
        max_table_depth: 1,
        description: "Selected model for each Direct API provider",
    },
]);
