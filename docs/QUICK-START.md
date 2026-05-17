# Quick Start — Direct API in 5 Minutes

Get started with Direct API and configure your first LLM provider.

## Prerequisites

- Warp OSS fork (built from source)
- API key from your chosen provider (OpenAI, Anthropic, Google Gemini, or Ollama)

## Setup Steps

### 1. Get Your API Key

Choose your provider:

**OpenAI** (GPT-4o, GPT-4 Turbo)
- Go to https://platform.openai.com/api-keys
- Create a new API key
- Format: starts with `sk-`

**Anthropic** (Claude 3.5 Sonnet, Claude 3 Opus)
- Go to https://console.anthropic.com/keys
- Create a new API key
- Format: starts with `sk-ant-`

**Google Gemini** (Gemini 2.0 Flash)
- Go to https://aistudio.google.com/app/apikey
- Create a new API key
- No key format restrictions

**Ollama** (Local LLM)
- Download from https://ollama.ai
- Run `ollama pull llama2` (or your preferred model)
- No API key needed

### 2. Open Warp Settings

1. Launch Warp
2. Press `Cmd+,` (Mac) or `Ctrl+,` (Linux/Windows)
3. Navigate to **Agents → Direct API**

### 3. Configure Your API Key

1. Select your provider from the dropdown
2. Paste your API key (or leave blank for Ollama)
3. For Ollama/OpenRouter/custom: Enter base URL if not default
4. Click **Test Connection** to validate local key and base URL format
5. Click **Save Settings**
6. Click **Refresh models** to validate provider access and populate models

### 4. Start Using AI

In any Warp terminal:

```bash
# Use Claude Code to work with your configured API
claude-code --help
```

Or use Warp's built-in Agent Mode:

```bash
@agent help me install Node.js
```

## Rig Agent Backend

Direct API profiles can optionally expose a Rig Agent backend when Warp is built with Rig support. The setting is off by default:

```toml
[agents.direct_api.experimental]
rig_backend_enabled = false
```

When enabled, Direct API profiles show `Agent engine: Native / Rig Agent`. Use `Native` unless testing Rig provider streaming.

## Troubleshooting

**"Failed to connect" error**
- Verify your API key is correct (no extra spaces)
- Check your provider's API status page
- For Ollama: ensure it's running on localhost:11434

**Settings are not saved**
- Confirm the `warp-oss` build can write to `~/.warp-oss/settings.toml`
- Restart Warp after fixing file permissions

**macOS asks for Keychain access**
- Direct API provider keys are stored in `~/.warp-oss/settings.toml`, not Keychain
- Other features may still read Keychain-backed secure storage
- Official Warp and Warp OSS have different app identities, so Keychain approval for official Warp does not approve `dev.warp.WarpOss`
- Use a stable signing identity for local builds:

```bash
WARP_CODESIGN_IDENTITY="Apple Development: Your Name (TEAMID)" ./script/run --dont-open
```

If no Apple Development identity is available and `WARP_CODESIGN_IDENTITY` is unset, the local scripts fall back to ad-hoc signing. That can run locally, but it is weaker for Keychain prompt diagnosis.

**"API key not configured" when running commands**
- Go to Settings → Agents → Direct API and verify the key is saved
- Test the connection before using

## What's Next?

- Read the [Direct API User Guide](./features/direct-api-user-guide.md) for detailed setup
- Read the [Direct API Profile Routing Guide](./features/direct-api-profile-routing.md) for per-profile routing and Rig Agent backend notes
- See [Direct API Developer Guide](./features/direct-api-developer-guide.md) for architecture details
- For general Warp docs, visit [docs.warp.dev](https://docs.warp.dev/)

---

**Estimated time**: 5 minutes  
**Last updated**: 2026-05-16
