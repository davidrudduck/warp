use chrono::Utc;
use once_cell::sync::Lazy;
use regex::Regex;
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

static OPENAI_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"sk-[A-Za-z0-9_\.\-]+").unwrap());

static ANTHROPIC_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"sk-ant-[A-Za-z0-9_-]+").unwrap());

static OPENROUTER_PATTERN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"sk-or-v1-[A-Za-z0-9_\.\-]+").unwrap());

static BEARER_PATTERN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"Bearer\s+[A-Za-z0-9_\.\-]+").unwrap());

static JWT_PATTERN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"eyJ[A-Za-z0-9_\-]+\.[A-Za-z0-9_\-]+\.[A-Za-z0-9_\-]+").unwrap());

const ANTHROPIC_REDACTION_PLACEHOLDER: &str = "__WARP_ANTHROPIC_KEY_REDACTED__";
const OPENROUTER_REDACTION_PLACEHOLDER: &str = "__WARP_OPENROUTER_KEY_REDACTED__";

fn redact_secrets(message: &str) -> String {
    let mut redacted = message.to_string();

    // Redact Anthropic API keys (sk-ant-...) - check this first as it's more specific
    redacted = ANTHROPIC_PATTERN
        .replace_all(&redacted, ANTHROPIC_REDACTION_PLACEHOLDER)
        .to_string();

    redacted = OPENROUTER_PATTERN
        .replace_all(&redacted, OPENROUTER_REDACTION_PLACEHOLDER)
        .to_string();

    // Redact OpenAI API keys (sk-...)
    redacted = OPENAI_PATTERN
        .replace_all(&redacted, "sk-***REDACTED***")
        .to_string();

    redacted = redacted
        .replace(ANTHROPIC_REDACTION_PLACEHOLDER, "sk-ant-***REDACTED***")
        .replace(OPENROUTER_REDACTION_PLACEHOLDER, "sk-or-v1-***REDACTED***");

    // Redact Bearer tokens
    redacted = BEARER_PATTERN
        .replace_all(&redacted, "Bearer ***REDACTED***")
        .to_string();

    // Redact any long base64-like strings (JWT tokens, etc.)
    redacted = JWT_PATTERN
        .replace_all(&redacted, "***REDACTED-JWT***")
        .to_string();

    redacted
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct RigDiagnosticEvent {
    pub(crate) provider: String,
    pub(crate) model_id: String,
    pub(crate) model_id_is_public: bool,
    pub(crate) event_count: usize,
    pub(crate) tool_call_count: usize,
    pub(crate) finish_reason: Option<String>,
    pub(crate) error_category: Option<String>,
    pub(crate) http_status: Option<u16>,
}

pub fn redact_rig_diagnostic_event(event: &RigDiagnosticEvent) -> String {
    let model_field = if event.model_id_is_public && is_safe_log_value(&event.model_id) {
        format!("model_id={}", event.model_id)
    } else {
        format!("model_id_hash={}", hash_custom_model_id(&event.model_id))
    };
    let finish_reason = event
        .finish_reason
        .as_deref()
        .filter(|value| is_safe_log_value(value))
        .unwrap_or("none");
    let error_category = event
        .error_category
        .as_deref()
        .filter(|value| is_safe_log_value(value))
        .unwrap_or("none");
    let http_status = event
        .http_status
        .map(|status| status.to_string())
        .unwrap_or_else(|| "none".to_string());

    format!(
        "backend=rig_agent provider={} {} event_count={} tool_call_count={} finish_reason={} error_category={} status={}",
        if is_safe_log_value(&event.provider) {
            event.provider.as_str()
        } else {
            "unknown"
        },
        model_field,
        event.event_count,
        event.tool_call_count,
        finish_reason,
        error_category,
        http_status,
    )
}

pub fn redact_direct_api_route_diagnostic(
    backend: &str,
    provider: &str,
    base_url: &str,
    model_id: &str,
    api_key: Option<&str>,
    status: Option<u16>,
    provider_message: Option<&str>,
) -> String {
    let base_url_host = reqwest::Url::parse(base_url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_string))
        .unwrap_or_else(|| "unknown".to_string());
    let status = status
        .map(|status| status.to_string())
        .unwrap_or_else(|| "none".to_string());
    let error_hash = provider_message
        .filter(|message| !message.trim().is_empty())
        .map(hash_custom_model_id)
        .unwrap_or_else(|| "none".to_string());

    format!(
        "direct_api_route backend={} provider={} base_url_host={} model_id_hash={} api_key_present={} status={} provider_error_hash={}",
        safe_log_token(backend),
        safe_log_token(provider),
        safe_log_token(&base_url_host),
        hash_custom_model_id(model_id),
        api_key.is_some_and(|key| !key.trim().is_empty()),
        safe_log_token(&status),
        error_hash,
    )
}

pub fn http_status_from_diagnostic_message(message: &str) -> Option<u16> {
    ["Status:", "status:", "HTTP", "http", "status code"]
        .iter()
        .find_map(|marker| status_after_marker(message, marker))
}

fn status_after_marker(message: &str, marker: &str) -> Option<u16> {
    let marker_index = message.find(marker)?;
    let tail = &message[marker_index + marker.len()..];
    let digits = tail
        .chars()
        .skip_while(|ch| !ch.is_ascii_digit())
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    if digits.len() == 3 {
        digits.parse().ok()
    } else {
        None
    }
}

fn is_safe_log_value(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b'/'))
}

fn safe_log_token(value: &str) -> &str {
    if is_safe_log_value(value) {
        value
    } else {
        "unknown"
    }
}

fn hash_custom_model_id(model_id: &str) -> String {
    let digest = Sha256::digest(model_id.as_bytes());
    digest
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub struct DirectApiLogger {
    log_file: Arc<Mutex<std::fs::File>>,
}

impl DirectApiLogger {
    pub fn new(log_dir: PathBuf) -> Self {
        // Create log directory
        fs::create_dir_all(&log_dir).expect("Failed to create log directory");

        // Open log file in append mode
        let log_path = log_dir.join("direct-api.log");
        let log_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .expect("Failed to open log file");

        Self {
            log_file: Arc::new(Mutex::new(log_file)),
        }
    }

    pub async fn log(&self, message: &str) {
        let log_line = format!(
            "[{}] {}\n",
            Utc::now().format("%Y-%m-%d %H:%M:%S%.3f"),
            redact_secrets(message)
        );

        let file = self.log_file.clone();
        tokio::task::spawn_blocking(move || {
            if let Ok(mut f) = file.lock() {
                let _ = f.write_all(log_line.as_bytes());
                let _ = f.flush();
            }
        })
        .await
        .ok();
    }
}

#[cfg(test)]
#[path = "logger_tests.rs"]
mod tests;
