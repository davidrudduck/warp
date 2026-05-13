use super::*;
use std::fs;
use tempfile::tempdir;

#[test]
fn logger_creates_log_directory() {
    let temp_dir = tempdir().unwrap();
    let log_dir = temp_dir.path().join("logs");

    let _logger = DirectApiLogger::new(log_dir.clone());

    assert!(log_dir.exists());
    assert!(log_dir.join("direct-api.log").exists());
}

#[tokio::test]
async fn logger_writes_to_regular_log() {
    let temp_dir = tempdir().unwrap();
    let log_dir = temp_dir.path().join("logs");

    let logger = DirectApiLogger::new(log_dir.clone());
    logger.log("Test message").await;

    let content = fs::read_to_string(log_dir.join("direct-api.log")).unwrap();
    assert!(content.contains("Test message"));
}

#[tokio::test]
async fn logger_redacts_api_keys() {
    let temp_dir = tempdir().unwrap();
    let log_dir = temp_dir.path().join("logs");

    let logger = DirectApiLogger::new(log_dir.clone());

    // Log message with API key
    logger
        .log("Request with key: sk-1234567890abcdefghijklmnop")
        .await;

    let content = fs::read_to_string(log_dir.join("direct-api.log")).unwrap();
    assert!(!content.contains("sk-1234567890abcdefghijklmnop"));
    assert!(content.contains("sk-***REDACTED***"));
}

#[tokio::test]
async fn logger_redacts_bearer_tokens() {
    let temp_dir = tempdir().unwrap();
    let log_dir = temp_dir.path().join("logs");

    let logger = DirectApiLogger::new(log_dir.clone());

    // Log message with bearer token
    logger
        .log("Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9")
        .await;

    let content = fs::read_to_string(log_dir.join("direct-api.log")).unwrap();
    assert!(!content.contains("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9"));
    assert!(content.contains("Bearer ***REDACTED***"));
}

#[tokio::test]
async fn logger_redacts_multiple_secrets_in_one_line() {
    let temp_dir = tempdir().unwrap();
    let log_dir = temp_dir.path().join("logs");

    let logger = DirectApiLogger::new(log_dir.clone());

    logger.log("Key: sk-abc123 and token: Bearer xyz789").await;

    let content = fs::read_to_string(log_dir.join("direct-api.log")).unwrap();
    assert!(content.contains("sk-***REDACTED***"));
    assert!(content.contains("Bearer ***REDACTED***"));
    assert!(!content.contains("abc123"));
    assert!(!content.contains("xyz789"));
}
