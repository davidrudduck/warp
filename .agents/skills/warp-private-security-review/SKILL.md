---
name: warp-private-security-review
description: Reviews practical security risks for this private-network Warp OSS fork. Use when changing secrets, API keys, logs, settings, local files, shell execution, MCP/OAuth, HTTP endpoints, agent tools, prompt context, or data sent to model providers.
---

# Warp Private Security Review

This is not an enterprise compliance workflow. Calibrate findings for a privately run local app on a private network. Still treat secrets, shell execution, local files, and agent tool exposure as high-impact local risks.

## Threat Model

Assume:

- single primary user or trusted private-network use
- no public internet hosting of this app
- local filesystem, shell, terminal history, settings, and logs can contain sensitive data
- LLM providers and MCP servers may receive prompt/tool context

Do not over-index on SaaS multi-tenant controls. Do check for realistic local leakage and unsafe automation.

## Review Checklist

Secrets and logs:

- Search for API keys, tokens, auth headers, SSH material, local paths, or command output written to logs.
- Verify redaction tests cover provider-specific keys and generic bearer/JWT-like strings when log code changes.
- Avoid printing secrets in errors, telemetry, debug logs, or test snapshots.

Settings and storage:

- For Direct API in warp-oss, settings intentionally live in `~/.warp-oss/settings.toml`.
- Do not write Direct API configuration to official `~/.warp`.
- For non-Direct-API auth/MCP credentials, verify the existing storage boundary before moving anything to plaintext.

Network and endpoints:

- HTTPS should be required for remote endpoints.
- Plain HTTP should be limited to localhost or private LAN ranges where explicitly intended.
- Custom base URLs must be validated before use.

Agent and shell tools:

- Unknown or side-effecting tools should require confirmation.
- File edits, shell commands, computer use, MCP tool calls, and artifact uploads need clear user control.
- Prompt/context assembly must not include secrets unless required and expected.

Local files:

- Normalize and validate paths before writes.
- Avoid writing outside the intended Warp OSS config/state directories unless the user chooses the path.
- Be careful with recursive scans and symlink traversal.

## Evidence To Collect

Use `rg` for direct evidence:

```bash
rg -n "api_key|secret|token|Authorization|Bearer|secure_storage|settings.toml|http://|Command|shell|write_all|log::|telemetry" app crates
```

Pair code inspection with targeted tests when changing behavior.

External reference for deeper checklisting: https://owasp.org/www-project-application-security-verification-standard/
