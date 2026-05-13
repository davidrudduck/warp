/// Canonical 6-variant provider identifier bridging UI (ProviderType) and core (ProviderKind).
///
/// This enum exists because:
/// - UI layer has 6 providers: OpenAI, Anthropic, GoogleGemini, Ollama, OpenRouter, Custom
/// - Core layer (ProviderKind) has 5 variants: OpenAI, Anthropic, Google, Ollama, OpenAICompatible
/// - Persistence needs a stable identifier that doesn't conflate OpenRouter + Custom
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd, serde::Serialize, serde::Deserialize,
)]
pub enum ProviderId {
    OpenAI,
    Anthropic,
    GoogleGemini,
    Ollama,
    OpenRouter,
    Custom,
}

impl ProviderId {
    /// Map to genai::adapter::AdapterKind for providers supported by genai.
    /// Returns None for OpenRouter and Custom (not in genai's AdapterKind).
    pub fn as_genai_adapter_kind(&self) -> Option<genai::adapter::AdapterKind> {
        match self {
            ProviderId::OpenAI => Some(genai::adapter::AdapterKind::OpenAI),
            ProviderId::Anthropic => Some(genai::adapter::AdapterKind::Anthropic),
            ProviderId::GoogleGemini => Some(genai::adapter::AdapterKind::Gemini),
            ProviderId::Ollama => Some(genai::adapter::AdapterKind::Ollama),
            ProviderId::OpenRouter => None,
            ProviderId::Custom => None,
        }
    }

    /// Convert from UI ProviderType. This is a 1:1 mapping.
    /// Note: This will be wired in a future step when we integrate with direct_api_page.rs
    pub fn from_provider_type_str(provider_str: &str) -> Option<Self> {
        match provider_str {
            "OpenAI" => Some(ProviderId::OpenAI),
            "Anthropic" => Some(ProviderId::Anthropic),
            "Google Gemini" => Some(ProviderId::GoogleGemini),
            "Ollama" => Some(ProviderId::Ollama),
            "OpenRouter" => Some(ProviderId::OpenRouter),
            "Custom (OpenAI-compatible)" => Some(ProviderId::Custom),
            _ => None,
        }
    }

    /// Get display name for UI (matches ProviderType::as_str)
    pub fn display_name(&self) -> &'static str {
        match self {
            ProviderId::OpenAI => "OpenAI",
            ProviderId::Anthropic => "Anthropic",
            ProviderId::GoogleGemini => "Google Gemini",
            ProviderId::Ollama => "Ollama",
            ProviderId::OpenRouter => "OpenRouter",
            ProviderId::Custom => "Custom (OpenAI-compatible)",
        }
    }
}
