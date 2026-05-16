# Direct API Profile Routing

Direct API profile routing lets an execution profile use locally configured Direct API models instead of Warp Provider models.

Direct API keys and base URLs stay in the channel-specific settings file. For the `warp-oss` macOS build, that file is `~/.warp-oss/settings.toml`.

## Profile Setup

1. Open **Settings -> Agents -> Direct API**.
2. Configure and save at least one provider.
3. Open **Settings -> Agents -> Profiles**.
4. Edit a profile.
5. Set **Model Routing** to **Direct API**.
6. Choose the desired `Provider / Model`.

Warp Provider remains the default route for new and existing profiles. Direct API routing is opt-in per profile.

## Experimental Rig Backend

The Direct API settings page can expose an experimental Rig backend. It is off by default:

```toml
[agents.direct_api.experimental]
rig_backend_enabled = false
```

When enabled in a build compiled with Rig support, Direct API profiles show:

```text
Agent engine: Native / Rig Agent
```

Use `Native` unless testing Rig provider streaming. `Native` remains the default backend, and Warp still owns tool permissions, action execution, cancellation, and persistence when `Rig Agent` is selected.

If the cargo feature is not available, or the setting is off, profiles that store `Rig Agent` fall back to `Native` at runtime.
