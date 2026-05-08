use super::*;

#[test]
fn known_claude_model_returns_correct_context_window() {
    let caps = ModelRegistry::capabilities_for("claude-3-5-sonnet-20241022");
    assert_eq!(caps.context_window, 200_000);
    assert!(caps.supports_tools);
    assert!(caps.supports_vision);
    assert!(caps.supports_streaming);
}

#[test]
fn known_openai_model_returns_correct_capabilities() {
    let caps = ModelRegistry::capabilities_for("gpt-4o");
    assert_eq!(caps.context_window, 128_000);
    assert!(caps.supports_tools);
    assert!(caps.supports_vision);
    assert!(caps.supports_streaming);
}

#[test]
fn known_gemini_model_returns_correct_context_window() {
    let caps = ModelRegistry::capabilities_for("gemini-1.5-pro");
    assert_eq!(caps.context_window, 2_000_000);
    assert!(caps.supports_tools);
    assert!(caps.supports_vision);
    assert!(caps.supports_streaming);
}

#[test]
fn unknown_model_returns_default_capabilities() {
    let caps = ModelRegistry::capabilities_for("totally-unknown-model-xyz");
    // Default has context_window = 128_000 per ModelCapabilities::default()
    assert_eq!(caps.context_window, 128_000);
    assert!(caps.supports_tools);
    assert!(!caps.supports_vision);
    assert!(caps.supports_streaming);
}

#[test]
fn is_known_returns_true_for_known_model() {
    assert!(ModelRegistry::is_known("claude-3-opus-20240229"));
    assert!(ModelRegistry::is_known("gpt-4o-mini"));
    assert!(ModelRegistry::is_known("gemini-2.0-flash"));
}

#[test]
fn is_known_returns_false_for_unknown_model() {
    assert!(!ModelRegistry::is_known("not-a-real-model"));
    assert!(!ModelRegistry::is_known(""));
}

#[test]
fn o1_model_does_not_support_tools() {
    let caps = ModelRegistry::capabilities_for("o1");
    assert!(!caps.supports_tools);
    assert!(!caps.supports_streaming);
}

#[test]
fn gpt35_turbo_does_not_support_vision() {
    let caps = ModelRegistry::capabilities_for("gpt-3.5-turbo");
    assert!(!caps.supports_vision);
    assert_eq!(caps.context_window, 16_385);
}
