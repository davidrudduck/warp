# Comprehensive Fix Plan - All Identified Issues
**Date**: 2026-05-09  
**Scope**: Fix all 14+ issues from adversarial review + implement deferred Settings UI

---

## Fix Groups (Parallel Execution)

### Group 1: Critical Concurrency Fixes
**Agent**: Opus Executor  
**Files**: `direct_loop/mod.rs`, `conversation/repository.rs`

1. **Cancellation Race** (Critical)
   - Add `CancellationToken` to `ToolDispatchRequest`
   - Propagate token to executor
   - Return `Err(ProviderError::Cancelled)` instead of `Ok(())`
   
2. **SQLite Contention + Panics** (High)
   - Add helper: `fn establish_connection() -> Result<SqliteConnection>`
   - Set `PRAGMA busy_timeout = 5000;`
   - Set `PRAGMA journal_mode = WAL;`
   - Replace `db_path.to_str().unwrap()` → `to_str().ok_or_else(...)?` (5 locations)
   - Add connection pooling with `r2d2`

3. **Tool Result Ordering** (Medium)
   - Enumerate before partition
   - Preserve original indices through dispatch

---

### Group 2: Performance Optimizations
**Agent**: Gemini Executor  
**Files**: `logging/mod.rs`, `conversation/repository.rs`, `direct_loop/mod.rs`

4. **Regex Hot Path** (High)
   - Add `once_cell = "1.20"` to Cargo.toml
   - Use `Lazy<Regex>` for all 4 patterns
   
5. **N+1 INSERT** (Medium)
   - Batch insert: `Vec<NewDirectMessage>` → single query
   
6. **Message Clone Optimization** (Medium)
   - Change `trim_to_context_window` to take `&[ChatMessage]`
   - Clone only retained messages
   
7. **String Allocations** (Low)
   - Pass owned strings to spawn_blocking closures
   
8. **Tool Dispatch Clone** (Low)
   - Take ownership instead of cloning

9. **Async Logging** (Low)
   - Make `log()` async with `spawn_blocking`
   
10. **RefCell Documentation** (Medium)
    - Add SAFETY comment explaining main-thread invariant
    - Consider `OnceCell` alternative

---

### Group 3: Code Quality (Codex Findings)
**Agent**: Code Reviewer  
**Files**: TBD (waiting for Codex completion)

11. Hardcoded context window (100) → Config
12. Hardcoded MAX_DIRECT_LOOP_TURNS (50) → Config
13. Database paths → Config
14. Log paths → Config
15. Any TODOs or logic errors

---

### Group 4: Settings UI (Wrongly Deferred)
**Agent**: UI Specialist  
**Files**: `app/src/settings_view/direct_api.rs`, `app/src/conversation_sidebar/mod.rs`

16. **Settings Page** (Essential for OSS usability)
    - Provider dropdown (OpenAI, Anthropic, Ollama, Gemini)
    - API key input (with masking)
    - Test Connection button (validates key)
    - Save to keychain button
    - WarpUI components: `v_stack`, `h_stack`, `dropdown`, `text_input`, `button`
    
17. **Conversation Sidebar** (Nice-to-have)
    - Load recent 50 conversations
    - Display with title + message count
    - Click to resume
    - Archive button

---

## Implementation Strategy

### Phase 1: Concurrent Fixes (Groups 1-2)
- Launch 2 parallel agents:
  - Agent A: Critical concurrency (Issues 1-3)
  - Agent B: Performance optimizations (Issues 4-10)
- Expected time: 30-45 minutes (parallel)

### Phase 2: Settings UI (Group 4)
- Launch UI agent after Phase 1 completes
- Implement Settings page first (essential)
- Conversation sidebar second (nice-to-have)
- Expected time: 30-45 minutes

### Phase 3: Code Quality (Group 3)
- Fix Codex findings when available
- Expected time: 15-30 minutes

### Total Estimated Time: 1.5-2 hours

---

## Verification Plan

After each phase:
1. `cargo build --bin warp-oss` (zero warnings)
2. `cargo test -p ai --lib` (all tests pass)
3. `cargo test -p persistence --lib` (all tests pass)
4. `cargo clippy -- -D warnings` (zero warnings)

Final verification:
1. Run E2E tests with real API keys
2. Manual test: Configure provider via Settings UI
3. Manual test: Save/resume conversation

---

## Dependencies to Add

```toml
# crates/ai/Cargo.toml
[dependencies]
once_cell = "1.20"
tokio-util = { version = "0.7", features = ["sync"] }

# crates/persistence/Cargo.toml
[dependencies]
r2d2 = "0.8"
```

---

## Expected Outcomes

**Before**:
- 1 Critical issue (side-effect leaks)
- 3 High issues (contention, panics, regex)
- 4 Medium issues (ordering, clones, inserts)
- 6 Low issues (minor inefficiencies)
- 0 Settings UI (unusable without it)

**After**:
- ✅ All concurrency issues resolved
- ✅ All performance bottlenecks optimized
- ✅ All code quality issues fixed
- ✅ Settings UI functional
- ✅ OSS fork fully usable without backend server

---

## Risk Mitigation

- Each agent works in isolation (no conflicts)
- TDD workflow ensures correctness
- Can rollback via git if issues arise
- All changes tested before commit
