# Direct API - Open Questions & Deferred Items

This document tracks unresolved questions and items deferred to future iterations.

---

## Phase 2: Model Selection - Deferred Items

### Provider Coverage (V2)

**Ollama Model Listing**
- **Status**: Deferred to V2
- **Reason**: Requires integration with local Ollama tags API (`ollama list`)
- **Implementation**: Call `ollama list --format json` and parse model names
- **Timeline**: Week 5-6 (V2 enhancements)

**OpenRouter Model Listing**
- **Status**: Deferred to V2
- **Reason**: Requires `/v1/models` endpoint integration
- **Implementation**: Similar to OpenAI provider (HTTP GET with bearer auth)
- **Timeline**: Week 5-6 (V2 enhancements)

**Custom Provider**
- **Status**: Not applicable
- **Reason**: Custom endpoints are user-configured, no standard discovery mechanism
- **Workaround**: Users manually specify model ID in conversation creation

### Model Validation (V2)

**Validate Model on Conversation Start**
- **Question**: Should we validate that the selected model exists before starting a conversation?
- **Current Behavior**: Trust user selection, fail at first API call if model unavailable
- **Proposed**: Add optional pre-flight check (`HEAD /v1/models/{model_id}`)
- **Tradeoff**: Adds latency to conversation start, but catches errors earlier
- **Decision**: Defer to V2, current behavior acceptable for MVP

**Graceful Degradation for Unavailable Models**
- **Question**: What should happen if a selected model is unavailable (deprecated, region-locked, etc)?
- **Current Behavior**: API call fails with error
- **Proposed Options**:
  1. Auto-fallback to provider default model
  2. Show error + suggest alternative models
  3. Cache last-known-good model list and warn if selection not in list
- **Decision**: Defer to V2, implement option 2 (error + suggestions)

### UI Enhancements (V3)

**Model Cost/Capability Metadata**
- **Question**: Should we show model pricing, context window, and capabilities in the UI?
- **Current Behavior**: Model dropdown shows only model IDs
- **Proposed**: Add tooltip/subtitle with:
  - Input/output pricing (per 1M tokens)
  - Context window size
  - Capabilities (vision, tool use, etc)
- **Data Source**: Static JSON file or provider API
- **Timeline**: V3 (weeks 9-12)

**Model Deprecation Warnings**
- **Question**: How should we notify users when a model is deprecated?
- **Current Behavior**: No deprecation tracking
- **Proposed**: 
  - Fetch deprecation timeline from provider API
  - Show warning badge in UI
  - Suggest migration path to newer model
- **Timeline**: V3 (weeks 9-12)

### Concurrency & Performance (Phase 2 - In Progress)

**Fetch Concurrency Control**
- **Status**: US-011 (in progress)
- **Question**: Should we allow multiple concurrent model list fetches?
- **Current Behavior**: No guard, possible duplicate fetches
- **Proposed**: Add `fetch_in_flight: Cell<bool>` guard
- **Timeline**: Complete in Phase 2

**CancellationToken for In-Flight Fetches**
- **Status**: US-011 (in progress)
- **Question**: Should users be able to cancel a slow model list fetch?
- **Current Behavior**: No cancellation, must wait for timeout
- **Proposed**: Integrate `CancellationToken` from tokio-util
- **Timeline**: Complete in Phase 2

**Stale Model Handling**
- **Status**: US-016 (in progress)
- **Question**: What should happen if cached model is no longer available?
- **Current Behavior**: Undefined (not yet implemented)
- **Proposed**: Fallback policy:
  1. Try selected model first
  2. If 404, fall back to provider default
  3. Log warning for telemetry
- **Timeline**: Complete in Phase 2

---

## Future Considerations

### Multi-Model Conversations (V3)

**Question**: Should we support switching models mid-conversation?
- **Use Case**: Start with fast model (Haiku), escalate to powerful model (Opus) for complex queries
- **Challenge**: Model-specific context window limits, tool compatibility
- **Timeline**: V3 or later

### Model Comparison Mode (V3)

**Question**: Should we support running the same prompt on multiple models side-by-side?
- **Use Case**: A/B testing, finding best model for a task
- **Challenge**: Cost (2-4× API calls), UI complexity
- **Timeline**: V3 or later

### Model Fine-Tuning Integration (Long-term)

**Question**: Should we support OpenAI/Anthropic fine-tuned models?
- **Use Case**: Domain-specific models for enterprise users
- **Challenge**: Requires org-level API key management, billing
- **Timeline**: Long-term (outside OSS fork scope)

---

## Documentation Gaps

All Phase 2 documentation is now complete:
- ✅ User guide: "Selecting a Model" section added
- ✅ Developer guide: Phase 2 architecture section added
- ✅ PLAN-OSS-TDD.md: Phase 2 status section added
- ✅ This file: Deferred items documented

---

**Last Updated**: 2026-05-13  
**Next Review**: Week 5 (V2 planning)
