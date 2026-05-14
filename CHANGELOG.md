# Changelog

All notable changes to the Warp OSS Fork will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Direct API configuration now persists to `~/.warp/settings.toml` instead of macOS Keychain
- One-time automatic migration from keychain to settings.toml on first app startup
- Migration logged: "Migrated Direct API keys from keychain to settings.toml"

### Changed
- All Direct API key save operations now write to settings.toml (US-004)
- Button label updated from "Save to Keychain" to "Save Settings" (US-005)
- DirectAPISettings moved to shared `crates/settings` for cross-crate access

### Fixed
- Eliminated keychain password prompts for Direct API configuration
- Direct API settings now survive rebuilds (independent of binary)

### Removed
- Removed obsolete `write_keys_to_secure_storage()` function (US-007)

### Technical Details
**Migration Implementation (2026-05-14)**
- Commit: `702cbab`
- Files changed: 6 files, 293 insertions(+), 24 deletions(-)
- New file: `crates/settings/src/direct_api.rs` (DirectAPISettings definition)
- Modified: `crates/ai/src/api_keys.rs` (migration logic, settings I/O)
- Modified: `app/src/lib.rs` (migration call at startup)
- Modified: `app/src/settings_view/direct_api_page.rs` (button label)
- Build: ✅ Zero errors, zero warnings
- Tests: ✅ 271 passing

**Architecture**:
- `load_keys_from_settings()` - reads from DirectAPISettings on lazy load
- `write_keys_to_settings()` - writes to DirectAPISettings on save
- `migrate_from_keychain_if_needed()` - one-time migration at app startup
- Settings location: `~/.warp/settings.toml` under `[agents.direct_api]`

**Known Issues**:
- Settings UI buttons (Test Connection, Save Settings, Update Model List) may not respond due to WarpUI framework limitation
- Workaround: Manually edit `~/.warp/settings.toml` to configure Direct API

## [0.1.0] - 2026-05-11

### Added
- Initial Direct API implementation with 6 provider support (OpenAI, Anthropic, Google Gemini, Ollama, OpenRouter, Custom)
- Interactive Settings UI for Direct API configuration
- Multi-provider abstraction layer
- SQLite conversation persistence with WAL mode
- Agentic chat loop with tool dispatch
- Streaming responses with cancellation support
- File-based logging with secret redaction
- Session caching for keychain access
- Comprehensive documentation (1,721 lines)

### Technical Details
- Implementation commits: `2d176b5`, `fcc496b`, `5aed59e`, `a6ac264`
- Documentation commits: `4ffbc9f`, `58f9e1f`, `6b7e940`, `9378737`
- Test coverage: 271 tests passing (263 AI + 8 persistence)
- Performance optimizations: 200× faster secret redaction, 5× faster batch INSERT
