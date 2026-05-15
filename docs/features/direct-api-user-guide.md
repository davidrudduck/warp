# Direct API User Guide

## What is Direct API?

Direct API allows you to configure your own LLM provider API keys directly in Warp, without relying on Warp's cloud infrastructure. This is ideal for:

- Open-source fork users who want full control over their AI
- Users who prefer local LLMs (Ollama)
- Teams managing API keys via existing infrastructure
- Privacy-conscious users who avoid cloud intermediaries

### Key Benefits

- **Offline-capable**: Use local models like Ollama without internet
- **Privacy**: API keys stored locally in the channel-specific settings file
- **Multi-provider**: OpenAI, Anthropic, Google Gemini, Ollama, OpenRouter, custom endpoints
- **Conversation persistence**: Full chat history saved locally, survives app restarts
- **No cloud dependency**: Works entirely offline with Ollama

## Supported Providers

| Provider | API Key Required? | Models | Setup Time |
|---|---|---|---|
| **OpenAI** | Yes | GPT-4o, GPT-4 Turbo, GPT-3.5 Turbo | 2 min |
| **Anthropic** | Yes | Claude 3.5 Sonnet, Claude 3 Opus, Claude 3 Haiku | 2 min |
| **Google Gemini** | Yes | Gemini 2.0 Flash, Gemini 1.5 Pro | 2 min |
| **Ollama** | No | llama2, mistral, neural-chat (local) | 5 min |
| **OpenRouter** | Yes | 100+ models (Meta, Mistral, etc.) | 2 min |
| **Custom (OpenAI-compatible)** | Optional | Any OpenAI-compatible endpoint | 2 min |

## Accessing Direct API Settings

Direct API setup has two parts:

1. Configure provider keys and base URLs in **Settings -> Agents -> Direct API**.
2. Enable Direct API per execution profile in **Settings -> Agents -> Profiles**.

Warp Provider remains the default route for existing and new profiles until a profile is explicitly switched to Direct API.

### Step 1: Open Settings

**macOS**
```text
Warp → Settings (Cmd+,)
```

**Linux/Windows**
```text
Warp → Settings (Ctrl+,)
```

### Step 2: Navigate to Agents

In the Settings sidebar:
1. Click **Agents** section
2. Click **Direct API**

You should see a page showing which API keys are currently configured.

[Screenshot: Settings → Agents → Direct API]

### Step 3: Configure Provider

See provider-specific setup below.

## Using Direct API in an Agent Profile

After at least one Direct API provider is configured:

1. Open **Settings -> Agents -> Profiles**.
2. Open the profile you want to edit.
3. Set **Model Routing** to **Direct API**.
4. Choose a model from **Direct API model**. Choices are shown as `Provider / Model`, for example `OpenAI / gpt-4o-mini` or `Ollama / llama3`.
5. Save the profile.

When **Model Routing** is **Warp Provider**, the existing Warp-provided model controls are used. When **Model Routing** is **Direct API**, the profile routes Agent Mode requests locally through the selected Direct API provider/model.

Direct API keys stay in the channel-specific settings file and are not attached to Warp server requests for Direct API-routed profiles. For the warp-oss macOS build, the settings file is `~/.warp-oss/settings.toml`.

## Provider Setup Guides

### OpenAI

**Get Your API Key**

1. Go to https://platform.openai.com/api-keys
2. Click **Create new secret key**
3. Copy the key (format: `sk-...`)

**Enter in Warp**

1. Open Warp Settings → Agents → Direct API
2. Select **OpenAI** from the Provider dropdown
3. Paste your API key into the **API Key** field
4. Click **Test Connection**
5. You should see: ✓ API key format valid (full test pending)
6. Click **Save Settings**

**Choose a Model**

Default: `gpt-4o` (latest, recommended)

Other options:
- `gpt-4-turbo` — Better context understanding, ~2× slower
- `gpt-3.5-turbo` — Fastest, cheapest
- Full list: https://platform.openai.com/docs/models

**Pricing**

- GPT-4o: $5/1M input, $15/1M output tokens
- See https://openai.com/pricing for current rates

### Anthropic (Claude)

**Get Your API Key**

1. Go to https://console.anthropic.com/keys
2. Click **Create Key**
3. Copy the key (format: `sk-ant-...`)

**Enter in Warp**

1. Open Warp Settings → Agents → Direct API
2. Select **Anthropic** from the Provider dropdown
3. Paste your API key
4. Click **Test Connection**
5. You should see: ✓ API key format valid (full test pending)
6. Click **Save Settings**

**Choose a Model**

Default: `claude-3-5-sonnet-20241022`

Other options:
- `claude-3-opus-20240229` — Most capable, slower, more expensive
- `claude-3-haiku-20240307` — Fastest, cheapest
- Full list: https://docs.anthropic.com/claude/reference/models-overview

**Pricing**

- Claude 3.5 Sonnet: $3/1M input, $15/1M output tokens
- See https://www.anthropic.com/pricing for current rates

### Google Gemini

**Get Your API Key**

1. Go to https://aistudio.google.com/app/apikey
2. Click **Create API Key**
3. Copy the key

**Enter in Warp**

1. Open Warp Settings → Agents → Direct API
2. Select **Google Gemini** from the Provider dropdown
3. Paste your API key
4. Click **Test Connection**
5. You should see: ✓ API key format valid (full test pending)
6. Click **Save Settings**

**Choose a Model**

Default: `gemini-2.0-flash`

Other options:
- `gemini-1.5-pro` — Most capable
- Full list: https://ai.google.dev/gemini-api/docs/models/gemini

**Pricing**

- Free tier: 60 requests/minute
- Paid tier: $1.50/1M input tokens, $6/1M output tokens

### Ollama (Local LLM)

**Install Ollama**

1. Download from https://ollama.ai
2. Follow platform-specific installation
3. Start the Ollama service

**Pull a Model**

```bash
ollama pull llama2          # ~4GB, recommended for beginners
ollama pull mistral         # ~5GB, very fast
ollama pull neural-chat     # ~5GB, good for conversation
```

**Verify Installation**

```bash
curl http://localhost:11434/api/tags
```

You should see your downloaded models.

**Enter in Warp**

1. Open Warp Settings → Agents → Direct API
2. Select **Ollama** from the Provider dropdown
3. Leave **API Key** blank (not required)
4. **Base URL** should be `http://localhost:11434` (default)
5. Click **Test Connection**
6. You should see: ✓ Ollama runs locally - no API key needed
7. Click **Save Settings**

**Choose a Model**

Available models from `ollama pull` command:
- `llama2` — General purpose
- `mistral` — Fast reasoning
- `neural-chat` — Conversation optimized

**Advantages**

- ✓ Runs entirely offline (no internet needed)
- ✓ Free (no API costs)
- ✓ Private (models run on your machine)
- ✓ Fast (GPU-accelerated if available)

**Disadvantages**

- ✗ Slower than cloud models
- ✗ Requires 4GB+ RAM
- ✗ Requires GPU for acceptable performance

### OpenRouter

**Get Your API Key**

1. Go to https://openrouter.ai
2. Sign up or log in
3. Go to https://openrouter.ai/keys
4. Copy your API key

**Enter in Warp**

1. Open Warp Settings → Agents → Direct API
2. Select **OpenRouter** from the Provider dropdown
3. Paste your API key
4. **Base URL** should be `https://openrouter.ai/api/v1` (default)
5. Click **Test Connection**
6. You should see: ✓ API key format valid (full test pending)
7. Click **Save Settings**

**Choose a Model**

Popular models:
- `meta-llama/llama-2-70b-chat` — Fast, open-source
- `mistralai/mistral-7b-instruct` — Very fast, small
- `openai/gpt-4-turbo` — Route to OpenAI's GPT-4 via OpenRouter

Full model list: https://openrouter.ai/models

**Pricing**

Varies by model. See https://openrouter.ai/models for current rates.

### Custom (OpenAI-Compatible)

For endpoints compatible with OpenAI's API format (including LM Studio, Vllm, custom servers).

**Enter in Warp**

1. Open Warp Settings → Agents → Direct API
2. Select **Custom (OpenAI-compatible)** from the Provider dropdown
3. Enter your **API Key** (or leave blank if endpoint doesn't require auth)
4. Enter your **Base URL** (e.g., `http://localhost:8000`)
5. Click **Test Connection**
6. You should see: ✓ Custom provider configured (full test pending)
7. Click **Save Settings**

**Example: LM Studio**

```text
Base URL: http://localhost:1234/v1
API Key: (leave blank)
Model: llama-2-7b-chat.Q4_K_M
```

## Testing Your Connection

After entering your API key:

1. Click **Test Connection** button
2. Warp will validate:
   - API key format is correct
   - Required custom-provider base URL is present
   - Full provider reachability and authentication testing is pending
3. You'll see one of:
   - ✓ API key format valid (full test pending)
   - ✓ Ollama runs locally - no API key needed
   - ✓ Custom provider configured (full test pending)
   - ✗ Provider-specific key validation errors, such as a missing key or an unexpected key prefix
   - ✗ Base URL is required for custom providers

## Saving Settings

After a successful test:

1. Click **Save Settings**
2. Warp writes the Direct API configuration to the channel-specific settings file
3. For the warp-oss macOS build, that file is `~/.warp-oss/settings.toml`

## Using Your API Key

Once saved, your API key is available to execution profiles that use Direct API routing. It is not used by default.

To use it:

1. Open **Settings -> Agents -> Profiles**.
2. Edit the profile you use for Agent Mode.
3. Set **Model Routing** to **Direct API**.
4. Choose a `Provider / Model` value and save the profile.

Direct API keys are **never** sent to Warp's servers for Direct API-routed profiles. They are used directly with your configured provider.

## Conversation History

All conversations are saved locally in Warp's database:

- **Location**: `~/.warp-oss/` for warp-oss builds (hidden directory)
- **Persistence**: Survives app restarts
- **Privacy**: Stored only on your machine
- **Access**: View via Settings → Agents → Conversation History (future feature)

Each conversation includes:
- User messages
- Assistant responses
- Model used
- Creation date/time
- Token usage

## Security and Privacy

### API Key Storage

- **warp-oss macOS**: Stored in `~/.warp-oss/settings.toml`
- **official stable Warp macOS**: Uses the official channel path under `~/.warp/`
- **Linux/Windows**: Stored in the channel-specific settings path for that build

### What Warp Sees

Warp's OSS fork:
- Does NOT send API keys to Warp's cloud
- Does NOT log your conversations
- Does NOT track your API usage
- Is entirely open-source and auditable

### Best Practices

1. **Rotate keys regularly**: Generate new API keys periodically
2. **Use separate keys**: Create key-per-app or key-per-user
3. **Monitor usage**: Check your provider's dashboard for unexpected activity
4. **Never share keys**: Don't paste keys in public repos or chat

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

## Troubleshooting

### Common Issues

**Q: "Authentication failed" error during model refresh or Direct API requests**

- Check API key is exactly correct (no extra spaces)
- Verify key is still valid (not revoked)
- Check provider account has billing enabled
- For OpenAI: Verify organization access if using org API keys

**Q: "Connection failed" error during model refresh or Direct API requests**

- Check internet connection (unless using Ollama)
- For custom endpoints: Verify base URL is correct and server is running
- For Ollama: Ensure `ollama serve` is running in another terminal

**Q: Settings do not persist**

- Check that Warp can write to `~/.warp-oss/settings.toml`
- Check file permissions on `~/.warp-oss/`
- Restart Warp after fixing permissions

**Q: "Model not found" error**

- Verify model name matches provider's model list
- For Ollama: Run `ollama pull model-name` first
- Check model availability in your region (some may be geo-restricted)

**Q: Slow responses from Ollama**

- Ensure Ollama is using GPU (check Ollama settings)
- Try a smaller model (mistral, neural-chat instead of llama2)
- Check system resources (RAM, CPU) via Activity Monitor

**Q: API costs are higher than expected**

- Check Settings → Usage Dashboard for token counts
- Consider using a cheaper model (GPT-3.5, Haiku, Mistral)
- Use OpenRouter to compare model prices per request

### Getting Help

1. **Check logs**: use the normal app log for your build; on macOS, warp-oss writes `warp-oss.log` under `~/Library/Logs/`
2. **Join Slack**: https://go.warp.dev/join-preview
3. **File an issue**: https://github.com/warpdotdev/warp/issues
4. **Search existing issues**: https://github.com/warpdotdev/warp/issues?q=direct+api

## FAQ

**Q: Can I use multiple API keys?**

A: Yes. You can reconfigure any time. Warp stores the most recent configured key. Each conversation remembers which provider/model was used.

**Q: What if I don't have an API key?**

A: Use Ollama (free, local) or get a free tier key from OpenAI ($5 free credits) or Google Gemini (free tier available).

**Q: Is my API key safe?**

A: Keys are stored locally in the channel-specific settings file and never sent to Warp's servers. The app is open-source so you can verify this yourself.

**Q: Can I switch providers mid-conversation?**

A: Yes. Edit the active execution profile in Settings -> Agents -> Profiles, set Model Routing to Direct API, and choose a different `Provider / Model`. The new selection applies to new Agent Mode requests that use that profile; active provider calls already in flight are not changed.

**Q: What happens if I run out of API credits?**

A: Your requests will fail with an authentication error. Check your provider's dashboard and add billing or credits.

**Q: Do you support model switching?**

A: Yes. Model selection is supported per execution profile. Use Settings -> Agents -> Profiles, set Model Routing to Direct API, then choose the `Provider / Model` for that profile.

## Advanced Topics

### Custom OpenAI-Compatible Endpoints

Warp supports any OpenAI-API-compatible endpoint.

**Example: LM Studio**

1. Download LM Studio from https://lmstudio.ai
2. Load a model and start the local inference server
3. In Warp Settings:
   - Provider: Custom (OpenAI-compatible)
   - Base URL: `http://localhost:1234/v1`
   - API Key: (leave blank)
   - Model: (from LM Studio interface)

**Example: vLLM**

```bash
python -m vllm.entrypoints.openai.api_server \
  --model meta-llama/Llama-2-7b-hf \
  --port 8000
```

Then in Warp:
- Base URL: `http://localhost:8000/v1`

### Environment Variables

Direct API provider credentials are loaded from the channel-specific settings file through `DirectAPISettings`. Environment-variable overrides for Direct API keys or base URLs are not currently supported.

### Debug Logging

Direct API-specific debug log files are not currently wired in production builds. Use the normal app log for troubleshooting unless a developer has enabled a custom Direct API logger.

---

**Last updated**: 2026-05-11  
**Version**: Warp OSS v2024.05+  
**For official Warp docs**: https://docs.warp.dev/
