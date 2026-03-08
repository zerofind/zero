use thiserror::Error;

#[derive(Debug, Error)]
pub enum LlmError {
    #[error("no API key: configure in settings or set {env_var}")]
    NoApiKey { env_var: &'static str },

    #[error("unknown provider: {0} (supported: anthropic, openai)")]
    UnknownProvider(String),

    #[error("model error: {0}")]
    Model(String),

    #[error("runtime error: {0}")]
    Runtime(String),
}

/// Parse a raw API/stream error into a short, human-readable message.
///
/// Provider SDKs return errors like:
///   "API request failed with status 429 Too Many Requests: { "error": { "message": "...", "code": "..." } }"
///
/// We extract the meaningful part so the UI shows something like
/// "Insufficient quota — check your plan and billing details." instead of a JSON dump.
pub fn friendly_error(raw: &str) -> String {
    // Strip wrapper prefixes added by our own code before classifying.
    let cleaned = raw.strip_prefix("LLM stream error: ").unwrap_or(raw);

    // Try to extract JSON body from the raw error string
    if let Some(json_start) = cleaned.find('{') {
        let json_str = &cleaned[json_start..];
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(json_str) {
            // OpenAI / Anthropic format: { "error": { "message": "...", "code": "..." } }
            if let Some(msg) = json["error"]["message"].as_str() {
                let code = json["error"]["code"].as_str().unwrap_or("");
                return classify_error(msg, code);
            }
            // Flat format: { "message": "...", "code": "..." }
            if let Some(msg) = json["message"].as_str() {
                let code = json["code"].as_str().unwrap_or("");
                return classify_error(msg, code);
            }
        }
    }

    // Pattern-match billing / credit balance errors (Anthropic sends these as plain text)
    if cleaned.contains("credit balance")
        || cleaned.contains("balance is too low")
        || cleaned.contains("billing")
            && (cleaned.contains("upgrade") || cleaned.contains("purchase"))
    {
        return "Insufficient credits — check your billing at the provider dashboard.".to_string();
    }

    // Try to detect HTTP status codes in the raw string
    if cleaned.contains("401") || cleaned.contains("Unauthorized") {
        return "Invalid API key. Check your key and try again.".to_string();
    }
    if cleaned.contains("429") || cleaned.contains("Too Many Requests") {
        return "Rate limited. Wait a moment and try again.".to_string();
    }
    if cleaned.contains("503") || cleaned.contains("Service Unavailable") {
        return "Provider is temporarily unavailable. Try again shortly.".to_string();
    }
    if cleaned.contains("timeout") || cleaned.contains("timed out") {
        return "Request timed out. Check your connection and try again.".to_string();
    }
    if cleaned.contains("connect") && cleaned.contains("error") {
        return "Could not connect to the provider. Check your internet connection.".to_string();
    }

    // Fallback: show the cleaned message (without our wrapper prefix)
    cleaned.to_string()
}

/// Map known error codes/messages to concise user-facing text.
fn classify_error(message: &str, code: &str) -> String {
    match code {
        "insufficient_quota" => {
            "Insufficient quota — check your plan and billing details.".to_string()
        }
        "invalid_api_key" => "Invalid API key. Check your key and try again.".to_string(),
        "model_not_found" => {
            format!("Model not available. {message}")
        }
        "rate_limit_exceeded" => "Rate limited. Wait a moment and try again.".to_string(),
        "server_error" | "service_unavailable" => {
            "Provider is temporarily unavailable. Try again shortly.".to_string()
        }
        "context_length_exceeded" => {
            "Conversation too long. Clear the chat and try again.".to_string()
        }
        "overloaded" => "Provider is overloaded. Try again in a few seconds.".to_string(),
        _ => {
            // No known code — use the message, but clean it up
            let clean = message
                .split("For more information on this error")
                .next()
                .unwrap_or(message)
                .trim()
                .trim_end_matches('.')
                .to_string();

            if clean.is_empty() {
                "An unexpected error occurred.".to_string()
            } else {
                format!("{clean}.")
            }
        }
    }
}

#[cfg(test)]
#[path = "error_test.rs"]
mod error_test;
