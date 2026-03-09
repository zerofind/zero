use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use futures_util::StreamExt;
use llm_kit_core::ToolSet;
use llm_kit_core::prompt::convert_to_language_model_prompt::convert_to_language_model_prompt;
use llm_kit_core::prompt::standardize::StandardizedPrompt;
use llm_kit_core::tool::{execute_tool_call, prepare_tools_and_tool_choice};
use llm_kit_provider::LanguageModel;
use llm_kit_provider::language_model::call_options::LanguageModelCallOptions;
use llm_kit_provider::language_model::stream_part::LanguageModelStreamPart;
use llm_kit_provider_utils::message::{
    AssistantContentPart, AssistantMessage, Message, TextPart, ToolCallPart, ToolContentPart,
    ToolMessage, ToolResultOutput, ToolResultPart, UserMessage,
};
use llm_kit_provider_utils::tool::{ToolCall, ToolOutput};
use serde_json::json;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use crate::SharedIndex;
use crate::config::LlmConfig;
use crate::error::{LlmError, friendly_error};
use crate::prompt::system_prompt;
use crate::provider::build_model;
use crate::tools::build_tools;

const MAX_ITERATIONS: u32 = 10;

/// Events emitted during an LLM response.
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// Streaming text chunk.
    TextDelta(String),
    /// A thinking/reasoning text chunk.
    ThinkingDelta(String),
    /// A tool call is starting.
    ToolCallStart(String),
    /// A tool call completed with a result summary.
    ToolCallDone(String, String),
    /// Full response text (sent when done).
    Done(String),
    /// An error occurred.
    Error(String),
}

/// The Zero LLM agent with conversation history.
///
/// Does **not** own an `IndexManager` — search tools hold a shared
/// reference that is populated when the index loads. The agent works
/// immediately; search tools gracefully degrade until the index is ready.
pub struct ZeroAgent {
    model: Arc<dyn LanguageModel>,
    tools: ToolSet,
    index: SharedIndex,
    messages: Vec<Message>,
    max_output_tokens: u32,
    thinking: bool,
    thinking_budget: u32,
    runtime: tokio::runtime::Handle,
}

impl ZeroAgent {
    /// Create a new agent from config and a shared index reference.
    ///
    /// The runtime handle must come from an externally-owned `Runtime`
    /// (typically held by `LlmService`) so it outlives the agent.
    pub fn new(
        config: &LlmConfig,
        index: SharedIndex,
        runtime: tokio::runtime::Handle,
    ) -> Result<Self, LlmError> {
        let model = build_model(config)?;
        let tools = build_tools(index.clone());

        Ok(Self {
            model,
            tools,
            index,
            messages: Vec::new(),
            max_output_tokens: config.max_output_tokens,
            thinking: config.thinking,
            thinking_budget: config.thinking_budget,
            runtime,
        })
    }

    /// Ask a question and receive streaming events via a channel.
    ///
    /// The system prompt is generated fresh each call so it reflects
    /// the current index state (file count, roots).
    pub fn ask(&mut self, question: &str) -> mpsc::UnboundedReceiver<StreamEvent> {
        let (tx, rx) = mpsc::unbounded_channel();
        let truncated: String = question.chars().take(100).collect();
        info!(question = %truncated, history_len = self.messages.len(), "User ask");

        self.messages
            .push(Message::User(UserMessage::new(question)));

        let model = Arc::clone(&self.model);
        let tools = self.tools.clone();
        let system = system_prompt(&self.index);
        let messages = self.messages.clone();
        let max_tokens = self.max_output_tokens;
        let thinking = self.thinking;
        let thinking_budget = self.thinking_budget;

        self.runtime.spawn(async move {
            match run_agentic_loop(
                model,
                tools,
                system,
                messages,
                max_tokens,
                thinking,
                thinking_budget,
                &tx,
            )
            .await
            {
                Ok(full_text) => {
                    let _ = tx.send(StreamEvent::Done(full_text));
                }
                Err(e) => {
                    let raw = format!("{e}");
                    warn!(error = %raw, "LLM request failed");
                    let _ = tx.send(StreamEvent::Error(friendly_error(&raw)));
                }
            }
        });

        rx
    }

    /// Record the assistant's response in conversation history.
    pub fn record_response(&mut self, response: &str) {
        self.messages
            .push(Message::Assistant(AssistantMessage::new(response)));
    }

    /// Clear conversation history for a new conversation.
    pub fn clear(&mut self) {
        self.messages.clear();
    }
}

/// Build provider_options for Anthropic: disable tool streaming beta header,
/// and optionally enable thinking mode.
fn build_anthropic_provider_options(
    thinking: bool,
    thinking_budget: u32,
) -> HashMap<String, HashMap<String, serde_json::Value>> {
    let mut anthropic = HashMap::new();
    anthropic.insert("toolStreaming".to_string(), json!(false));

    if thinking {
        anthropic.insert(
            "thinking".to_string(),
            json!({"type": "enabled", "budgetTokens": thinking_budget}),
        );
    }

    let mut opts = HashMap::new();
    opts.insert("anthropic".to_string(), anthropic);
    opts
}

/// Drive the agentic loop: stream LLM -> execute tools -> repeat.
#[allow(clippy::too_many_arguments)]
async fn run_agentic_loop(
    model: Arc<dyn LanguageModel>,
    tools: ToolSet,
    system: String,
    mut messages: Vec<Message>,
    max_output_tokens: u32,
    thinking: bool,
    thinking_budget: u32,
    tx: &mpsc::UnboundedSender<StreamEvent>,
) -> Result<String> {
    let started = Instant::now();

    let (provider_tools, tool_choice) = prepare_tools_and_tool_choice(Some(&tools), None);
    let provider_options = build_anthropic_provider_options(thinking, thinking_budget);

    let tool_count = provider_tools.as_ref().map_or(0, |t| t.len());
    info!(
        model = %model.model_id(),
        max_output_tokens,
        thinking,
        tool_count,
        "Agentic loop starting"
    );

    let mut full_text = String::new();
    let mut total_tool_calls = 0u32;

    for iteration in 0..MAX_ITERATIONS {
        info!(iteration, "LLM call starting");
        let iter_started = Instant::now();

        let std_prompt = StandardizedPrompt::with_system(system.clone(), messages.clone());
        let lm_prompt = convert_to_language_model_prompt(std_prompt)
            .map_err(|e| anyhow::anyhow!("Prompt conversion error: {e}"))?;

        let mut options =
            LanguageModelCallOptions::new(lm_prompt).with_max_output_tokens(max_output_tokens);
        options.provider_options = Some(provider_options.clone());
        if let Some(ref t) = provider_tools {
            options.tools = Some(t.clone());
        }
        if let Some(ref tc) = tool_choice {
            options.tool_choice = Some(tc.clone());
        }

        debug!(provider_options = ?provider_options, "Request provider_options");

        let response = model
            .do_stream(options)
            .await
            .map_err(|e| anyhow::anyhow!("LLM stream error: {e}"))?;

        let mut stream = response.stream;
        let mut iteration_text = String::new();
        let mut pending_tool_calls = Vec::new();

        while let Some(part) = stream.next().await {
            match part {
                LanguageModelStreamPart::TextDelta(td) => {
                    iteration_text.push_str(&td.delta);
                    let _ = tx.send(StreamEvent::TextDelta(td.delta));
                }
                LanguageModelStreamPart::ReasoningDelta(rd) => {
                    let _ = tx.send(StreamEvent::ThinkingDelta(rd.delta));
                }
                LanguageModelStreamPart::ReasoningStart(_) => {
                    debug!(iteration, "Reasoning block started");
                }
                LanguageModelStreamPart::ReasoningEnd(_) => {
                    debug!(iteration, "Reasoning block ended");
                }
                LanguageModelStreamPart::ToolCall(tc) => {
                    info!(tool = %tc.tool_name, id = %tc.tool_call_id, "Tool call received");
                    pending_tool_calls.push(tc);
                }
                LanguageModelStreamPart::Error(e) => {
                    let raw = e
                        .error
                        .as_str()
                        .map(String::from)
                        .unwrap_or_else(|| e.error.to_string());
                    warn!(error = %raw, iteration, "Stream error");
                    let _ = tx.send(StreamEvent::Error(friendly_error(&raw)));
                }
                LanguageModelStreamPart::Finish(_) => break,
                _ => {}
            }
        }

        let iter_ms = iter_started.elapsed().as_millis();
        full_text.push_str(&iteration_text);

        if pending_tool_calls.is_empty() {
            info!(
                iteration,
                text_len = full_text.len(),
                duration_ms = iter_ms,
                "No tool calls, response complete"
            );
            break;
        }

        info!(
            iteration,
            tool_count = pending_tool_calls.len(),
            text_len = iteration_text.len(),
            duration_ms = iter_ms,
            "Executing tool calls"
        );

        // Build assistant message with text + tool calls for conversation history
        let mut assistant_parts: Vec<AssistantContentPart> = Vec::new();
        if !iteration_text.is_empty() {
            assistant_parts.push(AssistantContentPart::Text(TextPart::new(&iteration_text)));
        }
        for tc in &pending_tool_calls {
            let input: serde_json::Value =
                serde_json::from_str(&tc.input).unwrap_or(serde_json::Value::Null);
            assistant_parts.push(AssistantContentPart::ToolCall(ToolCallPart::new(
                &tc.tool_call_id,
                &tc.tool_name,
                input,
            )));
        }
        messages.push(Message::Assistant(AssistantMessage::with_parts(
            assistant_parts,
        )));

        // Execute each tool call and collect results
        let mut tool_result_parts: Vec<ToolContentPart> = Vec::new();

        for tc in pending_tool_calls {
            total_tool_calls += 1;
            let tool_name = tc.tool_name.clone();
            let tool_call_id = tc.tool_call_id.clone();

            let _ = tx.send(StreamEvent::ToolCallStart(tool_name.clone()));
            info!(tool = %tool_name, n = total_tool_calls, "Executing tool");
            let tool_started = Instant::now();

            let input: serde_json::Value =
                serde_json::from_str(&tc.input).unwrap_or(serde_json::Value::Null);
            let exec_call = ToolCall::new(&tool_call_id, &tool_name, input);

            let output = execute_tool_call(exec_call, &tools, vec![], None, None, None).await;

            let tool_ms = tool_started.elapsed().as_millis();

            match output {
                Some(ToolOutput::Result(result)) => {
                    let summary = summarize_tool_result(&result.output);
                    info!(tool = %tool_name, summary = %summary, duration_ms = tool_ms, "Tool succeeded");
                    let _ = tx.send(StreamEvent::ToolCallDone(tool_name.clone(), summary));
                    tool_result_parts.push(ToolContentPart::ToolResult(ToolResultPart::new(
                        &tool_call_id,
                        &tool_name,
                        ToolResultOutput::json(result.output),
                    )));
                }
                Some(ToolOutput::Error(error)) => {
                    let error_msg = error
                        .error
                        .as_str()
                        .map(String::from)
                        .unwrap_or_else(|| error.error.to_string());
                    warn!(tool = %tool_name, error = %error_msg, duration_ms = tool_ms, "Tool failed");
                    let _ = tx.send(StreamEvent::ToolCallDone(
                        tool_name.clone(),
                        format!("error: {error_msg}"),
                    ));
                    tool_result_parts.push(ToolContentPart::ToolResult(ToolResultPart::new(
                        &tool_call_id,
                        &tool_name,
                        ToolResultOutput::error_json(error.error),
                    )));
                }
                None => {
                    warn!(tool = %tool_name, duration_ms = tool_ms, "Tool has no execute function");
                    let _ = tx.send(StreamEvent::ToolCallDone(
                        tool_name.clone(),
                        "error: no execute function".into(),
                    ));
                    tool_result_parts.push(ToolContentPart::ToolResult(ToolResultPart::new(
                        &tool_call_id,
                        &tool_name,
                        ToolResultOutput::error_text("Tool has no execute function"),
                    )));
                }
            }
        }

        messages.push(Message::Tool(ToolMessage::new(tool_result_parts)));
    }

    info!(
        total_tool_calls,
        text_len = full_text.len(),
        total_ms = started.elapsed().as_millis() as u64,
        "Agentic loop complete"
    );

    Ok(full_text)
}

fn summarize_tool_result(output: &serde_json::Value) -> String {
    let s = output.to_string();
    if s.len() <= 80 {
        s
    } else {
        format!("{}...", &s[..77])
    }
}
