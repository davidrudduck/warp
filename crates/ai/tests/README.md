# E2E Integration Tests for Direct Provider API

This directory contains end-to-end integration tests that validate the full pipeline:
- Provider API calls (OpenAI, Anthropic, Ollama)
- Direct loop orchestration
- Conversation persistence
- Logging with secret redaction

## Running Tests

### All Tests (Non-Ignored)
```bash
cargo test -p ai --test e2e_direct_provider
```

### Ollama Local LLM Test
Requires Ollama running on `localhost:11434`:
```bash
# Start Ollama first
ollama serve

# Run test (auto-skips if Ollama not available)
cargo test -p ai --test e2e_direct_provider e2e_ollama_local_llm
```

### OpenAI Tests (Ignored by Default)
Requires `OPENAI_API_KEY` environment variable:
```bash
export OPENAI_API_KEY="sk-..."
cargo test -p ai --test e2e_direct_provider --ignored e2e_openai_conversation_with_persistence
cargo test -p ai --test e2e_direct_provider --ignored e2e_resume_conversation
```

### Anthropic Tests (Ignored by Default)
Requires `ANTHROPIC_API_KEY` environment variable:
```bash
export ANTHROPIC_API_KEY="sk-ant-..."
cargo test -p ai --test e2e_direct_provider --ignored e2e_anthropic_conversation
```

### Run All Tests (Including Ignored)
```bash
export OPENAI_API_KEY="sk-..."
export ANTHROPIC_API_KEY="sk-ant-..."
cargo test -p ai --test e2e_direct_provider --include-ignored
```

## Test Coverage

### `e2e_openai_conversation_with_persistence`
- ✅ Full OpenAI API integration (gpt-4o-mini)
- ✅ Message persistence to SQLite
- ✅ Auto-title generation
- ✅ Logging with API key redaction
- ✅ Event streaming (TextChunk, Done)
- ✅ Token usage tracking

### `e2e_ollama_local_llm`
- ✅ Local LLM integration (Ollama)
- ✅ Graceful skip if Ollama not running
- ✅ Conversation persistence
- ✅ Event completion

### `e2e_resume_conversation`
- ✅ Multi-turn conversations
- ✅ History loading from database
- ✅ Context continuation across turns
- ✅ Full conversation persistence

### `e2e_anthropic_conversation`
- ✅ Anthropic Claude API integration
- ✅ Message persistence
- ✅ API key redaction (sk-ant-*)
- ✅ Event streaming

## Test Structure

Each test:
1. Creates isolated temp directory with SQLite database
2. Initializes schema via Diesel
3. Creates logger with temp log directory
4. Spawns `direct_loop::run` in background
5. Collects events via mpsc channel
6. Verifies response content
7. Verifies persistence (messages, auto-title, token counts)
8. Verifies logging (file exists, secrets redacted)

## Notes

- Tests use `#[ignore]` for API key requirements
- Ollama test auto-detects availability via TCP connection
- All tests use isolated temp directories (cleanup automatic)
- API keys are redacted in logs (verified by tests)
- Tests use minimal context windows for speed
