use super::list_provider::*;
use super::ProviderId;

#[tokio::test]
async fn mock_provider_returns_success() {
    let models = vec![ModelDescriptor {
        id: "test-model".to_string(),
        display_name: Some("Test Model".to_string()),
        context_window: Some(4096),
        supports_tools: true,
    }];

    let mock = mock::MockModelListProvider::new_success(ProviderId::OpenAI, models.clone());

    assert_eq!(mock.provider_id(), ProviderId::OpenAI);

    let result = mock.list_models().await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), models);
}

#[tokio::test]
async fn mock_provider_returns_error() {
    let mock =
        mock::MockModelListProvider::new_error(ProviderId::Anthropic, ModelListError::AuthFailed);

    assert_eq!(mock.provider_id(), ProviderId::Anthropic);

    let result = mock.list_models().await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), ModelListError::AuthFailed));
}

#[tokio::test]
async fn mock_provider_allows_response_updates() {
    let mock = mock::MockModelListProvider::new_success(ProviderId::GoogleGemini, vec![]);

    // Initially returns empty success
    assert!(mock.list_models().await.unwrap().is_empty());

    // Update to return error
    mock.set_response(Err(ModelListError::Network("test error".to_string())));
    assert!(mock.list_models().await.is_err());

    // Update back to success with models
    let models = vec![ModelDescriptor {
        id: "updated".to_string(),
        display_name: None,
        context_window: None,
        supports_tools: false,
    }];
    mock.set_response(Ok(models.clone()));
    assert_eq!(mock.list_models().await.unwrap(), models);
}
