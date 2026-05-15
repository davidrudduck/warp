---
name: warp-settings-toml
description: Guides Warp settings and settings.toml changes. Use when adding settings groups, changing define_settings_group! entries, toml_path values, PublicPreferences or PrivatePreferences behavior, settings UI persistence, or channel-specific config paths.
---

# Warp Settings TOML

Use this skill whenever user-visible configuration should persist through Warp's settings system.

## Core Files

- Settings macros and traits: `crates/settings/src/macros.rs`, `crates/settings/src/lib.rs`
- TOML backend: `crates/warpui_extras/src/user_preferences/toml_backed.rs`
- App settings init/path: `app/src/settings/init.rs`, `app/src/settings/mod.rs`
- Channel paths: `crates/warp_core/src/paths.rs`
- Existing Direct API example: `crates/settings/src/direct_api.rs`

## Rules

- Add settings with `define_settings_group!` unless an existing group is clearly the right owner.
- Non-private settings must have `toml_path`.
- Choose `sync_to_cloud: SyncToCloud::Never` for local-only or secret-like settings.
- For warp-oss Direct API, use local channel settings, not official `~/.warp`.
- Prefer typed settings APIs over ad hoc file parsing or string manipulation.
- Keep defaults explicit and backwards-compatible.
- Avoid compile-time cfg for behavior that can be represented as runtime settings or feature flags.

## TOML Path Guidance

- Use stable, user-readable paths.
- Group related values under one namespace, for example `agents.direct_api`.
- Avoid renaming existing paths unless you also handle migration or accept reset behavior.
- For maps, set `max_table_depth` when readable TOML structure matters.

## Testing

For schema or persistence changes, add file-backed tests. In tests, use:

- `TomlBackedUserPreferences` for actual TOML writes.
- `InMemoryPreferences` only when file shape is irrelevant.
- scoped `FeatureFlag::SettingsFile.override_enabled(true)` when needed.

Useful commands:

```bash
cargo test -p settings -- --nocapture
cargo test -p ai api_keys::tests -- --nocapture --test-threads=1
cargo check -p warp --bin warp-oss
```

## Review Checklist

- Does the setting have the right public/private storage?
- Does the TOML file contain the expected section and key names?
- Does the app recover from missing or invalid settings?
- Does changing the setting emit or subscribe to the right model events?
- Does documentation mention the correct channel path?
