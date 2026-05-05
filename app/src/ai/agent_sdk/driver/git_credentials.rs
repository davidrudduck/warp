/// Git credentials management for cloud agent sandboxes.
///
/// This module handles:
/// - Writing `~/.git-credentials` and `~/.config/gh/hosts.yaml` so that `git`
///   and the `gh` CLI can authenticate to GitHub without requiring environment
///   variables.
/// - One-time git configuration (`credential.helper store`, SSH→HTTPS URL
///   rewrites).
/// - Configuring the git user identity from the server-returned username/email.
/// - An async refresh loop that periodically fetches a fresh token from the
///   server and overwrites the credential files, keeping long-running agents
///   authenticated for their entire duration.
use std::{path::PathBuf, sync::Arc, time::Duration};

use anyhow::{Context, Result};

use crate::server::server_api::ai::{AIClient, GitCredential};

// Use the project's allowed Command wrapper (not std::process::Command, which is
// disallowed by clippy rules because it flashes a terminal window on Windows).
use command::blocking::Command as BlockingCommand;

/// How long to wait between credential refresh attempts (~50 minutes, staying
/// well ahead of the one-hour GitHub token expiry).
pub(crate) const GIT_CREDENTIALS_REFRESH_INTERVAL: Duration = Duration::from_secs(50 * 60);

/// Fallback git user name when the server returns no username.
const DEFAULT_GIT_NAME: &str = "Oz";

/// Fallback git user email when the server returns no email.
const DEFAULT_GIT_EMAIL: &str = "oz-agent@warp.dev";

/// Returns the home directory path, or an error if it cannot be determined.
fn home_dir() -> Result<PathBuf> {
    dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))
}

/// Write `~/.git-credentials` with the given credentials.
///
/// Each credential entry is formatted as:
/// - `https://{username}:{token}@{host}` when a username is present
/// - `https://x-access-token:{token}@{host}` for service-account tokens
///
/// The write is done atomically: a temporary file is written then renamed.
fn write_git_credentials_file(credentials: &[GitCredential]) -> Result<()> {
    if credentials.is_empty() {
        return Ok(());
    }

    let home = home_dir()?;
    let path = home.join(".git-credentials");
    let tmp_path = home.join(".git-credentials.tmp");

    let mut content = String::new();
    for cred in credentials {
        let userinfo = match &cred.username {
            Some(username) => format!("{username}:{}", cred.token),
            None => format!("x-access-token:{}", cred.token),
        };
        content.push_str(&format!("https://{}@{}\n", userinfo, cred.host));
    }

    std::fs::write(&tmp_path, &content)
        .with_context(|| format!("Failed to write {}", tmp_path.display()))?;
    std::fs::rename(&tmp_path, &path).with_context(|| {
        format!(
            "Failed to rename {} to {}",
            tmp_path.display(),
            path.display()
        )
    })?;

    Ok(())
}

/// Write `~/.config/gh/hosts.yaml` so the `gh` CLI is authenticated.
///
/// The YAML format is stable for `gh` v2+:
/// ```yaml
/// github.com:
///     oauth_token: TOKEN
///     git_protocol: https
///     user: USERNAME
/// ```
///
/// The write is atomic: a temporary file is written then renamed.
fn write_gh_hosts_yaml(credentials: &[GitCredential]) -> Result<()> {
    if credentials.is_empty() {
        return Ok(());
    }

    let home = home_dir()?;
    let gh_config_dir = home.join(".config").join("gh");
    std::fs::create_dir_all(&gh_config_dir)
        .with_context(|| format!("Failed to create {}", gh_config_dir.display()))?;

    let path = gh_config_dir.join("hosts.yaml");
    let tmp_path = gh_config_dir.join("hosts.yaml.tmp");

    let mut yaml = String::new();
    for cred in credentials {
        yaml.push_str(&format!("{}:\n", cred.host));
        yaml.push_str(&format!("    oauth_token: {}\n", cred.token));
        yaml.push_str("    git_protocol: https\n");
        if let Some(username) = &cred.username {
            yaml.push_str(&format!("    user: {username}\n"));
        }
    }

    std::fs::write(&tmp_path, &yaml)
        .with_context(|| format!("Failed to write {}", tmp_path.display()))?;
    std::fs::rename(&tmp_path, &path).with_context(|| {
        format!(
            "Failed to rename {} to {}",
            tmp_path.display(),
            path.display()
        )
    })?;

    Ok(())
}

/// Write credential files for both `git` (`~/.git-credentials`) and the `gh`
/// CLI (`~/.config/gh/hosts.yaml`).
pub(crate) fn write_git_credentials(credentials: &[GitCredential]) -> Result<()> {
    write_git_credentials_file(credentials)?;
    write_gh_hosts_yaml(credentials)?;
    Ok(())
}

/// Run a git config command, logging a warning on failure rather than
/// propagating the error (git may not be installed in all sandboxes).
fn run_git_config(key: &str, value: &str) {
    match BlockingCommand::new("git")
        .args(["config", "--global", key, value])
        .output()
    {
        Ok(output) if output.status.success() => {}
        Ok(output) => {
            log::warn!(
                "git config --global {key} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Err(e) => {
            log::warn!("Failed to run git config --global {key}: {e}");
        }
    }
}

/// Run one-time git configuration that is set at startup and never needs to
/// be refreshed:
/// - `credential.helper store` so git reads `~/.git-credentials`
/// - SSH→HTTPS URL rewrites so `git clone git@github.com:...` works
pub(crate) fn setup_git_config() {
    run_git_config("credential.helper", "store");
    // Rewrite both ssh:// and scp-style git@ URLs to HTTPS.
    run_git_config("url.https://github.com/.insteadOf", "ssh://git@github.com/");
    run_git_config("url.https://github.com/.insteadOf", "git@github.com:");
}

/// Configure the git user identity from the server-returned credential.
///
/// Uses the first credential's `username`/`email` fields, falling back to the
/// Oz defaults when either is absent (e.g. service-account principals).
pub(crate) fn configure_git_identity(credentials: &[GitCredential]) {
    let (name, email) = credentials
        .first()
        .map(|c| {
            (
                c.username.as_deref().unwrap_or(DEFAULT_GIT_NAME),
                c.email.as_deref().unwrap_or(DEFAULT_GIT_EMAIL),
            )
        })
        .unwrap_or((DEFAULT_GIT_NAME, DEFAULT_GIT_EMAIL));

    run_git_config("user.name", name);
    run_git_config("user.email", email);
}

/// Infinite async loop that refreshes git credentials every
/// [`GIT_CREDENTIALS_REFRESH_INTERVAL`].
///
/// On each iteration:
/// 1. Issue a short-lived workload token.
/// 2. Call `taskGitCredentials` to get a fresh token from the server.
/// 3. Overwrite `~/.git-credentials` and `~/.config/gh/hosts.yaml`.
///
/// If any step fails, a warning is logged and the loop continues with the
/// next interval. The existing credential files remain valid until the token
/// actually expires (~10 minutes of buffer remain when we retry).
///
/// This future never resolves — it is designed to be raced with the harness
/// execution future via `futures::select!` and dropped when the harness
/// completes.
pub(crate) async fn refresh_loop(task_id: String, ai_client: Arc<dyn AIClient>) {
    loop {
        warpui::r#async::Timer::after(GIT_CREDENTIALS_REFRESH_INTERVAL).await;

        log::info!("Refreshing git credentials for task {task_id}");

        // Issue a fresh workload token for this refresh call.
        let workload_token =
            match warp_isolation_platform::issue_workload_token(Some(Duration::from_mins(5))).await
            {
                Ok(token) => token.token,
                Err(e) => {
                    log::warn!("Failed to issue workload token for git credentials refresh: {e}");
                    continue;
                }
            };

        let credentials = match ai_client
            .get_task_git_credentials(task_id.clone(), workload_token)
            .await
        {
            Ok(creds) => creds,
            Err(e) => {
                log::warn!("Failed to refresh git credentials: {e:#}");
                continue;
            }
        };

        if credentials.is_empty() {
            log::debug!("No git credentials returned during refresh; skipping file write");
            continue;
        }

        if let Err(e) = write_git_credentials(&credentials) {
            log::warn!("Failed to write refreshed git credentials: {e:#}");
        } else {
            log::info!("Git credentials refreshed successfully");
        }
    }
}
