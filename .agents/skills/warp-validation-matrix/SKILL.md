---
name: warp-validation-matrix
description: Selects and runs the right validation for Warp changes. Use before claiming completion, before committing, or when deciding between cargo check, targeted tests, integration tests, UI/manual verification, clippy, fmt, or presubmit in this repository.
---

# Warp Validation Matrix

Use this skill to choose evidence that matches the risk of a Warp change. Do not default to the heaviest command; select the smallest set that proves the changed behavior.

## First Pass

1. Identify the changed surface:
   - Rust formatting or imports only
   - crate logic
   - settings or `settings.toml`
   - UI or WarpUI view/model behavior
   - terminal/session behavior
   - Direct API or agent streaming
   - security, secrets, auth, MCP, shell execution, or local files
2. Identify the proof needed:
   - compile proof
   - unit proof
   - file-backed persistence proof
   - UI/model proof
   - integration proof
   - security/redaction proof
3. Run fresh commands in this turn before claiming the result.

## Default Commands

- Formatting: `cargo fmt --check`
- Whitespace: `git diff --check`
- OSS compile path: `cargo check -p warp --bin warp-oss`
- Targeted crate tests: `cargo test -p <crate> <test_filter> -- --nocapture`
- Full crate tests: `cargo test -p <crate>`
- Nextest target: `cargo nextest run -p <crate>`
- Full workspace when broad risk warrants it: `cargo nextest run --no-fail-fast --workspace --exclude command-signatures-v2`

Use `cargo check -p warp --bin warp-oss` for app-level OSS work. The local `dev` binary can depend on private channel config and is not the best proof for this fork.

## Matrix

| Change type | Minimum useful proof |
|---|---|
| Docs only | `git diff --check` |
| Rust local function | targeted `cargo test -p <crate> <filter>` plus `cargo fmt --check` |
| Public type or enum | targeted tests plus `cargo check -p warp --bin warp-oss` |
| Settings schema | file-backed TOML test, settings tests if touched, `cargo check -p warp --bin warp-oss` |
| Direct API | `api_keys` tests, provider/model tests touched, redaction tests if logs touched |
| UI pure helpers | unit tests for helper behavior |
| UI view/model state | `warpui::App::test` or existing view tests; integration only for real user flow |
| Terminal/session flow | targeted terminal tests or integration test when PTY/UI behavior matters |
| Agent streaming/tool calls | stream/tool/cancellation tests plus app compile |
| Security/secrets | targeted redaction/storage/path tests plus manual search for logging and cleartext exposure |

## Completion Report

When finishing, report:

- commands run
- pass/fail result
- any warnings that remain and whether they are pre-existing
- any validation intentionally skipped and why

Never say work is complete from inspection alone when a command can prove it cheaply.
