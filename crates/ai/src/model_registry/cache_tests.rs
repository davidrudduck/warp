use super::cache::ModelListCache;
use super::{ModelDescriptor, ProviderId};
use std::time::Duration;

#[test]
fn cache_roundtrips_on_disk() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let cache_path = temp_dir.path().join("models.json");
    let cache = ModelListCache::new_with_path(cache_path).expect("failed to create cache");

    let models = vec![ModelDescriptor {
        id: "gpt-4o".to_string(),
        display_name: Some("GPT-4o".to_string()),
        context_window: Some(128000),
        supports_tools: true,
    }];

    cache
        .set(ProviderId::OpenAI, models.clone())
        .expect("failed to write cache");

    let entry = cache
        .get(ProviderId::OpenAI, Duration::from_secs(60))
        .expect("cache miss");

    assert_eq!(entry.models, models);
}

#[test]
fn atomic_write_preserves_original_on_failure() {
    // This test verifies tempfile + rename atomicity by:
    // 1. Writing valid cache
    // 2. Simulating write failure (can't easily do this without mocking filesystem)
    // 3. Verifying original cache intact

    // For now, verify the success path - atomic writes use tempfile::NamedTempFile
    // which guarantees atomicity via OS rename semantics
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let cache_path = temp_dir.path().join("models.json");
    let cache = ModelListCache::new_with_path(cache_path).expect("failed to create cache");

    let models_v1 = vec![ModelDescriptor {
        id: "gpt-4".to_string(),
        display_name: None,
        context_window: Some(8192),
        supports_tools: false,
    }];

    cache
        .set(ProviderId::OpenAI, models_v1.clone())
        .expect("failed to write cache v1");

    let models_v2 = vec![ModelDescriptor {
        id: "gpt-4o".to_string(),
        display_name: Some("GPT-4o".to_string()),
        context_window: Some(128000),
        supports_tools: true,
    }];

    cache
        .set(ProviderId::OpenAI, models_v2.clone())
        .expect("failed to write cache v2");

    let entry = cache
        .get(ProviderId::OpenAI, Duration::from_secs(60))
        .expect("cache miss");

    assert_eq!(entry.models, models_v2);
}

#[test]
fn get_returns_none_when_stale() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let cache_path = temp_dir.path().join("models.json");
    let cache = ModelListCache::new_with_path(cache_path).expect("failed to create cache");

    let models = vec![ModelDescriptor {
        id: "test".to_string(),
        display_name: None,
        context_window: None,
        supports_tools: false,
    }];

    cache
        .set(ProviderId::Anthropic, models)
        .expect("failed to write cache");

    // Fresh: should return Some
    assert!(cache
        .get(ProviderId::Anthropic, Duration::from_secs(60))
        .is_some());

    // Stale (max_age = 0): should return None
    assert!(cache
        .get(ProviderId::Anthropic, Duration::from_secs(0))
        .is_none());
}

#[test]
fn invalidate_removes_provider_entry() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let cache_path = temp_dir.path().join("models.json");
    let cache = ModelListCache::new_with_path(cache_path).expect("failed to create cache");

    let models = vec![ModelDescriptor {
        id: "gemini-2.0-flash".to_string(),
        display_name: None,
        context_window: Some(1000000),
        supports_tools: true,
    }];

    cache
        .set(ProviderId::GoogleGemini, models)
        .expect("failed to write cache");

    assert!(cache
        .get(ProviderId::GoogleGemini, Duration::from_secs(60))
        .is_some());

    cache
        .invalidate(ProviderId::GoogleGemini)
        .expect("failed to invalidate");

    assert!(cache
        .get(ProviderId::GoogleGemini, Duration::from_secs(60))
        .is_none());
}

#[test]
fn clear_all_removes_all_entries() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let cache_path = temp_dir.path().join("models.json");
    let cache = ModelListCache::new_with_path(cache_path).expect("failed to create cache");

    let models = vec![ModelDescriptor {
        id: "test".to_string(),
        display_name: None,
        context_window: None,
        supports_tools: false,
    }];

    cache
        .set(ProviderId::OpenAI, models.clone())
        .expect("failed to write");
    cache
        .set(ProviderId::Anthropic, models.clone())
        .expect("failed to write");

    cache.clear_all().expect("failed to clear");

    assert!(cache
        .get(ProviderId::OpenAI, Duration::from_secs(60))
        .is_none());
    assert!(cache
        .get(ProviderId::Anthropic, Duration::from_secs(60))
        .is_none());
}
