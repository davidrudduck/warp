use std::collections::BTreeSet;
use std::time::Duration;

use ai::api_keys::ApiKeys;
use ai::model_registry::{ModelListCache, ProviderId};

use super::DirectApiProfileModelSelection;

const MODEL_CACHE_TTL: Duration = Duration::from_secs(86_400);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectApiModelChoice {
    pub selection: DirectApiProfileModelSelection,
    pub label: String,
    pub is_stale_or_manual: bool,
}

pub fn direct_api_model_choices_from_parts(
    keys: &ApiKeys,
    cache: Option<&ModelListCache>,
) -> Vec<DirectApiModelChoice> {
    let mut choices = Vec::new();
    let mut seen = BTreeSet::new();

    for provider_id in direct_api_providers_with_possible_models(keys) {
        if let Some(cache) = cache {
            if let Some(entry) = cache.get(provider_id, MODEL_CACHE_TTL) {
                for model in entry.models {
                    push_choice(&mut choices, &mut seen, provider_id, model.id, false);
                }
            }
        }

        if let Some(model_id) = keys.selected_models.get(&provider_id) {
            push_choice(&mut choices, &mut seen, provider_id, model_id.clone(), true);
        }
        if let Some(model_id) = default_model_id(provider_id) {
            push_choice(
                &mut choices,
                &mut seen,
                provider_id,
                model_id.to_string(),
                false,
            );
        }
    }

    choices
}

fn direct_api_providers_with_possible_models(keys: &ApiKeys) -> Vec<ProviderId> {
    let mut providers = Vec::new();
    if has_non_empty_value(&keys.openai) {
        providers.push(ProviderId::OpenAI);
    }
    if has_non_empty_value(&keys.anthropic) {
        providers.push(ProviderId::Anthropic);
    }
    if has_non_empty_value(&keys.google) {
        providers.push(ProviderId::GoogleGemini);
    }
    if has_non_empty_value(&keys.ollama_base_url) {
        providers.push(ProviderId::Ollama);
    }
    if has_non_empty_value(&keys.open_router) {
        providers.push(ProviderId::OpenRouter);
    }
    if has_non_empty_value(&keys.custom_base_url) {
        providers.push(ProviderId::Custom);
    }
    providers
}

fn has_non_empty_value(value: &Option<String>) -> bool {
    value.as_ref().is_some_and(|value| !value.trim().is_empty())
}

fn default_model_id(provider_id: ProviderId) -> Option<&'static str> {
    match provider_id {
        ProviderId::OpenAI => Some("gpt-4o-mini"),
        ProviderId::Anthropic => Some("claude-3-5-sonnet-20241022"),
        ProviderId::GoogleGemini => Some("gemini-2.0-flash"),
        ProviderId::Ollama | ProviderId::OpenRouter | ProviderId::Custom => None,
    }
}

fn push_choice(
    choices: &mut Vec<DirectApiModelChoice>,
    seen: &mut BTreeSet<(ProviderId, String)>,
    provider_id: ProviderId,
    model_id: String,
    is_stale_or_manual: bool,
) {
    let model_id = model_id.trim().to_string();
    if model_id.is_empty() || !seen.insert((provider_id, model_id.clone())) {
        return;
    }

    let selection = DirectApiProfileModelSelection {
        provider_id,
        model_id,
    };
    choices.push(DirectApiModelChoice {
        label: selection.label(),
        selection,
        is_stale_or_manual,
    });
}

#[cfg(test)]
#[path = "direct_api_model_choices_tests.rs"]
mod tests;
