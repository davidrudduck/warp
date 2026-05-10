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
4. Click **Test Connection** to verify
5. Click **Save to Keychain**

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

## Troubleshooting

**"Failed to connect" error**
- Verify your API key is correct (no extra spaces)
- Check your provider's API status page
- For Ollama: ensure it's running on localhost:11434

**Keychain prompt appearing repeatedly**
- macOS Keychain prompt only appears once per session
- Restart Warp if you see it again

**"API key not configured" when running commands**
- Go to Settings → Agents → Direct API and verify the key is saved
- Test the connection before using

## What's Next?

- Read the [Direct API User Guide](./features/direct-api-user-guide.md) for detailed setup
- See [Direct API Developer Guide](./features/direct-api-developer-guide.md) for architecture details
- For general Warp docs, visit [docs.warp.dev](https://docs.warp.dev/)

---

**Estimated time**: 5 minutes  
**Last updated**: 2026-05-11
