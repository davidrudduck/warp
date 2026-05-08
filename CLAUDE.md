# CLAUDE.md — Warp Global Rules

## 1. Core Principles

- **Type safety first**: Prefer strong types; avoid `unwrap()` in library code — use `?` propagation
- **Runtime feature flags over cfg**: Use `FeatureFlag::YourFlag.is_enabled()` not `#[cfg(...)]` unless code genuinely cannot compile without it
- **Exhaustive matching**: Never use `_` wildcard in match arms — future-proof by handling all variants explicitly
- **No unused parameters**: Remove completely and update all call sites — never prefix with `_`
- **Inline format args**: `eprintln!("{message}")` not `eprintln!("{}", message)` — required by clippy
- **No `dbg!`**: Disallowed via `clippy.toml` — remove before committing
- **Presubmit before PR**: `cargo fmt` and `cargo clippy` must pass before opening or updating any PR

## 2. Tech Stack

| Component | Technology |
|---|---|
| Language | Rust (edition 2018) |
| Toolchain | 1.92.0 (pinned in `rust-toolchain.toml`) |
| Workspace | Cargo workspace, resolver v2, 60+ member crates |
| UI framework | WarpUI (custom, Entity-Component-Handle, Flutter-inspired) |
| Async runtime | Tokio 1.47.1 |
| Graphics | wgpu 29.0.1 (Metal / Vulkan / DX12 / GLES) |
| Database | Diesel 2.3.8 + SQLite |
| Serialization | serde 1.0 |
| GraphQL | cynic 3 (client-side code gen from schema) |
| HTTP client | reqwest 0.12.28 |
| Error handling | anyhow 1.0 + thiserror 2.0.17 + custom macros |
| Logging | `log` 0.4 + `warp_logging` wrapper |
| Testing | cargo-nextest + mockito |
| Error reporting | Sentry 0.41.0 |
| Platforms | macOS (primary), Windows, Linux, WASM |

## 3. Architecture

### Crate Layout

```text
app/                      # Main binary (terminal + AI + workspace + auth + settings + drive)
├── ai/                   # Agent Mode, MCP, skills, ambient agents
├── terminal/             # PTY, shell, blocks, history
├── workspace/            # Window/tab/pane management
├── auth/                 # Credentials, user management
└── ...

crates/
├── warpui/               # UI framework (re-exports warpui_core + platform/rendering)
├── warpui_core/          # Elements, actions, events, keymap, text, units
├── warp_core/            # Feature flags, errors, telemetry, platform, settings, paths
├── warp_logging/         # Logger init (native: file+stderr; WASM: console)
├── persistence/          # Diesel/SQLite ORM, migrations, schema
├── graphql/              # GraphQL client (cynic)
├── http_server/          # HTTP server (axum)
├── editor/               # Text editing
├── ipc/                  # Inter-process communication
├── lsp/                  # LSP client
├── integration/          # Integration test framework (excluded from default-members)
└── ...
```

### Key Patterns

**Entity-Handle System (WarpUI)**
- `App` owns all views/models as entities
- Views hold `ViewHandle<T>` — no direct ownership between views
- `AppContext` / `ViewContext` / `ModelContext` provide temporary access
- Context params always named `ctx`, always go last — except when a closure param exists, in which case the closure goes last

**Feature Flags**
- Single `FeatureFlag` enum in `warp_core/src/features.rs`
- Rollout tiers: `DOGFOOD_FLAGS` → `PREVIEW_FLAGS` → `RELEASE_FLAGS`
- Check with `FeatureFlag::YourFlag.is_enabled()`

**Cross-Platform Types**
- Use `command::blocking::Command` — NOT `std::process::Command` (Windows compat)
- Use `instant::Instant` — NOT `std::time::Instant` (WASM compat)

**Terminal Model Locking**
- `TerminalModel.lock()` is deadlock-sensitive — never acquire if a caller already holds it
- Pass locked refs down the call stack instead of re-locking; keep lock scope minimal

## 4. Code Style

### Naming

| Element | Convention | Example |
|---|---|---|
| Files, modules | `snake_case` | `terminal_model.rs` |
| Structs, enums, traits | `PascalCase` | `AppState`, `FeatureFlag` |
| Enum variants | `PascalCase` | `LogDestination::File` |
| Functions, methods | `snake_case` | `format_for_terminal_output` |
| Constants | `SCREAMING_SNAKE_CASE` | `DOGFOOD_FLAGS`, `MAX_FILES` |
| Context params | always `ctx`, always last | `fn render(&self, ctx: &AppContext)` |

### Style Examples

```rust
// ✅ Inline format args (required by clippy)
log::info!("{message}");
eprintln!("{error}");

// ❌ Positional format args
log::info!("{}", message);

// ✅ Exhaustive matching — no wildcard
match flag {
    FeatureFlag::AgentMode => { ... }
    FeatureFlag::DriveSync => { ... }
}

// ✅ Context param last
fn handle_event(&mut self, event: Event, ctx: &mut ViewContext<Self>) { ... }

// ✅ Closure last (exception to ctx-last rule)
fn with_view<F: FnOnce(&mut Self)>(&mut self, f: F, ctx: &mut AppContext) { ... }

// ✅ pub(crate) for internal APIs
pub(crate) fn internal_helper() { ... }
```

## 5. Logging

Use `log` crate macros only — no `println!` / `eprintln!` for runtime logging.

```rust
use log::{debug, info, warn, error};

log::info!("Session started: session_id={session_id}");
log::warn!("Config missing, using defaults: path={path}");
log::error!("Auth failed: {err}");
log::debug!("Frame rendered: elapsed={elapsed_ms}ms");
```

**Error reporting macros** (from `warp_core::errors`):
- `report_error!(err)` — actionable errors → Sentry + `error!`; non-actionable → `warn!` only
- `report_if_error!(result)` — calls `report_error!` only on `Err`

```rust
// At error boundaries
if let Err(err) = result {
    report_error!(err);
}

// Or inline
report_if_error!(do_thing());
```

## 6. Testing

### File Structure

```rust
// At the bottom of foo.rs:
#[cfg(test)]
#[path = "foo_tests.rs"]
mod tests;
```

Test file `foo_tests.rs` lives alongside `foo.rs` in the same directory.

### Test Pattern

```rust
// foo_tests.rs
use super::*;

#[test]
fn test_has_horizontal_split() {
    let state = AppState::new();
    assert!(!state.has_horizontal_split());
    assert_eq!(state.pane_count(), 1);
}
```

### Running Tests

```bash
cargo nextest run --no-fail-fast --workspace --exclude command-signatures-v2
cargo nextest run -p warp_completer --features v2   # single package
cargo test --doc                                     # doc tests
./script/presubmit                                   # full presubmit
```

Mocking: `mockito` for HTTP; test utilities gated behind `#[cfg(feature = "test-util")]`.

## 7. Error Handling

```rust
// Propagate with ? in library code
fn load_config(path: &Path) -> anyhow::Result<Config> {
    let content = std::fs::read_to_string(path)?;
    Ok(toml::from_str(&content)?)
}

// Domain-specific errors with thiserror
#[derive(Debug, thiserror::Error)]
enum AuthError {
    #[error("token expired")]
    TokenExpired,
    #[error("invalid credentials: {0}")]
    InvalidCredentials(String),
}

// At error boundaries: report then handle gracefully
if let Err(err) = result {
    report_error!(err);
}
```

- `anyhow::Result` for functions crossing crate boundaries
- `thiserror` for domain errors within a crate
- `unwrap()` only in tests or guaranteed-valid startup paths; use `expect("reason")` if used at all

## 8. Feature Flags

```rust
// 1. Add variant to warp_core/src/features.rs
pub enum FeatureFlag {
    YourNewFeature,
}

// 2. Optionally enable for dogfood
pub const DOGFOOD_FLAGS: &[FeatureFlag] = &[
    FeatureFlag::YourNewFeature,
];

// 3. Gate code at runtime — never use #[cfg] unless code won't compile otherwise
if FeatureFlag::YourNewFeature.is_enabled() {
    // new behavior
}

// 4. Promote: DOGFOOD_FLAGS → PREVIEW_FLAGS → RELEASE_FLAGS
// 5. Remove flag + dead branches after launch stabilizes
```

## 9. Development Commands

```bash
# Build & run
cargo run
cargo bundle --bin warp                       # macOS app bundle
cargo run --features with_local_server        # connect to local warp-server

# Format & lint (required before every PR)
cargo fmt
cargo clippy --workspace --all-targets --all-features --tests -- -D warnings

# Full presubmit
./script/presubmit

# Tests
cargo nextest run --no-fail-fast --workspace --exclude command-signatures-v2
cargo test --doc

# C/ObjC/C++ format
./script/run-clang-format.py -r --extensions 'c,h,cpp,m' ./crates/warpui/src/ ./app/src/

# WGSL shader format check
find . -name "*.wgsl" -exec wgslfmt --check {} +

# Setup
./script/bootstrap
./script/install_cargo_build_deps
./script/install_cargo_test_deps
```

## 10. AI Coding Assistant Instructions

- **Read this file first** on every task — follow all rules before writing code
- **Run `cargo fmt` + `cargo clippy` before every PR** — both must pass with zero warnings
- **Inline format args always** — `"{var}"` not `"{}", var` in every macro invocation
- **Never use `_` wildcards in match** — handle all variants explicitly; add arms when enum grows
- **Never prefix unused params with `_`** — delete the param and update all call sites
- **Never use `std::process::Command` or `std::time::Instant`** — use warp's cross-platform wrappers
- **Feature flags are runtime, not compile-time** — `FeatureFlag::X.is_enabled()` over `#[cfg(...)]`
- **Context params go last, named `ctx`** — closure params go last instead when one is present
- **Unit tests go in sibling files** — `foo_tests.rs` next to `foo.rs`, included via `#[path = ...]`
- **No `dbg!` in committed code** — clippy rejects it; remove before any commit
