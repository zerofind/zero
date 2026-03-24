use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LlmConfig {
    pub enabled: bool,
    pub provider: String,
    pub model: String,
    pub base_url: Option<String>,
    pub max_output_tokens: u32,
    pub anthropic_api_key: Option<String>,
    pub openai_api_key: Option<String>,
    pub thinking: bool,
    pub thinking_budget: u32,
}

/// A model available for selection.
#[derive(Debug, Clone, Copy)]
pub struct ModelInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub thinking: bool,
    pub max_output_tokens: u32,
}

impl ModelInfo {
    /// Unique key for the model picker (disambiguates thinking vs non-thinking).
    pub fn picker_key(&self) -> String {
        if self.thinking {
            format!("{}:thinking", self.id)
        } else {
            self.id.to_string()
        }
    }
}

const ANTHROPIC_MODELS: &[ModelInfo] = &[
    ModelInfo {
        id: "claude-sonnet-4-6",
        name: "Sonnet 4.6",
        thinking: false,
        max_output_tokens: 16384,
    },
    ModelInfo {
        id: "claude-sonnet-4-5-20250514",
        name: "Sonnet 4.5",
        thinking: false,
        max_output_tokens: 16384,
    },
    ModelInfo {
        id: "claude-sonnet-4-20250514",
        name: "Sonnet 4",
        thinking: false,
        max_output_tokens: 8192,
    },
    ModelInfo {
        id: "claude-opus-4-6",
        name: "Opus 4.6",
        thinking: false,
        max_output_tokens: 16384,
    },
    ModelInfo {
        id: "claude-opus-4-5-20250514",
        name: "Opus 4.5",
        thinking: false,
        max_output_tokens: 16384,
    },
    ModelInfo {
        id: "claude-opus-4-20250514",
        name: "Opus 4",
        thinking: false,
        max_output_tokens: 8192,
    },
    ModelInfo {
        id: "claude-haiku-4-5-20251001",
        name: "Haiku 4.5",
        thinking: false,
        max_output_tokens: 8192,
    },
];

const OPENAI_MODELS: &[ModelInfo] = &[
    ModelInfo {
        id: "gpt-5-mini",
        name: "GPT-5 Mini",
        thinking: false,
        max_output_tokens: 16384,
    },
    ModelInfo {
        id: "gpt-5",
        name: "GPT-5",
        thinking: false,
        max_output_tokens: 16384,
    },
    ModelInfo {
        id: "gpt-5-nano",
        name: "GPT-5 Nano",
        thinking: false,
        max_output_tokens: 16384,
    },
    ModelInfo {
        id: "gpt-4o-mini",
        name: "GPT-4o Mini",
        thinking: false,
        max_output_tokens: 16384,
    },
    ModelInfo {
        id: "o3",
        name: "o3",
        thinking: false,
        max_output_tokens: 100_000,
    },
    ModelInfo {
        id: "o3-mini",
        name: "o3 Mini",
        thinking: false,
        max_output_tokens: 65536,
    },
];

impl LlmConfig {
    /// Default model for a given provider.
    pub fn default_model(provider: &str) -> &'static str {
        match provider {
            "anthropic" => "claude-sonnet-4-6",
            "openai" | "openai-compatible" => "gpt-5-mini",
            _ => "claude-sonnet-4-6",
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

    /// API key for a specific provider (settings only, not env vars).
    pub fn api_key_for_provider(&self, provider: &str) -> Option<&str> {
        let key = match provider {
            "anthropic" => self.anthropic_api_key.as_deref(),
            "openai" | "openai-compatible" => self.openai_api_key.as_deref(),
            _ => None,
        };
        key.filter(|k| !k.is_empty())
    }

    /// Providers that have a saved API key.
    pub fn providers_with_keys(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.api_key_for_provider("anthropic").is_some() {
            out.push("anthropic");
        }
        if self.api_key_for_provider("openai").is_some() {
            out.push("openai");
        }
        out
    }

    /// Look up which provider owns a model ID.
    pub fn provider_for_model(model_id: &str) -> Option<&'static str> {
        if ANTHROPIC_MODELS.iter().any(|m| m.id == model_id) {
            Some("anthropic")
        } else if OPENAI_MODELS.iter().any(|m| m.id == model_id) {
            Some("openai")
        } else {
            None
        }
    }

    /// Short display name for a model ID, optionally disambiguated by thinking mode.
    pub fn model_display_name(model_id: &str, thinking: bool) -> &'static str {
        for list in [ANTHROPIC_MODELS, OPENAI_MODELS] {
            for m in list {
                if m.id == model_id && m.thinking == thinking {
                    return m.name;
                }
            }
        }
        // Fallback: try without thinking match
        for list in [ANTHROPIC_MODELS, OPENAI_MODELS] {
            for m in list {
                if m.id == model_id {
                    return m.name;
                }
            }
        }
        "Unknown"
    }
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: "anthropic".to_string(),
            model: "claude-sonnet-4-6".to_string(),
            base_url: None,
            max_output_tokens: 4096,
            anthropic_api_key: None,
            openai_api_key: None,
            thinking: false,
            thinking_budget: 10000,
        }
    }
}
