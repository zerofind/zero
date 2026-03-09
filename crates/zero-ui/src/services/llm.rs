use std::sync::RwLock;

use gpui::*;
use tokio::sync::mpsc;

use zero::prelude::IndexManager;
use zero_llm::{LlmConfig, SharedIndex, StreamEvent, ZeroAgent};

pub enum LlmEvent {
    Configured,
    Error(String),
}

impl EventEmitter<LlmEvent> for LlmService {}

/// LLM agent service — fully independent of the search index lifecycle.
///
/// Starts immediately on app launch. If a saved API key exists, the agent
/// is ready to chat right away. The search index is injected later via
/// `set_index()` — tools that need it gracefully degrade until then.
pub struct LlmService {
    config: LlmConfig,
    agent: Option<ZeroAgent>,
    index: SharedIndex,
    runtime: tokio::runtime::Runtime,
    error: Option<String>,
}

impl LlmService {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        let config = crate::session::Settings::load().llm;
        let index: SharedIndex = std::sync::Arc::new(RwLock::new(None));

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .thread_name("zero-llm")
            .build()
            .expect("failed to create LLM tokio runtime");

        let has_key = config.api_key_for_provider(&config.provider).is_some();
        let agent = if config.enabled && has_key {
            match ZeroAgent::new(&config, index.clone(), runtime.handle().clone()) {
                Ok(agent) => {
                    tracing::info!(
                        provider = %config.provider,
                        model = %config.model,
                        "LLM agent ready on launch"
                    );
                    Some(agent)
                }
                Err(e) => {
                    tracing::warn!(error = %e, "LLM agent failed to start on launch");
                    None
                }
            }
        } else {
            tracing::debug!(
                enabled = config.enabled,
                has_key,
                "LLM service started (no agent — waiting for API key)"
            );
            None
        };

        Self {
            config,
            agent,
            index,
            runtime,
            error: None,
        }
    }

    /// Save an API key for a provider and build the agent immediately.
    ///
    /// Switches to the provider if not already active, using its default model.
    pub fn set_api_key(&mut self, provider: &str, api_key: &str, cx: &mut Context<Self>) {
        // Store key in the provider-specific field
        match provider {
            "anthropic" => self.config.anthropic_api_key = Some(api_key.to_string()),
            "openai" | "openai-compatible" => {
                self.config.openai_api_key = Some(api_key.to_string());
            }
            _ => {}
        }

        let provider_changed = self.config.provider != provider;
        self.config.provider = provider.to_string();
        if provider_changed {
            self.config.model = LlmConfig::default_model(provider).to_string();
        }
        self.config.enabled = true;

        self.rebuild_agent(cx);
    }

    /// Switch model, auto-switching provider if the model belongs to a different one.
    pub fn set_model(&mut self, model: &str, thinking: bool, cx: &mut Context<Self>) {
        if self.config.model == model && self.config.thinking == thinking {
            return;
        }
        // Auto-switch provider if needed
        if let Some(needed) = LlmConfig::provider_for_model(model)
            && self.config.provider != needed
        {
            self.config.provider = needed.to_string();
        }
        self.config.model = model.to_string();
        self.config.thinking = thinking;
        self.rebuild_agent(cx);
    }

    fn rebuild_agent(&mut self, cx: &mut Context<Self>) {
        let mut settings = crate::session::Settings::load();
        settings.llm = self.config.clone();
        settings.save();

        match ZeroAgent::new(
            &self.config,
            self.index.clone(),
            self.runtime.handle().clone(),
        ) {
            Ok(agent) => {
                self.agent = Some(agent);
                self.error = None;
                tracing::info!(
                    provider = %self.config.provider,
                    model = %self.config.model,
                    "LLM agent configured"
                );
                cx.emit(LlmEvent::Configured);
            }
            Err(e) => {
                let msg = e.to_string();
                tracing::warn!(error = %msg, "LLM agent failed to configure");
                self.error = Some(msg.clone());
                cx.emit(LlmEvent::Error(msg));
            }
        }
        cx.notify();
    }

    /// Inject the search index. Tools pick it up on their next call.
    pub fn set_index(&self, manager: IndexManager) {
        let file_count = manager.total_file_count();
        let root_count = manager.roots().len();
        match self.index.write() {
            Ok(mut guard) => {
                *guard = Some(manager);
                tracing::info!(
                    files = file_count,
                    roots = root_count,
                    "Search index provided to LLM tools"
                );
            }
            Err(e) => {
                tracing::warn!(error = %e.to_string(), "Failed to set index on LLM service");
            }
        }
    }

    pub fn ask(&mut self, question: &str) -> Option<mpsc::UnboundedReceiver<StreamEvent>> {
        if self.agent.is_none() {
            tracing::warn!("ask() called but no agent configured");
        }
        self.agent.as_mut().map(|agent| agent.ask(question))
    }

    pub fn record_response(&mut self, response: &str) {
        if let Some(agent) = &mut self.agent {
            agent.record_response(response);
        }
    }

    pub fn clear_history(&mut self) {
        if let Some(agent) = &mut self.agent {
            agent.clear();
        }
    }

    pub fn is_ready(&self) -> bool {
        self.agent.is_some()
    }

    #[allow(dead_code)]
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn model(&self) -> &str {
        &self.config.model
    }

    pub fn thinking(&self) -> bool {
        self.config.thinking
    }

    pub fn config(&self) -> &LlmConfig {
        &self.config
    }
}
