use std::net::IpAddr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BaseUrlValidationError {
    InvalidUrl,
    HttpNotLocalOrPrivate,
}

pub fn validate_direct_api_base_url(url: &str) -> Result<(), BaseUrlValidationError> {
    let parsed = reqwest::Url::parse(url.trim()).map_err(|_| BaseUrlValidationError::InvalidUrl)?;
    if parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(BaseUrlValidationError::InvalidUrl);
    }
    match parsed.scheme() {
        "https" => Ok(()),
        "http" if host_is_local_or_private(&parsed) => Ok(()),
        "http" => Err(BaseUrlValidationError::HttpNotLocalOrPrivate),
        _ => Err(BaseUrlValidationError::InvalidUrl),
    }
}

pub fn normalize_direct_api_base_url(url: &str) -> Result<String, BaseUrlValidationError> {
    validate_direct_api_base_url(url)?;
    Ok(url.trim().trim_end_matches('/').to_string())
}

pub fn normalize_openai_compatible_base_url(url: &str) -> Result<String, BaseUrlValidationError> {
    let normalized = normalize_direct_api_base_url(url)?;
    let trimmed = normalized.as_str();
    Ok(trimmed.strip_suffix("/v1").unwrap_or(trimmed).to_string())
}

pub fn openai_compatible_base_url_with_v1(url: &str) -> String {
    let trimmed = url.trim().trim_end_matches('/');
    if trimmed.ends_with("/v1") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/v1")
    }
}

fn host_is_local_or_private(url: &reqwest::Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    let Ok(addr) = host.parse::<IpAddr>() else {
        return false;
    };
    match addr {
        IpAddr::V4(addr) => {
            addr.is_loopback()
                || addr.is_private()
                || addr.octets()[0] == 169 && addr.octets()[1] == 254
        }
        IpAddr::V6(addr) => addr.is_loopback() || addr.is_unique_local(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_https_urls() {
        assert_eq!(
            validate_direct_api_base_url("https://api.openai.com/v1"),
            Ok(())
        );
    }

    #[test]
    fn allows_http_loopback_and_private_lan() {
        assert_eq!(
            validate_direct_api_base_url("http://localhost:11434"),
            Ok(())
        );
        assert_eq!(
            validate_direct_api_base_url("http://127.0.0.1:11434"),
            Ok(())
        );
        assert_eq!(
            validate_direct_api_base_url("http://192.168.1.10:8080"),
            Ok(())
        );
        assert_eq!(validate_direct_api_base_url("http://10.0.0.5:8080"), Ok(()));
        assert_eq!(
            validate_direct_api_base_url("http://172.16.0.5:8080"),
            Ok(())
        );
    }

    #[test]
    fn rejects_prefix_spoof_hosts() {
        assert_eq!(
            validate_direct_api_base_url("http://localhost.evil.test:11434"),
            Err(BaseUrlValidationError::HttpNotLocalOrPrivate)
        );
        assert_eq!(
            validate_direct_api_base_url("http://127.0.0.1.evil.test:11434"),
            Err(BaseUrlValidationError::HttpNotLocalOrPrivate)
        );
    }

    #[test]
    fn rejects_public_http_urls() {
        assert_eq!(
            validate_direct_api_base_url("http://8.8.8.8:8080"),
            Err(BaseUrlValidationError::HttpNotLocalOrPrivate)
        );
    }

    #[test]
    fn rejects_invalid_schemes_and_text() {
        assert_eq!(
            validate_direct_api_base_url("ftp://localhost"),
            Err(BaseUrlValidationError::InvalidUrl)
        );
        assert_eq!(
            validate_direct_api_base_url("not-a-url"),
            Err(BaseUrlValidationError::InvalidUrl)
        );
    }

    #[test]
    fn rejects_query_fragment_and_userinfo() {
        assert_eq!(
            validate_direct_api_base_url("https://api.example.com/v1?tenant=x"),
            Err(BaseUrlValidationError::InvalidUrl)
        );
        assert_eq!(
            validate_direct_api_base_url("https://api.example.com/v1#models"),
            Err(BaseUrlValidationError::InvalidUrl)
        );
        assert_eq!(
            validate_direct_api_base_url("https://user:pass@api.example.com/v1"),
            Err(BaseUrlValidationError::InvalidUrl)
        );
    }

    #[test]
    fn normalizes_openai_compatible_base_url_once() {
        assert_eq!(
            normalize_openai_compatible_base_url("https://example.test/v1").unwrap(),
            "https://example.test"
        );
        assert_eq!(
            normalize_openai_compatible_base_url("https://example.test/").unwrap(),
            "https://example.test"
        );
    }

    #[test]
    fn formats_openai_compatible_api_base_url_with_v1_once() {
        assert_eq!(
            openai_compatible_base_url_with_v1("https://example.test"),
            "https://example.test/v1"
        );
        assert_eq!(
            openai_compatible_base_url_with_v1(" https://example.test/v1/ "),
            "https://example.test/v1"
        );
    }

    #[test]
    fn normalizes_by_trimming_whitespace_and_trailing_slashes() {
        assert_eq!(
            normalize_direct_api_base_url(" https://example.test/v1/ ").unwrap(),
            "https://example.test/v1"
        );
    }
}
