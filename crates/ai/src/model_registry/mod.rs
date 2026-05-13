pub mod cache;
pub mod known_capabilities;
pub mod list_provider;
pub mod provider_id;
pub mod providers;

use crate::provider::ModelCapabilities;
use known_capabilities::known_capabilities;

pub use cache::{CacheEntry, ModelListCache};
pub use list_provider::{ModelDescriptor, ModelListError, ModelListProvider};
pub use provider_id::ProviderId;

/// A registry that resolves model capabilities by model ID.
///
/// Resolution order:
/// 1. Known static table (`known_capabilities`).
/// 2. `ModelCapabilities::default()` as a last-resort fallback.
pub struct ModelRegistry;

impl ModelRegistry {
    /// Look up capabilities for the given model ID.
    ///
    /// Returns the static capabilities when the model ID is known, or
    /// `ModelCapabilities::default()` when it is not.
    pub fn capabilities_for(model_id: &str) -> ModelCapabilities {
        known_capabilities().remove(model_id).unwrap_or_default()
    }

    /// Returns `true` if capabilities for `model_id` are present in the
    /// static known-capabilities table.
    pub fn is_known(model_id: &str) -> bool {
        known_capabilities().contains_key(model_id)
    }
}

#[cfg(test)]
#[path = "cache_tests.rs"]
mod cache_tests;

#[cfg(test)]
#[path = "registry_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "provider_id_tests.rs"]
mod provider_id_tests;

#[cfg(test)]
#[path = "list_provider_tests.rs"]
mod list_provider_tests;
