---
name: warp-oss-local-first
description: Keeps Warp OSS work local-first and server-independent. Use when changing AI, agents, settings, auth-adjacent code, Direct API, paths, build behavior, or anything that might assume proprietary Warp services, official Warp state, or code outside this repository.
---

# Warp OSS Local First

This repository is `warp-oss`, a private fork. Do not assume access to Warp server infrastructure, private APIs, or source outside this checkout.

## Required Orientation

Before designing or implementing, verify the relevant local code path:

- OSS binary entrypoint: `app/src/bin/oss.rs`
- channel/path logic: `crates/warp_core/src/paths.rs`
- local settings path: `app/src/settings/mod.rs`
- Direct API docs and settings: `docs/`, `crates/settings/src/direct_api.rs`, `crates/ai/src/api_keys.rs`

For macOS, `Channel::Oss` maps config storage to `~/.warp-oss`. Official stable Warp uses `~/.warp`. Do not overwrite or migrate official Warp state unless the user explicitly asks.

## Design Rules

- Prefer local Direct API behavior over Warp cloud assumptions.
- Treat features as locked to code present in this repository.
- Do not design around private server endpoints, proprietary GraphQL schema behavior, remote Oz services, or missing crates.
- If a feature path requires a Warp server, surface that as a blocker or design a local-only fallback.
- Keep `~/.warp-oss/settings.toml` separate from official `~/.warp`.
- Use build/runtime channel checks already present in the repo instead of ad hoc path checks.

## Validation

Use `warp-validation-matrix` and include at least:

```bash
cargo check -p warp --bin warp-oss
```

For settings or AI provider changes, also use targeted tests that prove local settings, provider, or persistence behavior.

## Review Questions

- Does this change work without Warp cloud access?
- Does it read or write only the OSS channel path?
- Does it require code or services outside this repository?
- Does it preserve fast local terminal and agent workflows?
- Does user-facing copy avoid promising official Warp behavior that this fork cannot provide?
