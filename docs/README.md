# Warp Documentation

Welcome to the Warp documentation. This directory contains comprehensive guides for users and developers.

## Quick Links

- [Quick Start Guide](./QUICK-START.md) — Get Warp running in 5 minutes
- [Direct API User Guide](./features/direct-api-user-guide.md) — Configure your own LLM provider
- [Direct API Developer Guide](./features/direct-api-developer-guide.md) — Technical implementation details

## Features

### Direct API

The Direct API feature allows OSS fork users to configure their own LLM provider API keys directly in Warp Settings. This enables:

- Use any OpenAI-compatible LLM provider (OpenAI, Anthropic, Google Gemini, Ollama, OpenRouter, custom)
- Local-only API key storage in `~/.warp-oss/settings.toml`
- Full conversation history persistence
- No dependency on Warp cloud backend

Start with the [Direct API User Guide](./features/direct-api-user-guide.md) for setup instructions.

## Documentation Structure

```text
docs/
├── README.md                           # This file
├── QUICK-START.md                      # 5-minute setup guide
└── features/
    ├── direct-api-user-guide.md        # User-friendly setup + troubleshooting
    └── direct-api-developer-guide.md   # Technical architecture + implementation
```

## Finding Help

- See [docs.warp.dev](https://docs.warp.dev/) for official Warp documentation
- Ask questions in the [Warp Slack community](https://go.warp.dev/join-preview)
- For this Open Source fork, see [CONTRIBUTING.md](../CONTRIBUTING.md)

---

Last updated: 2026-05-11
