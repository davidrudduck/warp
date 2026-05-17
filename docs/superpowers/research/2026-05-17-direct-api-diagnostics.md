# Direct API Diagnostics - 2026-05-17

## Environment

- Repo: `/Users/david/.codex/worktrees/d9a1/warp`
- Branch: `codex/direct-api-diagnosis-remediation`
- App bundle under test: `target/debug/bundle/osx/WarpOss.app`
- App bundle present at expected path: no
- Build features from launch or bundle command: unavailable because the app bundle was missing and the GUI flow was not rerun
- `~/.warp-oss/settings.toml` present: yes
- `~/Library/Logs/warp-oss.log` present: yes

## Settings Evidence

Sanitized `~/.warp-oss/settings.toml` Direct API entries:

```text
agents.direct_api.selected_provider = Custom
agents.direct_api.api_keys.open_router = <redacted len=73 prefix=sk-or-v1>
agents.direct_api.base_urls.ollama = http://localhost:11434
agents.direct_api.base_urls.custom = <redacted len=27 prefix=http???1>
agents.direct_api.base_urls.openrouter = https://openrouter.ai/api/v1
agents.direct_api.enabled_providers.Custom = true
agents.direct_api.experimental.rig_backend_enabled = true
```

Notes:

- The saved OpenRouter key has a plausible OpenRouter prefix and length.
- OpenRouter has a configured base URL of `https://openrouter.ai/api/v1`.
- `selected_provider = Custom` is present, but active profile Direct API model routing is stored outside this settings section. No `direct_api_model` profile entry was found under `~/.warp-oss` TOML or JSON files.
- The global Rig backend flag is enabled.

## Profile Evidence

No local TOML or JSON profile record containing `direct_api_model`, `OpenRouter`, `moonshot`, `kimi`, or `direct_api_agent_backend` was found under `~/.warp-oss`.

The user-supplied screenshot shows:

- Model routing: Direct API
- Direct API model: OpenRouter / `moonshotai/...`
- Agent engine options: Native and Rig Agent
- The selected engine is not visually obvious from the screenshot.

## OpenRouter Auth Probe

Command shape:

```bash
perl -0ne '...' "$HOME/.warp-oss/settings.toml" > "$tmp_curl_config"
curl -sS -D /tmp/openrouter-key.headers -o /tmp/openrouter-key.body \
  https://openrouter.ai/api/v1/key \
  --config "$tmp_curl_config"
```

The bearer token was passed through a `0600` temporary curl config file so it did not appear in shell output or curl process arguments.

Sanitized result:

```text
status=401
{"error":{"message":"User not found.","code":401}}
```

Interpretation:

- OpenRouter rejects the currently saved key when it is probed directly through OpenRouter's authenticated key endpoint.
- This confirms the specific user-visible `401 Unauthorized` / `User not found` failure can be reproduced outside Warp using OpenRouter's own key endpoint.

## In-App Reproduction

The app bundle was not present at `target/debug/bundle/osx/WarpOss.app`, so this task did not rerun the GUI flow. It uses the user-supplied reproduction:

```text
/agent test
Request failed with error: Other(provider stream error: Web stream error for model 'moonshotai/kimi-k2.6 (adapter: OpenAI)'.
Cause: HTTP error.
Status: 401 Unauthorized Unauthorized
Body: {"error":{"message":"User not found.","code":401}})
```

Notes:

- The `(adapter: OpenAI)` label is consistent with OpenAI-compatible routing and is confusing for OpenRouter users.
- Because the direct OpenRouter key probe fails with the same body, this instance of the error is consistent with a saved-key invalid/revoked failure. Route diagnostics are still needed to prove which credential the running app used.

## Log Evidence

Search across `~/Library/Logs/warp-oss.log*` for Direct API, OpenRouter, Rig, and the reported 401 strings produced no matching entries.

Interpretation:

- Current production-facing logs do not provide enough Direct API route evidence for this failure class.
- This supports the remediation plan to add redacted route diagnostics.

## Keychain And Signing Evidence

Expected app bundle path:

```text
target/debug/bundle/osx/WarpOss.app: No such file or directory
```

Keychain item lookup:

```text
keychain: "/Users/david/Library/Keychains/login.keychain-db"
class: "genp"
service: "dev.warp.WarpOss"
account: "User"
created: 2026-05-08T03:12:04Z
modified: 2026-05-16T23:16:25Z
```

Interpretation:

- A generic-password item exists for the OSS service namespace `dev.warp.WarpOss`.
- The current worktree does not have the expected app bundle available for code-signature inspection.
- Keychain prompt diagnosis remains partially open until the exact prompting bundle is available and `codesign --display --requirements :-` can be run against it.

## Root Cause Candidates

| Candidate | Evidence Required | Result | Verdict |
|---|---|---|---|
| Saved OpenRouter key is invalid | `/api/v1/key` returns 401 | OpenRouter returned `401` with `{"message":"User not found.","code":401}` | Confirmed for the currently saved key |
| Warp sends OpenRouter key to wrong endpoint | mocked provider test shows endpoint mismatch | Not tested in Task 1 | Still worth testing as regression coverage |
| Warp drops Authorization header | mocked provider test shows missing header | Not tested in Task 1 | Still worth testing as regression coverage |
| Rig OpenRouter path differs from native path | Rig and native diagnostics differ for same config | Not tested in Task 1 | Still worth testing after diagnostics land |
| Profile UI selected a stale/manual model under wrong provider | profile selection provider does not match label | User screenshot shows OpenRouter model; no persisted profile file found | Not confirmed |
| Keychain prompt is caused by unstable code identity | `codesign` DR changes across builds or app is ad hoc signed | Expected app bundle missing; keychain item exists for `dev.warp.WarpOss` | Inconclusive |

## Confirmed Root Cause For The Current Saved Credential

The currently saved OpenRouter key in `~/.warp-oss/settings.toml` is rejected by OpenRouter's own authenticated key endpoint with the same `401` body reported by the app. This explains the reported Direct API failure if the running app used the same saved key.

Separate issues remain valid and should still be remediated:

- Direct API settings layout is visually crowded and overlaps at the reported width.
- Agent engine selected state is too subtle.
- Direct API route diagnostics are insufficient.
- The keychain prompt needs signing evidence from the exact `WarpOss.app` bundle that prompts.
