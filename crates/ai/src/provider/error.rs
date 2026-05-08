#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("authentication failed: {0}")]
    Auth(String),
    #[error("HTTP {status}: {body}")]
    Http { status: u16, body: String },
    #[error("provider stream error: {message}")]
    Remote {
        provider: String,
        code: Option<String>,
        message: String,
    },
    #[error("rate limited; retry after {retry_after_secs:?}s")]
    RateLimited { retry_after_secs: Option<u64> },
    #[error("service unavailable")]
    ServiceUnavailable,
    #[error("context length exceeded")]
    ContextLengthExceeded,
    #[error("transport error: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("stream parse error: {0}")]
    StreamParse(String),
    #[error("cancelled")]
    Cancelled,
    #[error("model not supported: {0}")]
    UnsupportedModel(String),
}

#[cfg(test)]
#[path = "error_tests.rs"]
mod tests;
