use super::friendly_error;

#[test]
fn parse_openai_quota_error() {
    let raw = r#"API request failed with status 429 Too Many Requests: {
  "error": {
    "message": "You exceeded your current quota, please check your plan and billing details. For more information on this error, read the docs: https://platform.openai.com/docs/guides/error-codes/api-errors.",
    "type": "insufficient_quota",
    "param": null,
    "code": "insufficient_quota"
  }
}"#;
    assert_eq!(
        friendly_error(raw),
        "Insufficient quota — check your plan and billing details."
    );
}

#[test]
fn parse_openai_invalid_key() {
    let raw = r#"API request failed with status 401 Unauthorized: {
  "error": {
    "message": "Incorrect API key provided: sk-1234****5678.",
    "type": "invalid_request_error",
    "param": null,
    "code": "invalid_api_key"
  }
}"#;
    assert_eq!(
        friendly_error(raw),
        "Invalid API key. Check your key and try again."
    );
}

#[test]
fn parse_rate_limit() {
    let raw = r#"API request failed with status 429: {
  "error": {
    "message": "Rate limit reached for gpt-4o",
    "code": "rate_limit_exceeded"
  }
}"#;
    assert_eq!(
        friendly_error(raw),
        "Rate limited. Wait a moment and try again."
    );
}

#[test]
fn parse_context_length() {
    let raw = r#"API request failed: {
  "error": {
    "message": "This model's maximum context length is 128000 tokens.",
    "code": "context_length_exceeded"
  }
}"#;
    assert_eq!(
        friendly_error(raw),
        "Conversation too long. Clear the chat and try again."
    );
}

#[test]
fn parse_unknown_code_with_message() {
    let raw = r#"API request failed: {
  "error": {
    "message": "Something weird happened",
    "code": "unknown_thingy"
  }
}"#;
    assert_eq!(friendly_error(raw), "Something weird happened.");
}

#[test]
fn fallback_401_without_json() {
    let raw = "request failed: 401 Unauthorized";
    assert_eq!(
        friendly_error(raw),
        "Invalid API key. Check your key and try again."
    );
}

#[test]
fn fallback_timeout() {
    let raw = "request timed out after 30s";
    assert_eq!(
        friendly_error(raw),
        "Request timed out. Check your connection and try again."
    );
}

#[test]
fn fallback_connection_error() {
    let raw = "connect error: Connection refused";
    assert_eq!(
        friendly_error(raw),
        "Could not connect to the provider. Check your internet connection."
    );
}

#[test]
fn long_raw_error_not_truncated() {
    let raw = "x".repeat(200);
    let result = friendly_error(&raw);
    assert_eq!(result.len(), 200);
}

#[test]
fn short_error_passed_through() {
    let raw = "Something broke";
    assert_eq!(friendly_error(raw), "Something broke");
}

#[test]
fn parse_anthropic_credit_balance_error() {
    let raw = "LLM stream error: Invalid prompt: Your credit balance is too low to access the Anthropic API. Please go to Plans & Billing to upgrade or purchase credits.";
    assert_eq!(
        friendly_error(raw),
        "Insufficient credits — check your billing at the provider dashboard."
    );
}

#[test]
fn parse_credit_balance_without_prefix() {
    let raw = "Your credit balance is too low to access the Anthropic API.";
    assert_eq!(
        friendly_error(raw),
        "Insufficient credits — check your billing at the provider dashboard."
    );
}

#[test]
fn strips_llm_stream_error_prefix() {
    let raw = "LLM stream error: Something broke";
    assert_eq!(friendly_error(raw), "Something broke");
}

#[test]
fn strips_openai_docs_link() {
    let raw = r#"API error: {
  "error": {
    "message": "You exceeded your current quota, please check your plan and billing details. For more information on this error, read the docs: https://platform.openai.com/docs/guides/error-codes/api-errors.",
    "code": "some_other_code"
  }
}"#;
    let result = friendly_error(raw);
    assert!(!result.contains("platform.openai.com"));
    assert!(!result.contains("read the docs"));
}
