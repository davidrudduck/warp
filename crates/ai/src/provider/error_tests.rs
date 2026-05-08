use super::*;

#[test]
fn provider_error_display_auth() {
    let e = ProviderError::Auth("bad key".into());
    assert!(e.to_string().contains("authentication failed"));
}

#[test]
fn provider_error_display_http() {
    let e = ProviderError::Http {
        status: 429,
        body: "too many".into(),
    };
    assert!(e.to_string().contains("429"));
    assert!(e.to_string().contains("too many"));
}

#[test]
fn provider_error_display_remote() {
    let e = ProviderError::Remote {
        provider: "anthropic".into(),
        code: Some("overloaded_error".into()),
        message: "Service overloaded".into(),
    };
    assert!(e.to_string().contains("Service overloaded"));
}

#[test]
fn provider_error_display_rate_limited_with_retry() {
    let e = ProviderError::RateLimited {
        retry_after_secs: Some(30),
    };
    let s = e.to_string();
    assert!(s.contains("rate limited") || s.contains("retry"));
}

#[test]
fn provider_error_display_rate_limited_no_retry() {
    let e = ProviderError::RateLimited {
        retry_after_secs: None,
    };
    assert!(!e.to_string().is_empty());
}

#[test]
fn provider_error_context_length() {
    let e = ProviderError::ContextLengthExceeded;
    assert!(e.to_string().contains("context length"));
}

#[test]
fn provider_error_stream_parse() {
    let e = ProviderError::StreamParse("unexpected EOF".into());
    assert!(e.to_string().contains("unexpected EOF"));
}

#[test]
fn provider_error_cancelled() {
    let e = ProviderError::Cancelled;
    assert!(!e.to_string().is_empty());
}

#[test]
fn provider_error_unsupported_model() {
    let e = ProviderError::UnsupportedModel("gpt-99".into());
    assert!(e.to_string().contains("gpt-99"));
}
