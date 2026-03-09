use std::sync::Arc;

use llm_kit_provider::LanguageModel;
use tracing::{debug, info};

use crate::config::LlmConfig;
use crate::error::LlmError;

/// Build a language model from LLM config.
///
/// API key resolution: config (settings.json) → env var → error.
pub fn build_model(config: &LlmConfig) -> Result<Arc<dyn LanguageModel>, LlmError> {
    match config.provider.as_str() {
        "anthropic" => build_anthropic(config),
        "openai" | "openai-compatible" => build_openai_compatible(config),
        other => Err(LlmError::UnknownProvider(other.to_string())),
    }
}

fn resolve_api_key(env_var: &'static str, config_key: Option<&str>) -> Result<String, LlmError> {
    // 1. Key from settings
    if let Some(key) = config_key
        && !key.is_empty()
    {
        let masked = mask_key(key);
        debug!(source = "settings", key = %masked, "API key resolved");
        return Ok(key.to_string());
    }

    // 2. Environment variable
    if let Ok(key) = std::env::var(env_var)
        && !key.is_empty()
    {
        let masked = mask_key(&key);
        debug!(source = "env", env_var, key = %masked, "API key resolved");
        return Ok(key);
    }

    debug!(env_var, "No API key found in settings or environment");
    Err(LlmError::NoApiKey { env_var })
}

fn build_anthropic(config: &LlmConfig) -> Result<Arc<dyn LanguageModel>, LlmError> {
    let api_key = resolve_api_key("ANTHROPIC_API_KEY", config.anthropic_api_key.as_deref())?;

    info!(provider = "anthropic", model = %config.model, "Building model");

    let provider = llm_kit_anthropic::AnthropicClient::new()
        .api_key(api_key)
        .build();

    Ok(Arc::new(provider.language_model(config.model.clone())))
}

fn build_openai_compatible(config: &LlmConfig) -> Result<Arc<dyn LanguageModel>, LlmError> {
    let api_key = resolve_api_key("OPENAI_API_KEY", config.openai_api_key.as_deref())?;

    let base_url = config
        .base_url
        .as_deref()
        .unwrap_or("https://api.openai.com/v1");

    info!(
        provider = %config.provider,
        model = %config.model,
        base_url,
        "Building model"
    );

    let provider = llm_kit_openai_compatible::OpenAICompatibleClient::new()
        .base_url(base_url)
        .api_key(api_key)
        .build();

    Ok(provider.chat_model(&config.model))
}

/// Mask an API key for safe logging: "sk-proj-abc...xyz"
fn mask_key(key: &str) -> String {
    if key.len() <= 8 {
        return "***".to_string();
    }
    let prefix = &key[..4.min(key.len())];
    let suffix = &key[key.len().saturating_sub(3)..];
    format!("{prefix}...{suffix}")
}
