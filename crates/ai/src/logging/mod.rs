use chrono::Utc;
use regex::Regex;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

fn redact_secrets(message: &str) -> String {
    let mut redacted = message.to_string();

    // Redact Anthropic API keys (sk-ant-...) - check this first as it's more specific
    let anthropic_pattern = Regex::new(r"sk-ant-[A-Za-z0-9_-]+").unwrap();
    redacted = anthropic_pattern.replace_all(&redacted, "sk-ant-***REDACTED***").to_string();

    // Redact OpenAI API keys (sk-...)
    let openai_pattern = Regex::new(r"sk-[A-Za-z0-9]+").unwrap();
    redacted = openai_pattern.replace_all(&redacted, "sk-***REDACTED***").to_string();

    // Redact Bearer tokens
    let bearer_pattern = Regex::new(r"Bearer\s+[A-Za-z0-9_\.\-]+").unwrap();
    redacted = bearer_pattern.replace_all(&redacted, "Bearer ***REDACTED***").to_string();

    // Redact any long base64-like strings (JWT tokens, etc.)
    let jwt_pattern = Regex::new(r"eyJ[A-Za-z0-9_\-]+\.[A-Za-z0-9_\-]+\.[A-Za-z0-9_\-]+").unwrap();
    redacted = jwt_pattern.replace_all(&redacted, "***REDACTED-JWT***").to_string();

    redacted
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

    pub fn log(&self, message: &str) {
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S%.3f");
        let redacted = redact_secrets(message);
        let log_line = format!("[{}] {}\n", timestamp, redacted);

        if let Ok(mut file) = self.log_file.lock() {
            let _ = file.write_all(log_line.as_bytes());
            let _ = file.flush();
        }
    }
}

#[cfg(test)]
#[path = "logger_tests.rs"]
mod tests;
