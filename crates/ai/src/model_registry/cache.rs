//! Model list cache with JSON persistence and atomic writes.

use crate::model_registry::{ModelDescriptor, ProviderId};
use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

/// Cache entry for a provider's model list.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CacheEntry {
    pub fetched_at: SystemTime,
    pub models: Vec<ModelDescriptor>,
}

/// JSON-backed cache for model lists at warp_core::paths::cache_dir()/direct_api/models.json
pub struct ModelListCache {
    cache_path: PathBuf,
}

impl ModelListCache {
    /// Create a new cache instance at the standard location.
    pub fn new() -> Result<Self> {
        let cache_dir = warp_core::paths::cache_dir().join("direct_api");
        std::fs::create_dir_all(&cache_dir).context("failed to create cache directory")?;

        Ok(Self {
            cache_path: cache_dir.join("models.json"),
        })
    }

    /// Create a cache instance with a custom path.
    pub fn new_with_path(cache_path: PathBuf) -> Result<Self> {
        if let Some(parent) = cache_path.parent() {
            std::fs::create_dir_all(parent).context("failed to create cache directory")?;
        }

        Ok(Self { cache_path })
    }

    /// Get cached models for a provider if fresh (within max_age).
    pub fn get(&self, provider: ProviderId, max_age: Duration) -> Option<CacheEntry> {
        let cache = self.read_cache().ok()?;
        let entry = cache.get(&provider)?;

        let age = SystemTime::now().duration_since(entry.fetched_at).ok()?;

        if age <= max_age {
            Some(entry.clone())
        } else {
            None
        }
    }

    /// Store models for a provider with atomic write (tempfile + rename).
    pub fn set(&self, provider: ProviderId, models: Vec<ModelDescriptor>) -> Result<()> {
        let mut cache = self.read_cache().unwrap_or_default();

        cache.insert(
            provider,
            CacheEntry {
                fetched_at: SystemTime::now(),
                models,
            },
        );

        self.write_cache_atomic(&cache)
    }

    /// Invalidate (remove) cached entry for a provider.
    pub fn invalidate(&self, provider: ProviderId) -> Result<()> {
        let mut cache = self.read_cache().unwrap_or_default();
        cache.remove(&provider);
        self.write_cache_atomic(&cache)
    }

    /// Clear all cached entries.
    pub fn clear_all(&self) -> Result<()> {
        self.write_cache_atomic(&BTreeMap::new())
    }

    fn read_cache(&self) -> Result<BTreeMap<ProviderId, CacheEntry>> {
        if !self.cache_path.exists() {
            return Ok(BTreeMap::new());
        }

        let contents =
            std::fs::read_to_string(&self.cache_path).context("failed to read cache file")?;

        serde_json::from_str(&contents).context("failed to parse cache JSON")
    }

    fn write_cache_atomic(&self, cache: &BTreeMap<ProviderId, CacheEntry>) -> Result<()> {
        let json = serde_json::to_string_pretty(cache).context("failed to serialize cache")?;

        // Atomic write: tempfile + rename
        let temp = tempfile::NamedTempFile::new_in(
            self.cache_path
                .parent()
                .context("cache path has no parent")?,
        )
        .context("failed to create temp file")?;

        std::fs::write(temp.path(), json).context("failed to write temp file")?;

        temp.persist(&self.cache_path)
            .context("failed to persist temp file")?;

        Ok(())
    }
}

impl Default for ModelListCache {
    fn default() -> Self {
        Self::new().expect("failed to create default ModelListCache")
    }
}
