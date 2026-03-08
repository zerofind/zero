use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LlmConfig {
    pub enabled: bool,
    pub provider: String,
    pub model: String,
    pub base_url: Option<String>,
    pub max_output_tokens: u32,
    pub api_key: Option<String>,
}

/// A model available for selection.
#[derive(Debug, Clone, Copy)]
pub struct ModelInfo {
    pub id: &'static str,
    pub name: &'static str,
}

const ANTHROPIC_MODELS: &[ModelInfo] = &[
    ModelInfo {
        id: "claude-sonnet-4-5-20250929",
        name: "Sonnet 4.5",
    },
    ModelInfo {
        id: "claude-sonnet-4-20250514",
        name: "Sonnet 4",
    },
    ModelInfo {
        id: "claude-haiku-3-5-20241022",
        name: "Haiku 3.5",
    },
    ModelInfo {
        id: "claude-opus-4-20250514",
        name: "Opus 4",
    },
];

const OPENAI_MODELS: &[ModelInfo] = &[
    ModelInfo {
        id: "gpt-4o",
        name: "GPT-4o",
    },
    ModelInfo {
        id: "gpt-4o-mini",
        name: "GPT-4o Mini",
    },
    ModelInfo {
        id: "o3",
        name: "o3",
    },
    ModelInfo {
        id: "o3-mini",
        name: "o3 Mini",
    },
    ModelInfo {
        id: "o4-mini",
        name: "o4 Mini",
    },
];

impl LlmConfig {
    /// Default model for a given provider.
    pub fn default_model(provider: &str) -> &'static str {
        match provider {
            "anthropic" => "claude-sonnet-4-5-20250929",
            "openai" | "openai-compatible" => "gpt-4o",
            _ => "claude-sonnet-4-5-20250929",
        }
    }

    /// Available models for a provider.
    pub fn available_models(provider: &str) -> &'static [ModelInfo] {
        match provider {
            "anthropic" => ANTHROPIC_MODELS,
            "openai" | "openai-compatible" => OPENAI_MODELS,
            _ => &[],
        }
    }

    /// Short display name for a model ID (e.g. "claude-sonnet-4-5-20250929" → "Sonnet 4.5").
    pub fn model_display_name(model_id: &str) -> &str {
        for list in [ANTHROPIC_MODELS, OPENAI_MODELS] {
            for m in list {
                if m.id == model_id {
                    return m.name;
                }
            }
        }
        // Fallback: return the raw ID
        model_id
    }
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: "anthropic".to_string(),
            model: "claude-sonnet-4-5-20250929".to_string(),
            base_url: None,
            max_output_tokens: 4096,
            api_key: None,
        }
    }
}
