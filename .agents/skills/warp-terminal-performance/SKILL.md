---
name: warp-terminal-performance
description: Reviews Warp performance-sensitive terminal and agent changes. Use when changing rendering, terminal/session models, streaming output, async tasks, filesystem scans, settings pages, model caches, logging, or any path that can affect typing latency or UI responsiveness.
---

# Warp Terminal Performance

Use this skill to keep Warp feeling immediate. The terminal and agent UI should stay responsive while background work runs.

## Hot Surfaces

- terminal/session models and block list updates
- agent streaming and action queues
- settings pages with dynamic provider/model data
- file scans, grep, indexing, and workspace reads
- logging and redaction
- persistence and SQLite access
- global model events and view invalidation

## Review Workflow

1. Identify whether code runs on the UI/model path, async task, blocking task, or render path.
2. Search for blocking work:
   - filesystem IO
   - network calls
   - SQLite work
   - regex over large strings
   - shell/process execution
   - lock acquisition
3. Move blocking work behind existing async or `spawn_blocking` patterns when needed.
4. Keep lock scopes minimal, especially `TerminalModel.lock()`.
5. Avoid repeated allocations or parsing in render methods.
6. Cache only with invalidation rules and bounded lifetime.
7. Check event emission frequency. Avoid rerender storms from per-token or per-line updates unless the UI is designed for it.

## Red Flags

- Synchronous IO in a `View::render`, `ModelContext` update, or hot terminal path.
- Long-held `Arc<Mutex<_>>`, `parking_lot` locks, or nested terminal locks.
- Unbounded Vec/String growth for streams, logs, model lists, or histories.
- Recomputing layout-heavy data every render.
- Logging on every small stream chunk without buffering or redaction cost awareness.
- Sleeping in tests or runtime code instead of polling state.

## Validation

Choose proof by risk:

- compile only for low-risk refactors: `cargo check -p warp --bin warp-oss`
- targeted unit tests for cache/invalidations
- integration tests for terminal-visible regressions
- manual smoke for typing/streaming responsiveness when UI feel matters

When deeper profiling is needed, search existing performance hooks:

```bash
rg -n "performance sample|FOR PERFORMANCE BOT|pprof|tracing|Timer|spawn_blocking" app crates
```

External reference for UX-oriented performance thinking: https://web.dev/articles/rail
