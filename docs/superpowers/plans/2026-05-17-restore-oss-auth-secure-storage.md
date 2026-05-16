# Restore OSS Auth Secure Storage Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restore Warp cloud auth persistence for `warp-oss` by removing the OSS-only no-op secure-storage override while keeping Direct API provider keys in `~/.warp-oss/settings.toml`.

**Architecture:** Match upstream `warpdotdev/warp` secure-storage registration in `app/src/lib.rs`: only integration tests use no-op secure storage, Linux/FreeBSD use secure storage with file fallback, Windows uses file-backed secure storage, and macOS uses Keychain through `warpui_extras::secure_storage::register`. Do not move Direct API keys back to secure storage; they remain managed by `ApiKeyManager` through `DirectAPISettings`.

**Tech Stack:** Rust 2018/2021 workspace, WarpUI singleton models, `warpui_extras::secure_storage`, macOS Keychain, `DirectAPISettings`, TOML-backed settings at `~/.warp-oss/settings.toml`.

---

## Root Cause

Current fork behavior:

- `app/src/lib.rs` routes `Channel::Oss` to `warpui_extras::secure_storage::register_noop`.
- `crates/warpui_extras/src/secure_storage/noop.rs` returns `Ok(())` from `write_value` but stores nothing.
- `app/src/auth/auth_state.rs` reads `PersistedUser` from secure storage on startup.
- `app/src/auth/auth_manager.rs` writes Firebase auth state to secure storage after login.

Result: login appears to persist but is discarded by the no-op implementation, so every restart requires Warp cloud login again.

Upstream reference:

- `warpdotdev/warp` `b29c42426433ed5c6477c3a66f5df0243a6ddddc`, `app/src/lib.rs:1098-1108`, registers normal platform secure storage for `Channel::Oss`.

## File Structure

- Modify `app/src/lib.rs`
  - Restore upstream secure-storage registration.
  - Remove `should_use_noop_secure_storage`.
  - Remove `oss_secure_storage_tests::oss_channel_uses_noop_secure_storage`.
- Do not modify `crates/ai/src/api_keys.rs`
  - Direct API provider keys must continue using `DirectAPISettings`.
- Do not modify `crates/ai/src/api_keys_tests.rs`
  - Existing test `direct_api_configuration_writes_to_settings_without_secure_storage` already proves Direct API config writes to TOML without secure storage.

## Task 1: Prove The Regression With A Temporary Red Test

**Files:**
- Modify: `app/src/lib.rs`

- [ ] **Step 1: Replace the obsolete OSS no-op test with a failing expectation**

In `app/src/lib.rs`, temporarily replace the current test module:

```rust
#[cfg(test)]
mod oss_secure_storage_tests {
    use super::should_use_noop_secure_storage;
    use crate::channel::Channel;

    #[test]
    fn oss_channel_uses_noop_secure_storage() {
        assert!(should_use_noop_secure_storage(Channel::Oss));
        assert!(!should_use_noop_secure_storage(Channel::Stable));
        assert!(!should_use_noop_secure_storage(Channel::Preview));
        assert!(!should_use_noop_secure_storage(Channel::Dev));
    }
}
```

with this temporary regression test:

```rust
#[cfg(test)]
mod oss_secure_storage_tests {
    use super::should_use_noop_secure_storage;
    use crate::channel::Channel;

    #[test]
    fn oss_channel_should_not_force_noop_secure_storage() {
        assert!(!should_use_noop_secure_storage(Channel::Oss));
    }
}
```

- [ ] **Step 2: Run the temporary test and confirm it fails**

Run:

```bash
cargo test -p warp oss_channel_should_not_force_noop_secure_storage -- --nocapture
```

Expected: fail with an assertion failure because `should_use_noop_secure_storage(Channel::Oss)` currently returns `true`.

Do not commit this temporary failing test.

## Task 2: Restore Upstream Secure-Storage Registration

**Files:**
- Modify: `app/src/lib.rs`

- [ ] **Step 1: Replace the OSS-specific secure-storage branch with upstream registration**

In `app/src/lib.rs`, replace:

```rust
// Register an implementation of the secure storage service.
if should_use_noop_secure_storage(ChannelState::channel()) {
    warpui_extras::secure_storage::register_noop(&data_domain, ctx);
} else {
    cfg_if::cfg_if! {
        if #[cfg(feature = "integration_tests")] {
            warpui_extras::secure_storage::register_noop(&data_domain, ctx);
        } else if #[cfg(any(target_os = "linux", target_os = "freebsd"))] {
            warpui_extras::secure_storage::register_with_fallback(&data_domain, warp_core::paths::state_dir(), ctx)
        } else if #[cfg(target_os = "windows")] {
            warpui_extras::secure_storage::register_with_dir(&data_domain, warp_core::paths::state_dir(), ctx)
        } else {
            warpui_extras::secure_storage::register(&data_domain, ctx);
        }
    }
}
```

with the upstream registration:

```rust
// Register an implementation of the secure storage service.
cfg_if::cfg_if! {
    if #[cfg(feature = "integration_tests")] {
        warpui_extras::secure_storage::register_noop(&data_domain, ctx);
    } else if #[cfg(any(target_os = "linux", target_os = "freebsd"))] {
        warpui_extras::secure_storage::register_with_fallback(&data_domain, warp_core::paths::state_dir(), ctx)
    } else if #[cfg(target_os = "windows")] {
        warpui_extras::secure_storage::register_with_dir(&data_domain, warp_core::paths::state_dir(), ctx)
    } else {
        warpui_extras::secure_storage::register(&data_domain, ctx);
    }
}
```

- [ ] **Step 2: Remove the obsolete helper and test module**

Delete this code from `app/src/lib.rs`:

```rust
fn should_use_noop_secure_storage(channel: Channel) -> bool {
    matches!(channel, Channel::Oss)
}

#[cfg(test)]
mod oss_secure_storage_tests {
    use super::should_use_noop_secure_storage;
    use crate::channel::Channel;

    #[test]
    fn oss_channel_uses_noop_secure_storage() {
        assert!(should_use_noop_secure_storage(Channel::Oss));
        assert!(!should_use_noop_secure_storage(Channel::Stable));
        assert!(!should_use_noop_secure_storage(Channel::Preview));
        assert!(!should_use_noop_secure_storage(Channel::Dev));
    }
}
```

If the temporary test from Task 1 is still present, delete that temporary test module too.

- [ ] **Step 3: Remove now-unused imports if the compiler reports them**

If `cargo check` reports `Channel` is unused in `app/src/lib.rs`, remove it from the relevant `use` list. Do not remove `ChannelState`.

## Task 3: Verify Direct API Keys Still Use Settings TOML

**Files:**
- No code changes expected.

- [ ] **Step 1: Run the existing Direct API TOML persistence test**

Run:

```bash
cargo test -p ai direct_api_configuration_writes_to_settings_without_secure_storage -- --nocapture
```

Expected: pass. This confirms Direct API provider key/config writes still go through TOML-backed `DirectAPISettings`, not secure storage.

- [ ] **Step 2: Inspect the test if it fails**

If the test fails, inspect `crates/ai/src/api_keys.rs` and `crates/ai/src/api_keys_tests.rs`. The expected behavior is:

```rust
settings.api_key_custom.value().as_deref() == Some("custom-key")
settings.selected_provider.value().as_deref() == Some("Custom")
settings_toml.contains("[agents.direct_api.api_keys]")
settings_toml.contains("custom = \"custom-key\"")
```

Do not fix this by reintroducing keychain storage for provider keys.

## Task 4: Verify Build And Auth Persistence Path

**Files:**
- No code changes expected after Task 2.

- [ ] **Step 1: Run the OSS build check**

Run:

```bash
cargo check -p warp --bin warp-oss
```

Expected: exit code 0. Existing warnings may remain; do not treat existing warnings as this task's failure unless new errors appear.

- [ ] **Step 2: Confirm the restored code matches upstream shape**

Run:

```bash
git diff -- app/src/lib.rs
```

Expected: the secure-storage setup should match upstream shape:

```rust
cfg_if::cfg_if! {
    if #[cfg(feature = "integration_tests")] {
        warpui_extras::secure_storage::register_noop(&data_domain, ctx);
    } else if #[cfg(any(target_os = "linux", target_os = "freebsd"))] {
        warpui_extras::secure_storage::register_with_fallback(&data_domain, warp_core::paths::state_dir(), ctx)
    } else if #[cfg(target_os = "windows")] {
        warpui_extras::secure_storage::register_with_dir(&data_domain, warp_core::paths::state_dir(), ctx)
    } else {
        warpui_extras::secure_storage::register(&data_domain, ctx);
    }
}
```

Expected: no `should_use_noop_secure_storage` symbol remains:

```bash
rg -n "should_use_noop_secure_storage|oss_channel_uses_noop_secure_storage" app/src/lib.rs
```

Expected: no matches.

- [ ] **Step 3: Optional manual macOS auth persistence check**

Only run this if you can interact with the launched UI:

```bash
cargo build -p warp --bin warp-oss
cp -p target/debug/warp-oss target/debug/bundle/osx/WarpOss.app/Contents/MacOS/warp-oss
open -n target/debug/bundle/osx/WarpOss.app
```

Manual expected behavior:

1. Log in to Warp cloud once.
2. Quit `WarpOss.app`.
3. Reopen `WarpOss.app`.
4. The app should restore the logged-in user instead of prompting for login again.

If macOS asks for Keychain access, allow access for `WarpOss.app`; this is the restored secure-storage path.

## Task 5: Commit

**Files:**
- Modify: `app/src/lib.rs`

- [ ] **Step 1: Review final diff**

Run:

```bash
git diff -- app/src/lib.rs
git status --short
```

Expected: only `app/src/lib.rs` changed for this auth fix.

- [ ] **Step 2: Commit**

Run:

```bash
git add app/src/lib.rs
git commit -m "Restore OSS auth secure storage"
```

Expected: commit succeeds.

## Self-Review Checklist

- [ ] `Channel::Oss` no longer forces no-op secure storage.
- [ ] Integration tests still use no-op secure storage.
- [ ] Linux/FreeBSD still use secure storage with fallback.
- [ ] Windows still uses directory-backed secure storage.
- [ ] macOS uses platform secure storage / Keychain.
- [ ] `should_use_noop_secure_storage` is gone.
- [ ] `oss_channel_uses_noop_secure_storage` is gone.
- [ ] Direct API provider keys still write through `DirectAPISettings` and TOML.
- [ ] No Direct API provider key code was moved back to Keychain.
