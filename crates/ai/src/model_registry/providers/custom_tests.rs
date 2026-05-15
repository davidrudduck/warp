//! Tests for Custom provider implementation.

use super::custom::CustomListProvider;
use crate::model_registry::{ModelListError, ModelListProvider, ProviderId};

/// Install the default rustls crypto provider for tests.
/// This is required because the workspace uses `rustls-tls-native-roots-no-provider`.
fn install_crypto_provider() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}

#[tokio::test]
async fn custom_list_models_success() {
    install_crypto_provider();
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/v1/models")
        .match_header("authorization", "Bearer test-key")
        .with_status(200)
        .with_body(
            r#"{
                "data": [
                    { "id": "custom-model-1" },
                    { "id": "custom-model-2" }
                ]
            }"#,
        )
        .create_async()
        .await;

    let provider =
        CustomListProvider::new(server.url(), Some("test-key".to_string())).expect("valid URL");

    let result = provider.list_models().await;
    assert!(result.is_ok());

    let models = result.expect("should succeed");
    assert_eq!(models.len(), 2);

    assert_eq!(models[0].id, "custom-model-1");
    assert_eq!(models[0].display_name, None);
    assert_eq!(models[0].context_window, None);
    assert!(!models[0].supports_tools);

    assert_eq!(models[1].id, "custom-model-2");
    assert_eq!(models[1].display_name, None);
    assert_eq!(models[1].context_window, None);
    assert!(!models[1].supports_tools);

    assert_eq!(provider.provider_id(), ProviderId::Custom);
    mock.assert_async().await;
}

#[tokio::test]
async fn custom_list_models_does_not_double_append_v1() {
    install_crypto_provider();
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/v1/models")
        .with_status(200)
        .with_body(r#"{"data":[{"id":"custom-model"}]}"#)
        .create_async()
        .await;

    let provider =
        CustomListProvider::new(format!("{}/v1", server.url()), None).expect("valid URL");

    let result = provider.list_models().await;
    assert!(result.is_ok());
    assert_eq!(result.expect("should succeed")[0].id, "custom-model");

    mock.assert_async().await;
}

#[test]
fn custom_provider_rejects_public_plaintext_http() {
    let provider =
        CustomListProvider::new("http://8.8.8.8".to_string(), Some("test-key".to_string()));

    assert!(provider.is_err());
}

#[tokio::test]
async fn custom_list_models_without_auth() {
    install_crypto_provider();
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/v1/models")
        .with_status(200)
        .with_body(
            r#"{
                "data": [
                    { "id": "local-model" }
                ]
            }"#,
        )
        .create_async()
        .await;

    // Create provider without API key
    let provider = CustomListProvider::new(server.url(), None).expect("valid URL");

    let result = provider.list_models().await;
    assert!(result.is_ok());

    let models = result.expect("should succeed");
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].id, "local-model");

    mock.assert_async().await;
}

#[tokio::test]
async fn custom_auth_failed_on_403() {
    install_crypto_provider();
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/v1/models")
        .with_status(403)
        .create_async()
        .await;

    let provider =
        CustomListProvider::new(server.url(), Some("bad-key".to_string())).expect("valid URL");

    let result = provider.list_models().await;
    assert!(result.is_err());

    match result.unwrap_err() {
        ModelListError::AuthFailed => {}
        other => panic!("expected AuthFailed, got {other:?}"),
    }

    mock.assert_async().await;
}

#[tokio::test]
async fn custom_unsupported_on_parse_failure() {
    install_crypto_provider();
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/v1/models")
        .with_status(200)
        .with_body(r#"{"models": ["not-openai-format"]}"#)
        .create_async()
        .await;

    let provider =
        CustomListProvider::new(server.url(), Some("test-key".to_string())).expect("valid URL");

    let result = provider.list_models().await;
    assert!(result.is_err());

    match result.unwrap_err() {
        ModelListError::Unsupported => {}
        other => panic!("expected Unsupported, got {other:?}"),
    }

    mock.assert_async().await;
}

#[tokio::test]
async fn custom_offline_on_connection_error() {
    install_crypto_provider();
    // Use an unreachable URL to trigger connection error
    let provider = CustomListProvider::new(
        "http://localhost:1".to_string(),
        Some("test-key".to_string()),
    )
    .expect("valid URL");

    let result = provider.list_models().await;
    assert!(result.is_err());

    match result.unwrap_err() {
        ModelListError::Offline => {}
        other => panic!("expected Offline, got {other:?}"),
    }
}

#[tokio::test]
async fn custom_rate_limited_on_429() {
    install_crypto_provider();
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/v1/models")
        .with_status(429)
        .with_header("retry-after", "30")
        .create_async()
        .await;

    let provider =
        CustomListProvider::new(server.url(), Some("test-key".to_string())).expect("valid URL");

    let result = provider.list_models().await;
    assert!(result.is_err());

    match result.unwrap_err() {
        ModelListError::RateLimited { retry_after_secs } => {
            assert_eq!(retry_after_secs, Some(30));
        }
        other => panic!("expected RateLimited, got {other:?}"),
    }

    mock.assert_async().await;
}
