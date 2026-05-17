# Model Selection Section (to be merged)

## Selecting a Model

Direct API model use has two layers:

1. **Settings -> Agents -> Direct API** configures provider keys, base URLs, and cached model lists.
2. **Settings -> Agents -> Profiles** chooses whether each execution profile uses **Warp Provider** or **Direct API** routing.

To use Direct API for Agent Mode, edit the profile and set **Model Routing** to **Direct API**. Then choose a model from **Direct API model**. Profile choices are displayed as `Provider / Model`, for example `OpenAI / gpt-4o-mini`, `Anthropic / claude-3-5-sonnet-20241022`, or `Ollama / llama3`.

Warp Provider remains the default for existing and new profiles. Direct API keys stay local in the channel-specific settings file and are not sent to Warp server requests when a profile routes through Direct API.

Per-provider model selection allows you to choose which specific model to use for each provider. This is useful when:

- You want to use a faster/cheaper model for quick tasks
- You need a specific model for certain capabilities
- You're testing different models for comparison

### Accessing Model Selection

1. Open Warp Settings -> Agents -> Direct API.
2. Find the provider row you want to configure.
3. Enter and save your API key or base URL if not already done.
4. Click **Refresh models** in that provider row to update or confirm its available model list.
5. Open Warp Settings -> Agents -> Profiles.
6. Edit a profile, set **Model Routing** to **Direct API**, then choose the desired `Provider / Model`.

### Updating the Model List

The first time you use a provider, the model list may be empty or show defaults. To fetch the latest models:

**Step 1: Click "Refresh models"**

This button appears in each provider row. Clicking it will:
- Contact the provider's API to fetch available models
- Cache the results locally (refreshes every 24 hours)
- Populate the model dropdown

**What happens during update:**
```text
Fetching models... (shows loading state)
↓
✓ OpenAI access validated. Fetched 12 models.
↓
Models appear in dropdown
```

**Step 2: Select Your Model in a Profile**

Once the list is populated, open Settings -> Agents -> Profiles and switch a profile to Direct API. The profile model dropdown shows provider-qualified choices:

**OpenAI** (example):
- gpt-4o-mini (default, recommended)
- gpt-4o
- gpt-4-turbo
- gpt-3.5-turbo

**Anthropic** (example):
- claude-3-5-sonnet-20241022 (default, recommended)
- claude-3-opus-20240229
- claude-3-haiku-20240307

**Google Gemini** (example):
- gemini-2.0-flash (default, recommended)
- gemini-1.5-pro

**Step 3: Save the Profile**

Your profile model choice persists across:
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

**When to Refresh Models:**
- First time setup: Always refresh to see latest models
- After 24 hours: Cache expires, click to refresh
- New model release: Refresh manually to see it

**Troubleshooting:**

| Issue | Solution |
|---|---|
| "Refresh models" does nothing | Check API key is saved first |
| Model list empty | Click "Refresh models", wait 2-5 seconds |
| Old models showing | Model list cached, click refresh to update |
| Can't select model | Ensure DirectApiModelSelection feature enabled |

### Advanced: Model List Cache

On macOS OSS builds, models are cached at:
```bash
~/Library/Application Support/dev.warp.WarpOss/direct_api/models.json
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
1. Click "Refresh models" in UI, OR
2. Delete cache file manually (not recommended)

### Feature Flag

Model selection is gated behind `FeatureFlag::DirectApiModelSelection` in DOGFOOD_FLAGS. If the model selector is hidden, the feature is disabled in your build.
