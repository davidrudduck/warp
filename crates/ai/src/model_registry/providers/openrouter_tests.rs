//! Tests for OpenRouter provider implementation.

use super::openrouter::OpenRouterListProvider;
use crate::model_registry::{ModelListError, ModelListProvider, ProviderId};

/// Install the default rustls crypto provider for tests.
/// This is required because the workspace uses `rustls-tls-native-roots-no-provider`.
fn install_crypto_provider() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}

#[tokio::test]
async fn openrouter_list_models_success() {
    install_crypto_provider();
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/models")
        .match_header("authorization", "Bearer test-key")
        .with_status(200)
        .with_body(
            r#"{
                "data": [
                    {
                        "id": "openai/gpt-4",
                        "name": "GPT-4",
                        "context_length": 8192
                    },
                    {
                        "id": "anthropic/claude-3-opus",
                        "name": "Claude 3 Opus"
                    }
                ]
            }"#,
        )
        .create_async()
        .await;

    let provider = OpenRouterListProvider::new("test-key".to_string(), Some(server.url()));

    let result = provider.list_models().await;
    assert!(result.is_ok());

    let models = result.expect("should succeed");
    assert_eq!(models.len(), 2);

    assert_eq!(models[0].id, "openai/gpt-4");
    assert_eq!(models[0].display_name, Some("GPT-4".to_string()));
    assert_eq!(models[0].context_window, Some(8192));
    assert!(models[0].supports_tools);

    assert_eq!(models[1].id, "anthropic/claude-3-opus");
    assert_eq!(models[1].display_name, Some("Claude 3 Opus".to_string()));
    assert_eq!(models[1].context_window, None);
    assert!(models[1].supports_tools);

    assert_eq!(provider.provider_id(), ProviderId::OpenRouter);
    mock.assert_async().await;
}

#[tokio::test]
async fn openrouter_auth_failed_on_401() {
    install_crypto_provider();
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/models")
        .with_status(401)
        .create_async()
        .await;

    let provider = OpenRouterListProvider::new("bad-key".to_string(), Some(server.url()));

    let result = provider.list_models().await;
    assert!(result.is_err());

    match result.unwrap_err() {
        ModelListError::AuthFailed => {}
        other => panic!("expected AuthFailed, got {other:?}"),
    }

    mock.assert_async().await;
}

#[tokio::test]
async fn openrouter_forbidden_preserves_http_error_context() {
    install_crypto_provider();
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/models")
        .with_status(403)
        .create_async()
        .await;

    let provider = OpenRouterListProvider::new("test-key".to_string(), Some(server.url()));

    let result = provider.list_models().await;
    assert!(result.is_err());

    match result.unwrap_err() {
        ModelListError::Network(message) if message.contains("HTTP 403") => {}
        other => panic!("expected HTTP 403 Network error, got {other:?}"),
    }

    mock.assert_async().await;
}

#[tokio::test]
async fn openrouter_rate_limited_on_429() {
    install_crypto_provider();
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/models")
        .with_status(429)
        .with_header("retry-after", "60")
        .create_async()
        .await;

    let provider = OpenRouterListProvider::new("test-key".to_string(), Some(server.url()));

    let result = provider.list_models().await;
    assert!(result.is_err());

    match result.unwrap_err() {
        ModelListError::RateLimited { retry_after_secs } => {
            assert_eq!(retry_after_secs, Some(60));
        }
        other => panic!("expected RateLimited, got {other:?}"),
    }

    mock.assert_async().await;
}

#[tokio::test]
async fn openrouter_parse_failed_on_invalid_json() {
    install_crypto_provider();
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/models")
        .with_status(200)
        .with_body("invalid json")
        .create_async()
        .await;

    let provider = OpenRouterListProvider::new("test-key".to_string(), Some(server.url()));

    let result = provider.list_models().await;
    assert!(result.is_err());

    match result.unwrap_err() {
        ModelListError::ParseFailed(_) => {}
        other => panic!("expected ParseFailed, got {other:?}"),
    }

    mock.assert_async().await;
}

#[tokio::test]
async fn openrouter_offline_on_connection_error() {
    install_crypto_provider();
    // Use an unreachable URL to trigger connection error
    let provider = OpenRouterListProvider::new(
        "test-key".to_string(),
        Some("http://localhost:1".to_string()),
    );

    let result = provider.list_models().await;
    assert!(result.is_err());

    match result.unwrap_err() {
        ModelListError::Offline => {}
        other => panic!("expected Offline, got {other:?}"),
    }
}
