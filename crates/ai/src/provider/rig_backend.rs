#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RigProviderKind {
    OpenAI,
    Anthropic,
    GoogleGemini,
    Ollama,
    OpenRouter,
    CustomOpenAICompatible,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RigBackendConfig {
    pub provider_kind: RigProviderKind,
    pub model_id: String,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
}

impl RigBackendConfig {
    pub fn new(
        provider_kind: RigProviderKind,
        model_id: impl Into<String>,
        api_key: Option<String>,
        base_url: Option<String>,
    ) -> Self {
        Self {
            provider_kind,
            model_id: model_id.into(),
            api_key,
            base_url,
        }
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        if self.model_id.trim().is_empty() {
            anyhow::bail!("Rig Direct API backend requires a model");
        }

        match self.provider_kind {
            RigProviderKind::Ollama => Ok(()),
            RigProviderKind::OpenAI
            | RigProviderKind::Anthropic
            | RigProviderKind::GoogleGemini
            | RigProviderKind::OpenRouter => {
                if self
                    .api_key
                    .as_deref()
                    .is_none_or(|key| key.trim().is_empty())
                {
                    anyhow::bail!("Rig Direct API backend requires an API key");
                }
                Ok(())
            }
            RigProviderKind::CustomOpenAICompatible => {
                if self
                    .base_url
                    .as_deref()
                    .is_none_or(|url| url.trim().is_empty())
                {
                    anyhow::bail!("Rig Direct API backend requires a base URL");
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
#[path = "rig_backend_tests.rs"]
mod tests;
