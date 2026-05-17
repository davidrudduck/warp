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
async fn logger_redacts_hyphenated_openai_api_keys_fully() {
    let temp_dir = tempdir().unwrap();
    let log_dir = temp_dir.path().join("logs");

    let logger = DirectApiLogger::new(log_dir.clone());

    logger.log("OpenAI key: sk-proj-secret_suffix.part").await;

    let content = fs::read_to_string(log_dir.join("direct-api.log")).unwrap();
    assert!(!content.contains("sk-proj-secret_suffix.part"));
    assert!(!content.contains("secret_suffix"));
    assert!(content.contains("sk-***REDACTED***"));
}

#[tokio::test]
async fn logger_redacts_bearer_wrapped_openai_api_keys_fully() {
    let temp_dir = tempdir().unwrap();
    let log_dir = temp_dir.path().join("logs");

    let logger = DirectApiLogger::new(log_dir.clone());

    logger
        .log("Authorization: Bearer sk-proj-secret_suffix.part")
        .await;

    let content = fs::read_to_string(log_dir.join("direct-api.log")).unwrap();
    assert!(!content.contains("sk-proj-secret_suffix.part"));
    assert!(!content.contains("secret_suffix"));
    assert!(content.contains("Bearer ***REDACTED***"));
}

#[tokio::test]
async fn logger_redacts_openrouter_api_keys_fully() {
    let temp_dir = tempdir().unwrap();
    let log_dir = temp_dir.path().join("logs");

    let logger = DirectApiLogger::new(log_dir.clone());

    logger
        .log("OpenRouter key: sk-or-v1-secret-secret-secret")
        .await;

    let content = fs::read_to_string(log_dir.join("direct-api.log")).unwrap();
    assert!(!content.contains("sk-or-v1-secret-secret-secret"));
    assert!(!content.contains("-secret-secret"));
    assert!(content.contains("sk-or-v1-***REDACTED***"));
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

#[test]
fn rig_backend_diagnostics_redact_api_keys_and_tool_args() {
    let event = RigDiagnosticEvent {
        provider: "OpenRouter".to_string(),
        model_id: "sk-or-v1-secret\n{\"command\":\"cat ~/.ssh/id_rsa\"}".to_string(),
        ..Default::default()
    };

    let rendered = redact_rig_diagnostic_event(&event);

    assert!(!rendered.contains("sk-or-v1-secret"));
    assert!(!rendered.contains("id_rsa"));
    assert!(!rendered.contains("api_key"));
    assert!(!rendered.contains("tool_args"));
    assert!(!rendered.contains('\n'));
    assert!(rendered.contains("model_id_hash="));
}

#[test]
fn direct_api_route_diagnostics_do_not_render_secrets() {
    let rendered = redact_direct_api_route_diagnostic(
        "RigAgent",
        "OpenRouter",
        "https://openrouter.ai/api/v1",
        "moonshotai/kimi-k2.6",
        Some("sk-or-v1-secret-secret-secret"),
        Some(401),
        Some("User not found."),
    );

    assert!(rendered.contains("backend=RigAgent"));
    assert!(rendered.contains("provider=OpenRouter"));
    assert!(rendered.contains("base_url_host=openrouter.ai"));
    assert!(rendered.contains("status=401"));
    assert!(rendered.contains("api_key_present=true"));
    assert!(!rendered.contains("sk-or-v1"));
    assert!(!rendered.contains("kimi-k2.6"));
    assert!(!rendered.contains("User not found."));
}

#[test]
fn direct_api_status_extraction_handles_provider_error_text() {
    let message = "Web stream error.\nCause: HTTP error.\nStatus: 401 Unauthorized\nBody: secret";

    assert_eq!(http_status_from_diagnostic_message(message), Some(401));
}

#[test]
fn rig_backend_diagnostics_hash_custom_model_ids() {
    let event = RigDiagnosticEvent {
        provider: "CustomOpenAICompatible".to_string(),
        model_id: "private/internal-model".to_string(),
        event_count: 3,
        tool_call_count: 2,
        finish_reason: Some("ToolUse".to_string()),
        ..Default::default()
    };

    let rendered = redact_rig_diagnostic_event(&event);

    assert!(!rendered.contains("private/internal-model"));
    assert!(rendered.contains("model_id_hash="));
    assert!(rendered.contains("event_count=3"));
    assert!(rendered.contains("tool_call_count=2"));
    assert!(rendered.contains("finish_reason=ToolUse"));
}

#[test]
fn rig_backend_diagnostics_preserve_public_model_ids_and_error_category() {
    let event = RigDiagnosticEvent {
        provider: "OpenRouter".to_string(),
        model_id: "moonshotai/kimi-k2.6".to_string(),
        model_id_is_public: true,
        error_category: Some("remote".to_string()),
        ..Default::default()
    };

    let rendered = redact_rig_diagnostic_event(&event);

    assert!(rendered.contains("backend=rig_agent"));
    assert!(rendered.contains("provider=OpenRouter"));
    assert!(rendered.contains("model_id=moonshotai/kimi-k2.6"));
    assert!(rendered.contains("error_category=remote"));
}

#[test]
fn rig_backend_diagnostics_are_strict_allowlist() {
    let event = RigDiagnosticEvent {
        provider: "OpenRouter".to_string(),
        model_id: "moonshotai/kimi-k2.6".to_string(),
        model_id_is_public: true,
        event_count: 5,
        tool_call_count: 1,
        finish_reason: Some("Stop".to_string()),
        error_category: Some("none".to_string()),
        http_status: None,
    };

    let rendered = redact_rig_diagnostic_event(&event);
    let fields = rendered
        .split_whitespace()
        .map(|field| field.split_once('=').unwrap().0)
        .collect::<Vec<_>>();

    assert_eq!(
        fields,
        vec![
            "backend",
            "provider",
            "model_id",
            "event_count",
            "tool_call_count",
            "finish_reason",
            "error_category",
            "status",
        ]
    );
}

#[test]
fn rig_backend_diagnostics_hash_public_flagged_unsafe_model_ids() {
    let event = RigDiagnosticEvent {
        provider: "OpenRouter".to_string(),
        model_id: "moonshotai/kimi-k2.6\napi_key=secret".to_string(),
        model_id_is_public: true,
        ..Default::default()
    };

    let rendered = redact_rig_diagnostic_event(&event);

    assert!(!rendered.contains("api_key"));
    assert!(!rendered.contains("secret"));
    assert!(!rendered.contains('\n'));
    assert!(rendered.contains("model_id_hash="));
}
