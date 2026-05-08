use crate::provider::ModelCapabilities;
use std::collections::HashMap;

/// Returns a map of well-known model IDs to their static capabilities.
///
/// This serves as a fallback when the provider cannot be reached and no
/// cached record exists in the database.
pub fn known_capabilities() -> HashMap<&'static str, ModelCapabilities> {
    let mut map = HashMap::new();

    // --- Anthropic Claude models ---
    map.insert(
        "claude-3-5-sonnet-20241022",
        ModelCapabilities {
            context_window: 200_000,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
        },
    );
    map.insert(
        "claude-3-5-haiku-20241022",
        ModelCapabilities {
            context_window: 200_000,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
        },
    );
    map.insert(
        "claude-3-opus-20240229",
        ModelCapabilities {
            context_window: 200_000,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
        },
    );
    map.insert(
        "claude-3-sonnet-20240229",
        ModelCapabilities {
            context_window: 200_000,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
        },
    );
    map.insert(
        "claude-3-haiku-20240307",
        ModelCapabilities {
            context_window: 200_000,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
        },
    );

    // --- OpenAI models ---
    map.insert(
        "gpt-4o",
        ModelCapabilities {
            context_window: 128_000,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
        },
    );
    map.insert(
        "gpt-4o-mini",
        ModelCapabilities {
            context_window: 128_000,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
        },
    );
    map.insert(
        "gpt-4-turbo",
        ModelCapabilities {
            context_window: 128_000,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
        },
    );
    map.insert(
        "gpt-3.5-turbo",
        ModelCapabilities {
            context_window: 16_385,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
        },
    );
    map.insert(
        "o1",
        ModelCapabilities {
            context_window: 200_000,
            supports_tools: false,
            supports_vision: true,
            supports_streaming: false,
        },
    );
    map.insert(
        "o1-mini",
        ModelCapabilities {
            context_window: 128_000,
            supports_tools: false,
            supports_vision: false,
            supports_streaming: false,
        },
    );

    // --- Google Gemini models ---
    map.insert(
        "gemini-1.5-pro",
        ModelCapabilities {
            context_window: 2_000_000,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
        },
    );
    map.insert(
        "gemini-1.5-flash",
        ModelCapabilities {
            context_window: 1_000_000,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
        },
    );
    map.insert(
        "gemini-2.0-flash",
        ModelCapabilities {
            context_window: 1_000_000,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
        },
    );

    map
}
