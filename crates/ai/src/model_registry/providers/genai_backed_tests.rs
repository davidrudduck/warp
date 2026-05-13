//! Tests for `GenaiBackedListProvider`.

use super::genai_backed::GenaiBackedListProvider;
use crate::model_registry::{ModelListError, ModelListProvider, ProviderId};

#[tokio::test]
async fn list_models_succeeds_with_enriched_descriptors() {
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .ok();
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/models")
        .match_header("authorization", "Bearer test-key")
        .with_status(200)
        .with_body(
            r#"{
                "data": [
                    { "id": "gpt-4o" },
                    { "id": "gpt-4o-mini" },
                    { "id": "unknown-future-model" }
                ]
            }"#,
        )
        .create_async()
        .await;

    let provider = GenaiBackedListProvider::new(
        ProviderId::OpenAI,
        Some("test-key".to_string()),
        Some(server.url()),
    )
    .expect("OpenAI is supported");

    let result = provider.list_models().await;
    assert!(result.is_ok(), "expected Ok, got {result:?}");

    let models = result.expect("should succeed");
    assert_eq!(models.len(), 3);

    // Known model: enriched with capabilities from known_capabilities table.
    assert_eq!(models[0].id, "gpt-4o");
    assert_eq!(models[0].context_window, Some(128_000));
    assert!(models[0].supports_tools);

    assert_eq!(models[1].id, "gpt-4o-mini");
    assert_eq!(models[1].context_window, Some(128_000));
    assert!(models[1].supports_tools);

    // Unknown model: minimal descriptor with supports_tools=true assumption.
    assert_eq!(models[2].id, "unknown-future-model");
    assert_eq!(models[2].context_window, None);
    assert!(models[2].supports_tools);

    assert_eq!(provider.provider_id(), ProviderId::OpenAI);
    mock.assert_async().await;
}

#[tokio::test]
async fn list_models_returns_auth_failed_on_401() {
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .ok();
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/models")
        .with_status(401)
        .create_async()
        .await;

    let provider = GenaiBackedListProvider::new(
        ProviderId::OpenAI,
        Some("bad-key".to_string()),
        Some(server.url()),
    )
    .expect("OpenAI is supported");

    let result = provider.list_models().await;
    match result {
        Err(ModelListError::AuthFailed) => {}
        other => panic!("expected AuthFailed, got {other:?}"),
    }

    mock.assert_async().await;
}

#[tokio::test]
async fn list_models_returns_offline_on_network_error() {
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .ok();
    // Pointing at a closed local port deterministically triggers a connection
    // error, which the provider maps to `ModelListError::Offline`.
    let provider = GenaiBackedListProvider::new(
        ProviderId::OpenAI,
        Some("test-key".to_string()),
        Some("http://127.0.0.1:1".to_string()),
    )
    .expect("OpenAI is supported");

    let result = provider.list_models().await;
    match result {
        Err(ModelListError::Offline) => {}
        other => panic!("expected Offline, got {other:?}"),
    }
}

#[tokio::test]
async fn list_models_returns_fallback_when_empty() {
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .ok();
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/models")
        .with_status(200)
        .with_body(r#"{ "data": [] }"#)
        .create_async()
        .await;

    let provider = GenaiBackedListProvider::new(
        ProviderId::Anthropic,
        Some("test-key".to_string()),
        Some(server.url()),
    )
    .expect("Anthropic is supported");

    let result = provider.list_models().await;
    let models = result.expect("empty response should yield fallback models");

    // Fallback should not be empty.
    assert!(!models.is_empty(), "expected fallback models for Anthropic");
    // First fallback entry should be claude-3-5-sonnet-latest.
    assert_eq!(models[0].id, "claude-3-5-sonnet-latest");
    assert!(models[0].supports_tools);

    mock.assert_async().await;
}

#[tokio::test]
async fn new_rejects_unsupported_provider() {
    let result = GenaiBackedListProvider::new(ProviderId::OpenRouter, None, None);
    assert!(
        matches!(result, Err(ModelListError::Unsupported)),
        "expected Unsupported for OpenRouter"
    );

    let result = GenaiBackedListProvider::new(ProviderId::Custom, None, None);
    assert!(
        matches!(result, Err(ModelListError::Unsupported)),
        "expected Unsupported for Custom"
    );
}

#[tokio::test]
async fn list_models_parses_ollama_response() {
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .ok();
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/api/tags")
        .with_status(200)
        .with_body(
            r#"{
                "models": [
                    { "name": "llama3:latest" },
                    { "name": "mistral:latest" }
                ]
            }"#,
        )
        .create_async()
        .await;

    let provider = GenaiBackedListProvider::new(ProviderId::Ollama, None, Some(server.url()))
        .expect("Ollama is supported");

    let result = provider.list_models().await;
    let models = result.expect("should succeed");

    assert_eq!(models.len(), 2);
    assert_eq!(models[0].id, "llama3:latest");
    assert_eq!(models[1].id, "mistral:latest");

    mock.assert_async().await;
}
