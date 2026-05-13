# Model Selection Section (to be merged)

## Selecting a Model

Phase 2 adds per-provider model selection, allowing you to choose which specific model to use for each provider. This is useful when:

- You want to use a faster/cheaper model for quick tasks
- You need a specific model for certain capabilities
- You're testing different models for comparison

### Accessing Model Selection

1. Open Warp Settings → Agents → Direct API
2. Select your provider (e.g., OpenAI, Anthropic)
3. Enter and save your API key if not already done
4. Look for the **"Available Models"** dropdown

### Updating the Model List

The first time you use a provider, the model list may be empty or show defaults. To fetch the latest models:

**Step 1: Click "Update Model List"**

This button appears below the provider selector. Clicking it will:
- Contact the provider's API to fetch available models
- Cache the results locally (refreshes every 24 hours)
- Populate the model dropdown

**What happens during update:**
```text
Fetching models... (shows loading state)
↓
✓ Found 12 models for OpenAI
↓
Models appear in dropdown
```

**Step 2: Select Your Model**

Once the list is populated, click the dropdown to see:

**OpenAI** (example):
- gpt-4o (default, recommended)
- gpt-4-turbo
- gpt-3.5-turbo

**Anthropic** (example):
- claude-3-5-sonnet-20241022 (default, recommended)
- claude-3-opus-20240229
- claude-3-haiku-20240307

**Google Gemini** (example):
- gemini-2.0-flash (default, recommended)
- gemini-1.5-pro

**Step 3: Save Selection**

Your model choice is saved automatically and persists across:
- App restarts
- Settings page navigation
- New conversations

### Per-Provider Defaults

If you don't select a model, Warp uses these defaults:

| Provider | Default Model | Reason |
|---|---|---|
| OpenAI | gpt-4o-mini | Fast, affordable |
| Anthropic | claude-3-5-sonnet-20241022 | Balanced capability |
| Google Gemini | gemini-2.0-flash | Latest, fast |
| Ollama | (none) | User configures local model |
| OpenRouter | (none) | 100+ options, user choice |
| Custom | (none) | Endpoint-specific |

### Model Selection Best Practices

**Performance vs Cost:**
- Fast tasks: Use -mini or -haiku variants
- Complex tasks: Use -turbo or -sonnet variants
- Maximum capability: Use -opus or GPT-4

**When to Update Model List:**
- First time setup: Always update to see latest models
- After 24 hours: Cache expires, click to refresh
- New model release: Update manually to see it

**Troubleshooting:**

| Issue | Solution |
|---|---|
| "Update Model List" does nothing | Check API key is saved first |
| Model list empty | Click "Update Model List", wait 2-5 seconds |
| Old models showing | Model list cached, click update to refresh |
| Can't select model | Ensure DirectApiModelSelection feature enabled |

### Advanced: Model List Cache

Models are cached at:
```bash
~/.cache/warp/model_cache.json
```

Structure:
```json
{
  "openai": {
    "models": [...],
    "fetched_at": "2026-05-13T03:00:00Z"
  }
}
```

Cache expires after 24 hours. To force refresh:
1. Click "Update Model List" in UI, OR
2. Delete cache file manually (not recommended)

### Feature Flag

Model selection is gated behind `FeatureFlag::DirectApiModelSelection` in DOGFOOD_FLAGS. If the model selector is hidden, the feature is disabled in your build.
