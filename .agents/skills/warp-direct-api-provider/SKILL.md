---
name: warp-direct-api-provider
description: Guides Direct API provider work in Warp OSS. Use when changing API keys, provider selection, model lists, base URLs, OpenRouter, Ollama, custom OpenAI-compatible endpoints, genai adapters, DirectAPISettings, or direct-provider tests/docs.
---

# Warp Direct API Provider

Use this skill for local provider configuration and request routing in the Warp OSS fork.

## Core Files

- Settings UI: `app/src/settings_view/direct_api_page.rs`
- API key/settings manager: `crates/ai/src/api_keys.rs`
- Settings schema: `crates/settings/src/direct_api.rs`
- Provider adapters and registries: `crates/ai/src/provider/`, `crates/ai/src/model_registry/`
- Direct provider tests: `crates/ai/src/api_keys_tests.rs`, `crates/ai/tests/e2e_direct_provider.rs`
- Docs: `docs/QUICK-START.md`, `docs/features/direct-api-*.md`

## Invariants

- Direct API config for warp-oss writes through `DirectAPISettings` and the channel-specific `settings.toml`.
- macOS warp-oss path is `~/.warp-oss/settings.toml`; do not use official `~/.warp`.
- Provider selection, API keys, base URLs, and selected models must survive restart.
- Custom provider API keys belong to the custom provider, not OpenAI.
- Ollama may have no API key.
- Remote HTTP base URLs should be rejected unless they are localhost or private LAN.
- Model-list cache must invalidate when keys or relevant base URLs change.
- Logs must redact keys and bearer/JWT-like values.

## Implementation Workflow

1. Identify provider surface:
   - UI form only
   - settings persistence
   - provider request conversion
   - model-list fetching
   - logging/redaction
   - docs
2. Read the matching core files before editing.
3. Keep provider IDs and display strings mapped exhaustively.
4. Keep validation shared between test/save/update flows when possible.
5. Update docs when user-visible setup or storage changes.

## Testing

Prefer targeted tests:

```bash
cargo test -p ai api_keys::tests -- --nocapture --test-threads=1
cargo test -p ai <provider_or_model_filter> -- --nocapture
cargo check -p warp --bin warp-oss
```

Add or update tests for:

- TOML-backed writes when persistence changes
- legacy payload parsing when `ApiKeys` changes
- cache invalidation when keys/base URLs change
- custom provider behavior
- redaction when logs change

Use `warp-validation-matrix` before finishing.
