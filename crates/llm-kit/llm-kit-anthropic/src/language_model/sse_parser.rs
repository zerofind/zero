use bytes::Bytes;
use futures_util::stream::Stream;
use llm_kit_provider::language_model::call_warning::LanguageModelCallWarning;
use llm_kit_provider::language_model::stream_part::LanguageModelStreamPart;
use std::collections::HashMap;
use std::pin::Pin;
use std::task::{Context, Poll};

use super::stream_schema::AnthropicChunk;
use crate::map_stop_reason::map_anthropic_stop_reason;

/// Parse Server-Sent Events from a byte stream and convert to SDK stream parts
///
/// # Arguments
///
/// * `byte_stream` - Raw bytes from the HTTP response
/// * `uses_json_response_tool` - Whether JSON response tool is being used
/// * `warnings` - Initial warnings to emit at stream start
/// * `include_raw_chunks` - Whether to include raw chunks in the stream
///
/// # Returns
///
/// A stream of `LanguageModelStreamPart` events
pub fn parse_sse_stream(
    byte_stream: impl Stream<Item = Result<Bytes, reqwest::Error>> + Unpin + Send + 'static,
    uses_json_response_tool: bool,
    warnings: Vec<LanguageModelCallWarning>,
    include_raw_chunks: bool,
) -> impl Stream<Item = LanguageModelStreamPart> + Unpin + Send {
    Box::pin(AnthropicStreamParser::new(
        Box::pin(byte_stream),
        uses_json_response_tool,
        warnings,
        include_raw_chunks,
    ))
}

/// State machine for parsing Anthropic streaming responses
struct AnthropicStreamParser {
    /// Underlying byte stream
    byte_stream: Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>,

    /// Buffer for incomplete SSE events
    buffer: String,

    /// Pending output events
    pending_events: Vec<LanguageModelStreamPart>,

    /// Content blocks being tracked
    content_blocks: HashMap<u32, ContentBlockState>,

    /// MCP tool calls being tracked
    mcp_tool_calls: HashMap<String, McpToolCallState>,

    /// Current block type being processed
    current_block_type: Option<String>,

    /// Usage information
    usage: UsageState,

    /// Provider metadata
    cache_creation_input_tokens: Option<u32>,
    stop_sequence: Option<String>,
    container: Option<serde_json::Value>,

    /// Configuration
    uses_json_response_tool: bool,
    #[allow(dead_code)]
    include_raw_chunks: bool,

    /// Finish reason
    finish_reason:
        Option<llm_kit_provider::language_model::finish_reason::LanguageModelFinishReason>,

    /// Whether stream has started
    stream_started: bool,

    /// Initial warnings
    warnings: Vec<LanguageModelCallWarning>,

    /// Response metadata
    response_id: Option<String>,
    model_id: Option<String>,
}

/// State for tracking content blocks
enum ContentBlockState {
    Text,
    Reasoning,
    ToolCall {
        tool_call_id: String,
        tool_name: String,
        input: String,
        provider_executed: bool,
        first_delta: bool,
    },
}

/// State for tracking MCP tool calls
struct McpToolCallState {
    #[allow(dead_code)]
    tool_call_id: String,
    tool_name: String,
    server_name: String,
}

/// Usage tracking state
struct UsageState {
    input_tokens: u64,
    output_tokens: u64,
    cached_input_tokens: u64,
}

impl AnthropicStreamParser {
    fn new(
        byte_stream: Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>,
        uses_json_response_tool: bool,
        warnings: Vec<LanguageModelCallWarning>,
        include_raw_chunks: bool,
    ) -> Self {
        Self {
            byte_stream,
            buffer: String::new(),
            pending_events: Vec::new(),
            content_blocks: HashMap::new(),
            mcp_tool_calls: HashMap::new(),
            current_block_type: None,
            usage: UsageState {
                input_tokens: 0,
                output_tokens: 0,
                cached_input_tokens: 0,
            },
            cache_creation_input_tokens: None,
            stop_sequence: None,
            container: None,
            uses_json_response_tool,
            include_raw_chunks,
            finish_reason: None,
            stream_started: false,
            warnings,
            response_id: None,
            model_id: None,
        }
    }

    /// Process a chunk event and generate stream parts
    fn process_chunk(&mut self, chunk: AnthropicChunk) {
        use llm_kit_provider::language_model::stream_part::finish::LanguageModelStreamFinish;

        match chunk {
            AnthropicChunk::Ping => {
                // Ignore ping events
            }

            AnthropicChunk::MessageStart { message } => {
                // Track usage and metadata
                self.usage.input_tokens = message.usage.input_tokens as u64;
                self.usage.cached_input_tokens =
                    message.usage.cache_read_input_tokens.unwrap_or(0) as u64;
                self.cache_creation_input_tokens = message.usage.cache_creation_input_tokens;
                self.response_id = message.id;
                self.model_id = message.model;

                // Emit response metadata
                if self.response_id.is_some() || self.model_id.is_some() {
                    self.pending_events.push(LanguageModelStreamPart::ResponseMetadata(
                        llm_kit_provider::language_model::response_metadata::LanguageModelResponseMetadata {
                            id: self.response_id.clone(),
                            model_id: self.model_id.clone(),
                            timestamp: None,
                        },
                    ));
                }
            }

            AnthropicChunk::ContentBlockStart {
                index,
                content_block,
            } => {
                self.process_content_block_start(index, content_block);
            }

            AnthropicChunk::ContentBlockDelta { index, delta } => {
                self.process_content_block_delta(index, delta);
            }

            AnthropicChunk::ContentBlockStop { index } => {
                self.process_content_block_stop(index);
            }

            AnthropicChunk::MessageDelta { delta, usage } => {
                // Update usage
                self.usage.output_tokens = usage.output_tokens as u64;

                // Update finish reason
                if let Some(stop_reason) = delta.stop_reason {
                    self.finish_reason = Some(map_anthropic_stop_reason(
                        Some(&stop_reason),
                        self.uses_json_response_tool,
                    ));
                }

                // Update stop sequence
                self.stop_sequence = delta.stop_sequence;
            }

            AnthropicChunk::MessageStop => {
                // Emit finish event
                let usage = llm_kit_provider::language_model::usage::LanguageModelUsage {
                    input_tokens: self.usage.input_tokens,
                    output_tokens: self.usage.output_tokens,
                    total_tokens: self.usage.input_tokens + self.usage.output_tokens,
                    reasoning_tokens: 0,
                    cached_input_tokens: self.usage.cached_input_tokens,
                };

                // Build provider metadata
                let provider_metadata = self.build_provider_metadata();

                let finish_reason = self.finish_reason.clone().unwrap_or(
                    llm_kit_provider::language_model::finish_reason::LanguageModelFinishReason::Unknown
                );

                self.pending_events.push(LanguageModelStreamPart::Finish(
                    LanguageModelStreamFinish::with_metadata(
                        usage,
                        finish_reason,
                        provider_metadata,
                    ),
                ));
            }

            AnthropicChunk::Error { error } => {
                // Emit error event
                self.pending_events.push(LanguageModelStreamPart::Error(
                    llm_kit_provider::language_model::stream_part::error::LanguageModelStreamError::new(
                        serde_json::json!(error.message),
                    ),
                ));
            }
        }
    }

    /// Process content block start events
    fn process_content_block_start(
        &mut self,
        index: u32,
        content_block: super::stream_schema::ContentBlockStart,
    ) {
        use super::stream_schema::ContentBlockStart;

        match content_block {
            ContentBlockStart::Text { .. } => {
                if self.uses_json_response_tool {
                    // Ignore text blocks when JSON response tool is used
                    return;
                }

                self.content_blocks.insert(index, ContentBlockState::Text);
                self.current_block_type = Some("text".to_string());

                self.pending_events.push(LanguageModelStreamPart::TextStart(
                    llm_kit_provider::language_model::stream_part::text_start::LanguageModelStreamTextStart::new(
                        index.to_string(),
                    ),
                ));
            }

            ContentBlockStart::Thinking { .. } => {
                self.content_blocks
                    .insert(index, ContentBlockState::Reasoning);
                self.current_block_type = Some("thinking".to_string());

                self.pending_events
                    .push(LanguageModelStreamPart::ReasoningStart(
                        llm_kit_provider::language_model::stream_part::reasoning_start::LanguageModelStreamReasoningStart::new(
                            index.to_string(),
                        ),
                    ));
            }

            ContentBlockStart::RedactedThinking { data } => {
                self.content_blocks
                    .insert(index, ContentBlockState::Reasoning);
                self.current_block_type = Some("redacted_thinking".to_string());

                let mut metadata = HashMap::new();
                let mut anthropic_meta = HashMap::new();
                anthropic_meta.insert("redactedData".to_string(), serde_json::json!(data));
                metadata.insert("anthropic".to_string(), anthropic_meta);

                self.pending_events
                    .push(LanguageModelStreamPart::ReasoningStart(
                        llm_kit_provider::language_model::stream_part::reasoning_start::LanguageModelStreamReasoningStart::with_metadata(
                            index.to_string(),
                            Some(metadata),
                        ),
                    ));
            }

            ContentBlockStart::ToolUse { id, name } => {
                let is_json_response_tool = self.uses_json_response_tool && name == "json";

                if is_json_response_tool {
                    // Treat as text block
                    self.content_blocks.insert(index, ContentBlockState::Text);
                    self.current_block_type = Some("tool_use".to_string());

                    self.pending_events.push(LanguageModelStreamPart::TextStart(
                        llm_kit_provider::language_model::stream_part::text_start::LanguageModelStreamTextStart::new(
                            index.to_string(),
                        ),
                    ));
                } else {
                    self.content_blocks.insert(
                        index,
                        ContentBlockState::ToolCall {
                            tool_call_id: id.clone(),
                            tool_name: name.clone(),
                            input: String::new(),
                            provider_executed: false,
                            first_delta: true,
                        },
                    );
                    self.current_block_type = Some("tool_use".to_string());

                    self.pending_events
                        .push(LanguageModelStreamPart::ToolInputStart(
                            llm_kit_provider::language_model::stream_part::tool_input_start::LanguageModelStreamToolInputStart::new(
                                id,
                                name,
                            ),
                        ));
                }
            }

            ContentBlockStart::ServerToolUse { id, name, .. } => {
                // Check if this is a supported server tool
                if matches!(
                    name.as_str(),
                    "web_fetch"
                        | "web_search"
                        | "code_execution"
                        | "text_editor_code_execution"
                        | "bash_code_execution"
                ) {
                    // Map tool names
                    let mapped_tool_name =
                        if name == "text_editor_code_execution" || name == "bash_code_execution" {
                            "code_execution".to_string()
                        } else {
                            name.clone()
                        };

                    self.content_blocks.insert(
                        index,
                        ContentBlockState::ToolCall {
                            tool_call_id: id.clone(),
                            tool_name: name.clone(),
                            input: String::new(),
                            provider_executed: true,
                            first_delta: true,
                        },
                    );
                    self.current_block_type = Some("server_tool_use".to_string());

                    let mut tool_input_start = llm_kit_provider::language_model::stream_part::tool_input_start::LanguageModelStreamToolInputStart::new(
                        id,
                        mapped_tool_name,
                    );
                    tool_input_start.provider_executed = Some(true);
                    self.pending_events
                        .push(LanguageModelStreamPart::ToolInputStart(tool_input_start));
                }
            }

            ContentBlockStart::McpToolUse {
                id,
                name,
                input,
                server_name,
            } => {
                // Track MCP tool call
                self.mcp_tool_calls.insert(
                    id.clone(),
                    McpToolCallState {
                        tool_call_id: id.clone(),
                        tool_name: name.clone(),
                        server_name: server_name.clone(),
                    },
                );
                self.current_block_type = Some("mcp_tool_use".to_string());

                // Emit complete tool call
                let input_str = serde_json::to_string(&input).unwrap_or_else(|_| "{}".to_string());
                let mut metadata = HashMap::new();
                let mut anthropic_meta = HashMap::new();
                anthropic_meta.insert("type".to_string(), serde_json::json!("mcp-tool-use"));
                anthropic_meta.insert("serverName".to_string(), serde_json::json!(server_name));
                metadata.insert("anthropic".to_string(), anthropic_meta);

                self.pending_events.push(LanguageModelStreamPart::ToolCall(
                    llm_kit_provider::language_model::content::tool_call::LanguageModelToolCall::with_options(
                        id,
                        name,
                        input_str,
                        Some(true),
                        Some(metadata),
                    ),
                ));
            }

            ContentBlockStart::McpToolResult {
                tool_use_id,
                is_error,
                content,
            } => {
                self.current_block_type = Some("mcp_tool_result".to_string());

                // Get the tool call state
                if let Some(mcp_state) = self.mcp_tool_calls.get(&tool_use_id) {
                    let result_str =
                        serde_json::to_string(&content).unwrap_or_else(|_| "[]".to_string());
                    let result_value: serde_json::Value =
                        serde_json::from_str(&result_str).unwrap_or(serde_json::json!([]));

                    let mut metadata = HashMap::new();
                    let mut anthropic_meta = HashMap::new();
                    anthropic_meta.insert("type".to_string(), serde_json::json!("mcp-tool-result"));
                    anthropic_meta.insert(
                        "serverName".to_string(),
                        serde_json::json!(mcp_state.server_name),
                    );
                    metadata.insert("anthropic".to_string(), anthropic_meta);

                    self.pending_events.push(LanguageModelStreamPart::ToolResult(
                        llm_kit_provider::language_model::content::tool_result::LanguageModelToolResult::with_options(
                            tool_use_id,
                            mcp_state.tool_name.clone(),
                            result_value,
                            Some(is_error),
                            Some(true),
                            Some(metadata),
                        ),
                    ));
                }
            }

            ContentBlockStart::WebFetchToolResult {
                tool_use_id,
                content,
            } => {
                self.current_block_type = Some("web_fetch_tool_result".to_string());
                self.emit_web_fetch_result(tool_use_id, content);
            }

            ContentBlockStart::WebSearchToolResult {
                tool_use_id,
                content,
            } => {
                self.current_block_type = Some("web_search_tool_result".to_string());
                self.emit_web_search_result(tool_use_id, content);
            }

            ContentBlockStart::CodeExecutionToolResult {
                tool_use_id,
                content,
            } => {
                self.current_block_type = Some("code_execution_tool_result".to_string());
                self.emit_code_execution_result(tool_use_id, content);
            }

            ContentBlockStart::BashCodeExecutionToolResult {
                tool_use_id,
                content,
            } => {
                self.current_block_type = Some("bash_code_execution_tool_result".to_string());
                self.emit_bash_code_execution_result(tool_use_id, content);
            }

            ContentBlockStart::TextEditorCodeExecutionToolResult {
                tool_use_id,
                content,
            } => {
                self.current_block_type =
                    Some("text_editor_code_execution_tool_result".to_string());
                self.emit_text_editor_code_execution_result(tool_use_id, content);
            }
        }
    }

    /// Process content block delta events
    fn process_content_block_delta(
        &mut self,
        index: u32,
        delta: super::stream_schema::ContentBlockDelta,
    ) {
        use super::stream_schema::ContentBlockDelta;

        match delta {
            ContentBlockDelta::TextDelta { text } => {
                if self.uses_json_response_tool {
                    // Ignore text deltas when JSON response tool is used
                    return;
                }

                self.pending_events
                    .push(LanguageModelStreamPart::TextDelta(
                        llm_kit_provider::language_model::stream_part::text_delta::LanguageModelStreamTextDelta::new(
                            index.to_string(),
                            text,
                        ),
                    ));
            }

            ContentBlockDelta::ThinkingDelta { thinking } => {
                self.pending_events.push(LanguageModelStreamPart::ReasoningDelta(
                    llm_kit_provider::language_model::stream_part::reasoning_delta::LanguageModelStreamReasoningDelta::new(
                        index.to_string(),
                        thinking,
                    ),
                ));
            }

            ContentBlockDelta::InputJsonDelta { partial_json } => {
                if partial_json.is_empty() {
                    // Skip empty deltas
                    return;
                }

                if let Some(ContentBlockState::ToolCall {
                    tool_call_id,
                    tool_name,
                    input,
                    first_delta,
                    ..
                }) = self.content_blocks.get_mut(&index)
                {
                    let mut delta = partial_json;

                    // For code execution tools, prepend type to first delta
                    if *first_delta
                        && (tool_name == "bash_code_execution"
                            || tool_name == "text_editor_code_execution")
                    {
                        delta = format!(r#"{{"type": "{}",{}"#, tool_name, &delta[1..]);
                    }

                    if self.uses_json_response_tool {
                        // Emit as text delta
                        self.pending_events.push(LanguageModelStreamPart::TextDelta(
                            llm_kit_provider::language_model::stream_part::text_delta::LanguageModelStreamTextDelta::new(
                                index.to_string(),
                                delta.clone(),
                            ),
                        ));
                    } else {
                        // Emit as tool input delta
                        self.pending_events.push(LanguageModelStreamPart::ToolInputDelta(
                            llm_kit_provider::language_model::stream_part::tool_input_delta::LanguageModelStreamToolInputDelta::new(
                                tool_call_id.clone(),
                                delta.clone(),
                            ),
                        ));
                    }

                    input.push_str(&delta);
                    *first_delta = false;
                }
            }
        }
    }

    /// Process content block stop events
    fn process_content_block_stop(&mut self, index: u32) {
        if let Some(block_state) = self.content_blocks.remove(&index) {
            match block_state {
                ContentBlockState::Text => {
                    self.pending_events
                        .push(LanguageModelStreamPart::TextEnd(
                            llm_kit_provider::language_model::stream_part::text_end::LanguageModelStreamTextEnd::new(
                                index.to_string(),
                            ),
                        ));
                }

                ContentBlockState::Reasoning => {
                    self.pending_events.push(LanguageModelStreamPart::ReasoningEnd(
                        llm_kit_provider::language_model::stream_part::reasoning_end::LanguageModelStreamReasoningEnd::new(
                            index.to_string(),
                        ),
                    ));
                }

                ContentBlockState::ToolCall {
                    tool_call_id,
                    tool_name,
                    input,
                    provider_executed,
                    ..
                } => {
                    let is_json_response_tool = self.uses_json_response_tool && tool_name == "json";

                    if !is_json_response_tool {
                        // Emit tool input end
                        self.pending_events.push(LanguageModelStreamPart::ToolInputEnd(
                            llm_kit_provider::language_model::stream_part::tool_input_end::LanguageModelStreamToolInputEnd::new(
                                tool_call_id.clone(),
                            ),
                        ));

                        // Map tool name for code execution tools
                        let mapped_tool_name = if tool_name == "text_editor_code_execution"
                            || tool_name == "bash_code_execution"
                        {
                            "code_execution".to_string()
                        } else {
                            tool_name
                        };

                        // Emit complete tool call
                        self.pending_events.push(LanguageModelStreamPart::ToolCall(
                            llm_kit_provider::language_model::content::tool_call::LanguageModelToolCall::with_options(
                                tool_call_id,
                                mapped_tool_name,
                                input,
                                Some(provider_executed),
                                None,
                            ),
                        ));
                    }
                }
            }
        }

        self.current_block_type = None;
    }

    // Helper methods for emitting tool results
    fn emit_web_fetch_result(
        &mut self,
        tool_use_id: String,
        content: super::response_schema::WebFetchContent,
    ) {
        use super::response_schema::WebFetchContent;

        let (result, is_error) = match content {
            WebFetchContent::Result {
                r#type,
                url,
                retrieved_at,
                content,
            } => (
                serde_json::json!({
                    "type": r#type,
                    "url": url,
                    "retrievedAt": retrieved_at,
                    "content": content
                }),
                false,
            ),
            WebFetchContent::Error { r#type, error_code } => (
                serde_json::json!({
                    "type": r#type,
                    "errorCode": error_code
                }),
                true,
            ),
        };

        self.pending_events.push(LanguageModelStreamPart::ToolResult(
            llm_kit_provider::language_model::content::tool_result::LanguageModelToolResult::with_options(
                tool_use_id,
                "web_fetch",
                result,
                Some(is_error),
                None,
                None,
            ),
        ));
    }

    fn emit_web_search_result(
        &mut self,
        tool_use_id: String,
        content: super::response_schema::WebSearchContent,
    ) {
        use super::response_schema::WebSearchContent;
        use llm_kit_provider::language_model::content::source::LanguageModelSource;

        match content {
            WebSearchContent::Results(results) => {
                // Emit tool result
                let result_value = serde_json::to_value(&results).unwrap_or(serde_json::json!([]));
                self.pending_events.push(LanguageModelStreamPart::ToolResult(
                    llm_kit_provider::language_model::content::tool_result::LanguageModelToolResult::with_options(
                        tool_use_id,
                        "web_search",
                        result_value,
                        Some(false),
                        None,
                        None,
                    ),
                ));

                // Emit source events for each result
                for result in results {
                    let id = uuid::Uuid::new_v4().to_string();
                    let mut anthropic_meta = HashMap::new();
                    if let Some(page_age) = result.page_age {
                        anthropic_meta.insert("pageAge".to_string(), serde_json::json!(page_age));
                    }

                    let provider_metadata = if !anthropic_meta.is_empty() {
                        let mut metadata = HashMap::new();
                        metadata.insert("anthropic".to_string(), anthropic_meta);
                        Some(metadata)
                    } else {
                        None
                    };

                    self.pending_events.push(LanguageModelStreamPart::Source(
                        LanguageModelSource::Url {
                            id,
                            url: result.url,
                            title: Some(result.title),
                            provider_metadata,
                        },
                    ));
                }
            }
            WebSearchContent::Error { error_code, .. } => {
                self.pending_events.push(LanguageModelStreamPart::ToolResult(
                    llm_kit_provider::language_model::content::tool_result::LanguageModelToolResult::with_options(
                        tool_use_id,
                        "web_search",
                        serde_json::json!({
                            "type": "web_search_tool_result_error",
                            "errorCode": error_code
                        }),
                        Some(true),
                        None,
                        None,
                    ),
                ));
            }
        }
    }

    fn emit_code_execution_result(
        &mut self,
        tool_use_id: String,
        content: super::response_schema::CodeExecutionContent,
    ) {
        use super::response_schema::CodeExecutionContent;

        let (result, is_error) = match content {
            CodeExecutionContent::Result {
                stdout,
                stderr,
                return_code,
                ..
            } => (
                serde_json::json!({
                    "stdout": stdout,
                    "stderr": stderr,
                    "return_code": return_code
                }),
                return_code != 0,
            ),
            CodeExecutionContent::Error { error_code, .. } => (
                serde_json::json!({
                    "type": "code_execution_tool_result_error",
                    "errorCode": error_code
                }),
                true,
            ),
        };

        self.pending_events.push(LanguageModelStreamPart::ToolResult(
            llm_kit_provider::language_model::content::tool_result::LanguageModelToolResult::with_options(
                tool_use_id,
                "code_execution",
                result,
                Some(is_error),
                None,
                None,
            ),
        ));
    }

    fn emit_bash_code_execution_result(
        &mut self,
        tool_use_id: String,
        content: super::response_schema::BashCodeExecutionContent,
    ) {
        let result = serde_json::to_value(&content).unwrap_or(serde_json::json!({}));

        self.pending_events.push(LanguageModelStreamPart::ToolResult(
            llm_kit_provider::language_model::content::tool_result::LanguageModelToolResult::with_options(
                tool_use_id,
                "code_execution",
                result,
                Some(false),
                None,
                None,
            ),
        ));
    }

    fn emit_text_editor_code_execution_result(
        &mut self,
        tool_use_id: String,
        content: super::response_schema::TextEditorCodeExecutionContent,
    ) {
        let result = serde_json::to_value(&content).unwrap_or(serde_json::json!({}));

        self.pending_events.push(LanguageModelStreamPart::ToolResult(
            llm_kit_provider::language_model::content::tool_result::LanguageModelToolResult::with_options(
                tool_use_id,
                "text_editor_code_execution",
                result,
                Some(false),
                None,
                None,
            ),
        ));
    }

    fn build_provider_metadata(
        &self,
    ) -> Option<llm_kit_provider::shared::provider_metadata::SharedProviderMetadata> {
        if self.cache_creation_input_tokens.is_some()
            || self.stop_sequence.is_some()
            || self.container.is_some()
        {
            let mut metadata: llm_kit_provider::shared::provider_metadata::SharedProviderMetadata =
                HashMap::new();
            let mut anthropic_metadata: HashMap<String, serde_json::Value> = HashMap::new();

            if let Some(cache_tokens) = self.cache_creation_input_tokens {
                anthropic_metadata.insert(
                    "cacheCreationInputTokens".to_string(),
                    serde_json::json!(cache_tokens),
                );
            }

            if let Some(stop_seq) = &self.stop_sequence {
                anthropic_metadata.insert("stopSequence".to_string(), serde_json::json!(stop_seq));
            }

            if let Some(container) = &self.container {
                anthropic_metadata.insert("container".to_string(), container.clone());
            }

            metadata.insert("anthropic".to_string(), anthropic_metadata);
            Some(metadata)
        } else {
            None
        }
    }
}

impl Stream for AnthropicStreamParser {
    type Item = LanguageModelStreamPart;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // Emit stream start event first
        if !self.stream_started {
            self.stream_started = true;
            let warnings = std::mem::take(&mut self.warnings);
            return Poll::Ready(Some(LanguageModelStreamPart::StreamStart(
                llm_kit_provider::language_model::stream_part::stream_start::LanguageModelStreamStart::new(
                    warnings,
                ),
            )));
        }

        // Return pending events first
        if !self.pending_events.is_empty() {
            return Poll::Ready(Some(self.pending_events.remove(0)));
        }

        // Poll for more bytes
        match self.byte_stream.as_mut().poll_next(cx) {
            Poll::Ready(Some(Ok(bytes))) => {
                // Add bytes to buffer
                if let Ok(text) = std::str::from_utf8(&bytes) {
                    self.buffer.push_str(text);
                }

                // Parse SSE events from buffer
                self.parse_sse_events();

                // Return next pending event or continue polling
                if !self.pending_events.is_empty() {
                    Poll::Ready(Some(self.pending_events.remove(0)))
                } else {
                    // Continue polling
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
            }
            Poll::Ready(Some(Err(e))) => {
                // Emit error
                Poll::Ready(Some(LanguageModelStreamPart::Error(
                    llm_kit_provider::language_model::stream_part::error::LanguageModelStreamError::new(
                        serde_json::json!(e.to_string()),
                    ),
                )))
            }
            Poll::Ready(None) => {
                // Stream ended
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

impl AnthropicStreamParser {
    /// Parse SSE events from the buffer
    fn parse_sse_events(&mut self) {
        // SSE format: "data: {json}\n\n"
        while let Some(pos) = self.buffer.find("\n\n") {
            let event_str = self.buffer.drain(..pos + 2).collect::<String>();

            // Extract data line
            for line in event_str.lines() {
                if let Some(data) = line.strip_prefix("data: ") {
                    // Parse JSON
                    if let Ok(chunk) = serde_json::from_str::<AnthropicChunk>(data) {
                        self.process_chunk(chunk);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;
    use llm_kit_provider::language_model::finish_reason::LanguageModelFinishReason;
    use llm_kit_provider::language_model::stream_part::LanguageModelStreamPart;

    /// Create SSE-formatted data for a single event.
    fn sse(json: &str) -> String {
        format!("data: {}\n\n", json)
    }

    /// Collect all stream parts from SSE data.
    async fn collect(sse_data: &str, json_response_tool: bool) -> Vec<LanguageModelStreamPart> {
        let bytes = Bytes::from(sse_data.to_string());
        let stream = futures_util::stream::iter(vec![Ok::<_, reqwest::Error>(bytes)]);
        let mut parser = parse_sse_stream(stream, json_response_tool, vec![], false);
        let mut parts = vec![];
        while let Some(part) = parser.next().await {
            parts.push(part);
        }
        parts
    }

    /// Build a full text streaming SSE sequence.
    fn text_stream_sse(text_deltas: &[&str]) -> String {
        let mut data = String::new();
        data.push_str(&sse(
            r#"{"type":"message_start","message":{"id":"msg_1","model":"claude-3","usage":{"input_tokens":10}}}"#,
        ));
        data.push_str(&sse(
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#,
        ));
        for delta in text_deltas {
            data.push_str(&sse(&format!(
                r#"{{"type":"content_block_delta","index":0,"delta":{{"type":"text_delta","text":"{}"}}}}"#,
                delta
            )));
        }
        data.push_str(&sse(r#"{"type":"content_block_stop","index":0}"#));
        data.push_str(&sse(
            r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":5}}"#,
        ));
        data.push_str(&sse(r#"{"type":"message_stop"}"#));
        data
    }

    #[tokio::test]
    async fn test_stream_start_emitted_first() {
        let parts = collect(&sse(r#"{"type":"ping"}"#), false).await;
        assert!(!parts.is_empty());
        assert!(matches!(&parts[0], LanguageModelStreamPart::StreamStart(_)));
    }

    #[tokio::test]
    async fn test_ping_produces_no_content_events() {
        let parts = collect(&sse(r#"{"type":"ping"}"#), false).await;
        // Only StreamStart — ping is silently consumed
        assert_eq!(parts.len(), 1);
    }

    #[tokio::test]
    async fn test_message_start_emits_response_metadata() {
        let data = sse(
            r#"{"type":"message_start","message":{"id":"msg_abc","model":"claude-3-5-sonnet","usage":{"input_tokens":42,"cache_read_input_tokens":10}}}"#,
        );
        let parts = collect(&data, false).await;

        let meta = parts
            .iter()
            .find(|p| matches!(p, LanguageModelStreamPart::ResponseMetadata(_)));
        assert!(meta.is_some(), "Expected ResponseMetadata event");

        if let LanguageModelStreamPart::ResponseMetadata(m) = meta.unwrap() {
            assert_eq!(m.id, Some("msg_abc".to_string()));
            assert_eq!(m.model_id, Some("claude-3-5-sonnet".to_string()));
        }
    }

    #[tokio::test]
    async fn test_text_streaming_full_lifecycle() {
        let data = text_stream_sse(&["Hello", " world"]);
        let parts = collect(&data, false).await;

        let text_starts: Vec<_> = parts
            .iter()
            .filter(|p| matches!(p, LanguageModelStreamPart::TextStart(_)))
            .collect();
        let text_deltas: Vec<_> = parts
            .iter()
            .filter(|p| matches!(p, LanguageModelStreamPart::TextDelta(_)))
            .collect();
        let text_ends: Vec<_> = parts
            .iter()
            .filter(|p| matches!(p, LanguageModelStreamPart::TextEnd(_)))
            .collect();
        let finishes: Vec<_> = parts
            .iter()
            .filter(|p| matches!(p, LanguageModelStreamPart::Finish(_)))
            .collect();

        assert_eq!(text_starts.len(), 1);
        assert_eq!(text_deltas.len(), 2);
        assert_eq!(text_ends.len(), 1);
        assert_eq!(finishes.len(), 1);

        if let LanguageModelStreamPart::TextDelta(td) = text_deltas[0] {
            assert_eq!(td.delta, "Hello");
        } else {
            panic!("Expected TextDelta");
        }

        if let LanguageModelStreamPart::TextDelta(td) = text_deltas[1] {
            assert_eq!(td.delta, " world");
        } else {
            panic!("Expected TextDelta");
        }
    }

    #[tokio::test]
    async fn test_finish_contains_usage_and_reason() {
        let data = text_stream_sse(&["Hi"]);
        let parts = collect(&data, false).await;

        let finish = parts
            .iter()
            .find(|p| matches!(p, LanguageModelStreamPart::Finish(_)));
        assert!(finish.is_some());

        if let LanguageModelStreamPart::Finish(f) = finish.unwrap() {
            assert_eq!(f.usage.input_tokens, 10);
            assert_eq!(f.usage.output_tokens, 5);
            assert!(matches!(f.finish_reason, LanguageModelFinishReason::Stop));
        }
    }

    #[tokio::test]
    async fn test_reasoning_streaming() {
        let mut data = String::new();
        data.push_str(&sse(
            r#"{"type":"message_start","message":{"usage":{"input_tokens":5}}}"#,
        ));
        data.push_str(&sse(
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}"#,
        ));
        data.push_str(&sse(
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"Let me think"}}"#,
        ));
        data.push_str(&sse(r#"{"type":"content_block_stop","index":0}"#));
        data.push_str(&sse(
            r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":3}}"#,
        ));
        data.push_str(&sse(r#"{"type":"message_stop"}"#));

        let parts = collect(&data, false).await;

        let reasoning_starts: Vec<_> = parts
            .iter()
            .filter(|p| matches!(p, LanguageModelStreamPart::ReasoningStart(_)))
            .collect();
        let reasoning_deltas: Vec<_> = parts
            .iter()
            .filter(|p| matches!(p, LanguageModelStreamPart::ReasoningDelta(_)))
            .collect();
        let reasoning_ends: Vec<_> = parts
            .iter()
            .filter(|p| matches!(p, LanguageModelStreamPart::ReasoningEnd(_)))
            .collect();

        assert_eq!(reasoning_starts.len(), 1);
        assert_eq!(reasoning_deltas.len(), 1);
        assert_eq!(reasoning_ends.len(), 1);

        if let LanguageModelStreamPart::ReasoningDelta(rd) = reasoning_deltas[0] {
            assert_eq!(rd.delta, "Let me think");
        }
    }

    #[tokio::test]
    async fn test_redacted_thinking() {
        let mut data = String::new();
        data.push_str(&sse(
            r#"{"type":"message_start","message":{"usage":{"input_tokens":5}}}"#,
        ));
        data.push_str(&sse(
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"redacted_thinking","data":"abc123"}}"#,
        ));
        data.push_str(&sse(r#"{"type":"content_block_stop","index":0}"#));
        data.push_str(&sse(
            r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":0}}"#,
        ));
        data.push_str(&sse(r#"{"type":"message_stop"}"#));

        let parts = collect(&data, false).await;

        let reasoning_starts: Vec<_> = parts
            .iter()
            .filter(|p| matches!(p, LanguageModelStreamPart::ReasoningStart(_)))
            .collect();
        assert_eq!(reasoning_starts.len(), 1);

        if let LanguageModelStreamPart::ReasoningStart(rs) = reasoning_starts[0] {
            // Redacted thinking should carry metadata with redactedData
            assert!(rs.provider_metadata.is_some());
            let meta = rs.provider_metadata.as_ref().unwrap();
            let anthropic = meta.get("anthropic").expect("anthropic key");
            assert!(anthropic.contains_key("redactedData"));
        }
    }

    #[tokio::test]
    async fn test_tool_call_streaming() {
        let mut data = String::new();
        data.push_str(&sse(
            r#"{"type":"message_start","message":{"usage":{"input_tokens":5}}}"#,
        ));
        data.push_str(&sse(
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"toolu_1","name":"get_weather"}}"#,
        ));
        data.push_str(&sse(
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"city\":"}}"#,
        ));
        data.push_str(&sse(
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"\"SF\"}"}}"#,
        ));
        data.push_str(&sse(r#"{"type":"content_block_stop","index":0}"#));
        data.push_str(&sse(
            r#"{"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":10}}"#,
        ));
        data.push_str(&sse(r#"{"type":"message_stop"}"#));

        let parts = collect(&data, false).await;

        // Expect ToolInputStart, ToolInputDelta(s), ToolInputEnd, ToolCall
        let tool_starts: Vec<_> = parts
            .iter()
            .filter(|p| matches!(p, LanguageModelStreamPart::ToolInputStart(_)))
            .collect();
        let tool_deltas: Vec<_> = parts
            .iter()
            .filter(|p| matches!(p, LanguageModelStreamPart::ToolInputDelta(_)))
            .collect();
        let tool_ends: Vec<_> = parts
            .iter()
            .filter(|p| matches!(p, LanguageModelStreamPart::ToolInputEnd(_)))
            .collect();
        let tool_calls: Vec<_> = parts
            .iter()
            .filter(|p| matches!(p, LanguageModelStreamPart::ToolCall(_)))
            .collect();

        assert_eq!(tool_starts.len(), 1);
        assert_eq!(tool_deltas.len(), 2);
        assert_eq!(tool_ends.len(), 1);
        assert_eq!(tool_calls.len(), 1);

        if let LanguageModelStreamPart::ToolInputStart(ts) = tool_starts[0] {
            assert_eq!(ts.id, "toolu_1");
            assert_eq!(ts.tool_name, "get_weather");
            assert!(ts.provider_executed.is_none() || ts.provider_executed == Some(false));
        }

        if let LanguageModelStreamPart::ToolCall(tc) = tool_calls[0] {
            assert_eq!(tc.tool_call_id, "toolu_1");
            assert_eq!(tc.tool_name, "get_weather");
            assert_eq!(tc.input, r#"{"city":"SF"}"#);
        }
    }

    #[tokio::test]
    async fn test_server_tool_code_execution() {
        let mut data = String::new();
        data.push_str(&sse(
            r#"{"type":"message_start","message":{"usage":{"input_tokens":5}}}"#,
        ));
        data.push_str(&sse(
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"server_tool_use","id":"toolu_ce","name":"code_execution"}}"#,
        ));
        data.push_str(&sse(
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"code\":\"print(1)\"}"}}"#,
        ));
        data.push_str(&sse(r#"{"type":"content_block_stop","index":0}"#));
        data.push_str(&sse(
            r#"{"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":8}}"#,
        ));
        data.push_str(&sse(r#"{"type":"message_stop"}"#));

        let parts = collect(&data, false).await;

        let tool_starts: Vec<_> = parts
            .iter()
            .filter(|p| matches!(p, LanguageModelStreamPart::ToolInputStart(_)))
            .collect();
        assert_eq!(tool_starts.len(), 1);

        if let LanguageModelStreamPart::ToolInputStart(ts) = tool_starts[0] {
            assert_eq!(ts.tool_name, "code_execution");
            assert_eq!(ts.provider_executed, Some(true));
        }

        let tool_calls: Vec<_> = parts
            .iter()
            .filter(|p| matches!(p, LanguageModelStreamPart::ToolCall(_)))
            .collect();
        assert_eq!(tool_calls.len(), 1);

        if let LanguageModelStreamPart::ToolCall(tc) = tool_calls[0] {
            assert_eq!(tc.tool_name, "code_execution");
            assert_eq!(tc.provider_executed, Some(true));
        }
    }

    #[tokio::test]
    async fn test_bash_code_execution_name_mapped() {
        let mut data = String::new();
        data.push_str(&sse(
            r#"{"type":"message_start","message":{"usage":{"input_tokens":5}}}"#,
        ));
        data.push_str(&sse(
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"server_tool_use","id":"toolu_bash","name":"bash_code_execution"}}"#,
        ));
        data.push_str(&sse(
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"cmd\":\"ls\"}"}}"#,
        ));
        data.push_str(&sse(r#"{"type":"content_block_stop","index":0}"#));
        data.push_str(&sse(
            r#"{"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":5}}"#,
        ));
        data.push_str(&sse(r#"{"type":"message_stop"}"#));

        let parts = collect(&data, false).await;

        // bash_code_execution should be mapped to code_execution in ToolInputStart
        let tool_starts: Vec<_> = parts
            .iter()
            .filter(|p| matches!(p, LanguageModelStreamPart::ToolInputStart(_)))
            .collect();
        if let LanguageModelStreamPart::ToolInputStart(ts) = tool_starts[0] {
            assert_eq!(ts.tool_name, "code_execution");
        }

        // ToolCall should also have mapped name
        let tool_calls: Vec<_> = parts
            .iter()
            .filter(|p| matches!(p, LanguageModelStreamPart::ToolCall(_)))
            .collect();
        if let LanguageModelStreamPart::ToolCall(tc) = tool_calls[0] {
            assert_eq!(tc.tool_name, "code_execution");
        }
    }

    #[tokio::test]
    async fn test_mcp_tool_use() {
        let mut data = String::new();
        data.push_str(&sse(
            r#"{"type":"message_start","message":{"usage":{"input_tokens":5}}}"#,
        ));
        data.push_str(&sse(
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"mcp_tool_use","id":"mcp_1","name":"search","input":{"query":"test"},"server_name":"my-server"}}"#,
        ));
        data.push_str(&sse(r#"{"type":"content_block_stop","index":0}"#));
        data.push_str(&sse(
            r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":3}}"#,
        ));
        data.push_str(&sse(r#"{"type":"message_stop"}"#));

        let parts = collect(&data, false).await;

        let tool_calls: Vec<_> = parts
            .iter()
            .filter(|p| matches!(p, LanguageModelStreamPart::ToolCall(_)))
            .collect();
        assert_eq!(tool_calls.len(), 1);

        if let LanguageModelStreamPart::ToolCall(tc) = tool_calls[0] {
            assert_eq!(tc.tool_name, "search");
            assert_eq!(tc.provider_executed, Some(true));
            // Check provider metadata contains MCP info
            let meta = tc.provider_metadata.as_ref().expect("metadata");
            let anthropic = meta.get("anthropic").expect("anthropic key");
            assert_eq!(anthropic.get("type").unwrap(), "mcp-tool-use");
            assert_eq!(anthropic.get("serverName").unwrap(), "my-server");
        }
    }

    #[tokio::test]
    async fn test_mcp_tool_result() {
        let mut data = String::new();
        data.push_str(&sse(
            r#"{"type":"message_start","message":{"usage":{"input_tokens":5}}}"#,
        ));
        // First, the MCP tool use so we track the tool call state
        data.push_str(&sse(
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"mcp_tool_use","id":"mcp_1","name":"search","input":{"q":"test"},"server_name":"srv"}}"#,
        ));
        data.push_str(&sse(r#"{"type":"content_block_stop","index":0}"#));
        // Then the result
        data.push_str(&sse(
            r#"{"type":"content_block_start","index":1,"content_block":{"type":"mcp_tool_result","tool_use_id":"mcp_1","is_error":false,"content":[{"type":"text","text":"found it"}]}}"#,
        ));
        data.push_str(&sse(r#"{"type":"content_block_stop","index":1}"#));
        data.push_str(&sse(
            r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":3}}"#,
        ));
        data.push_str(&sse(r#"{"type":"message_stop"}"#));

        let parts = collect(&data, false).await;

        let tool_results: Vec<_> = parts
            .iter()
            .filter(|p| matches!(p, LanguageModelStreamPart::ToolResult(_)))
            .collect();
        assert_eq!(tool_results.len(), 1);

        if let LanguageModelStreamPart::ToolResult(tr) = tool_results[0] {
            assert_eq!(tr.tool_call_id, "mcp_1");
            assert_eq!(tr.tool_name, "search");
            assert_eq!(tr.is_error, Some(false));
        }
    }

    #[tokio::test]
    async fn test_json_response_tool_emits_text() {
        // When uses_json_response_tool=true, tool_use named "json" should emit TextStart/TextDelta
        let mut data = String::new();
        data.push_str(&sse(
            r#"{"type":"message_start","message":{"usage":{"input_tokens":5}}}"#,
        ));
        data.push_str(&sse(
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"toolu_json","name":"json"}}"#,
        ));
        data.push_str(&sse(
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"key\":\"value\"}"}}"#,
        ));
        data.push_str(&sse(r#"{"type":"content_block_stop","index":0}"#));
        data.push_str(&sse(
            r#"{"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":5}}"#,
        ));
        data.push_str(&sse(r#"{"type":"message_stop"}"#));

        let parts = collect(&data, true).await;

        // When uses_json_response_tool=true and tool name is "json":
        // - ContentBlockStart emits TextStart (treats tool_use as text block)
        // - InputJsonDelta is dropped (state is Text, handler checks for ToolCall)
        // - ContentBlockStop emits TextEnd
        // - No ToolInputStart or ToolCall events
        let text_starts: Vec<_> = parts
            .iter()
            .filter(|p| matches!(p, LanguageModelStreamPart::TextStart(_)))
            .collect();
        let tool_starts: Vec<_> = parts
            .iter()
            .filter(|p| matches!(p, LanguageModelStreamPart::ToolInputStart(_)))
            .collect();
        let tool_calls: Vec<_> = parts
            .iter()
            .filter(|p| matches!(p, LanguageModelStreamPart::ToolCall(_)))
            .collect();

        assert_eq!(text_starts.len(), 1);
        assert_eq!(tool_starts.len(), 0);
        assert_eq!(tool_calls.len(), 0);
    }

    #[tokio::test]
    async fn test_json_response_tool_ignores_text_blocks() {
        // When uses_json_response_tool=true, regular text blocks should be ignored
        let mut data = String::new();
        data.push_str(&sse(
            r#"{"type":"message_start","message":{"usage":{"input_tokens":5}}}"#,
        ));
        data.push_str(&sse(
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#,
        ));
        data.push_str(&sse(
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"ignored"}}"#,
        ));
        data.push_str(&sse(r#"{"type":"content_block_stop","index":0}"#));
        data.push_str(&sse(
            r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":1}}"#,
        ));
        data.push_str(&sse(r#"{"type":"message_stop"}"#));

        let parts = collect(&data, true).await;

        let text_starts: Vec<_> = parts
            .iter()
            .filter(|p| matches!(p, LanguageModelStreamPart::TextStart(_)))
            .collect();
        let text_deltas: Vec<_> = parts
            .iter()
            .filter(|p| matches!(p, LanguageModelStreamPart::TextDelta(_)))
            .collect();

        assert_eq!(text_starts.len(), 0);
        assert_eq!(text_deltas.len(), 0);
    }

    #[tokio::test]
    async fn test_error_event() {
        let data = sse(
            r#"{"type":"error","error":{"type":"rate_limit_error","message":"Rate limit exceeded"}}"#,
        );
        let parts = collect(&data, false).await;

        let errors: Vec<_> = parts
            .iter()
            .filter(|p| matches!(p, LanguageModelStreamPart::Error(_)))
            .collect();
        assert_eq!(errors.len(), 1);

        if let LanguageModelStreamPart::Error(e) = errors[0] {
            let msg = e.error.as_str().unwrap_or("");
            assert!(msg.contains("Rate limit exceeded"));
        }
    }

    #[tokio::test]
    async fn test_empty_input_json_delta_skipped() {
        let mut data = String::new();
        data.push_str(&sse(
            r#"{"type":"message_start","message":{"usage":{"input_tokens":5}}}"#,
        ));
        data.push_str(&sse(
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"t1","name":"tool"}}"#,
        ));
        // Empty partial_json should be skipped
        data.push_str(&sse(
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":""}}"#,
        ));
        data.push_str(&sse(
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{}"}}"#,
        ));
        data.push_str(&sse(r#"{"type":"content_block_stop","index":0}"#));
        data.push_str(&sse(
            r#"{"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":3}}"#,
        ));
        data.push_str(&sse(r#"{"type":"message_stop"}"#));

        let parts = collect(&data, false).await;

        let tool_deltas: Vec<_> = parts
            .iter()
            .filter(|p| matches!(p, LanguageModelStreamPart::ToolInputDelta(_)))
            .collect();
        // Only 1 non-empty delta
        assert_eq!(tool_deltas.len(), 1);
    }

    #[tokio::test]
    async fn test_initial_warnings_in_stream_start() {
        let bytes = Bytes::from(sse(r#"{"type":"ping"}"#));
        let stream = futures_util::stream::iter(vec![Ok::<_, reqwest::Error>(bytes)]);

        let warnings = vec![LanguageModelCallWarning::other("test warning".to_string())];
        let mut parser = parse_sse_stream(stream, false, warnings, false);
        let first = parser.next().await.expect("should have stream start");

        if let LanguageModelStreamPart::StreamStart(ss) = first {
            assert_eq!(ss.warnings.len(), 1);
        } else {
            panic!("Expected StreamStart");
        }
    }

    #[tokio::test]
    async fn test_multiple_content_blocks() {
        let mut data = String::new();
        data.push_str(&sse(
            r#"{"type":"message_start","message":{"usage":{"input_tokens":5}}}"#,
        ));
        // Text block
        data.push_str(&sse(
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#,
        ));
        data.push_str(&sse(
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"I'll check"}}"#,
        ));
        data.push_str(&sse(r#"{"type":"content_block_stop","index":0}"#));
        // Tool block
        data.push_str(&sse(
            r#"{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"t1","name":"search"}}"#,
        ));
        data.push_str(&sse(
            r#"{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{}"}}"#,
        ));
        data.push_str(&sse(r#"{"type":"content_block_stop","index":1}"#));
        data.push_str(&sse(
            r#"{"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":8}}"#,
        ));
        data.push_str(&sse(r#"{"type":"message_stop"}"#));

        let parts = collect(&data, false).await;

        let text_starts = parts
            .iter()
            .filter(|p| matches!(p, LanguageModelStreamPart::TextStart(_)))
            .count();
        let tool_starts = parts
            .iter()
            .filter(|p| matches!(p, LanguageModelStreamPart::ToolInputStart(_)))
            .count();

        assert_eq!(text_starts, 1);
        assert_eq!(tool_starts, 1);
    }

    #[tokio::test]
    async fn test_stop_reason_tool_use() {
        let mut data = String::new();
        data.push_str(&sse(
            r#"{"type":"message_start","message":{"usage":{"input_tokens":5}}}"#,
        ));
        data.push_str(&sse(
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"t1","name":"fn"}}"#,
        ));
        data.push_str(&sse(
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{}"}}"#,
        ));
        data.push_str(&sse(r#"{"type":"content_block_stop","index":0}"#));
        data.push_str(&sse(
            r#"{"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":5}}"#,
        ));
        data.push_str(&sse(r#"{"type":"message_stop"}"#));

        let parts = collect(&data, false).await;

        let finish = parts
            .iter()
            .find(|p| matches!(p, LanguageModelStreamPart::Finish(_)));
        if let Some(LanguageModelStreamPart::Finish(f)) = finish {
            assert!(matches!(
                f.finish_reason,
                LanguageModelFinishReason::ToolCalls
            ));
        }
    }

    #[tokio::test]
    async fn test_cache_creation_tokens_in_provider_metadata() {
        let mut data = String::new();
        data.push_str(&sse(
            r#"{"type":"message_start","message":{"usage":{"input_tokens":10,"cache_creation_input_tokens":50}}}"#,
        ));
        data.push_str(&sse(
            r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":1}}"#,
        ));
        data.push_str(&sse(r#"{"type":"message_stop"}"#));

        let parts = collect(&data, false).await;

        let finish = parts
            .iter()
            .find(|p| matches!(p, LanguageModelStreamPart::Finish(_)));
        if let Some(LanguageModelStreamPart::Finish(f)) = finish {
            let meta = f.provider_metadata.as_ref().expect("provider metadata");
            let anthropic = meta.get("anthropic").expect("anthropic");
            assert_eq!(anthropic.get("cacheCreationInputTokens").unwrap(), 50);
        }
    }

    #[tokio::test]
    async fn test_web_fetch_tool_result() {
        let mut data = String::new();
        data.push_str(&sse(
            r#"{"type":"message_start","message":{"usage":{"input_tokens":5}}}"#,
        ));
        // WebFetchContent::Result requires nested WebFetchDocument with WebFetchSource
        let content_block = serde_json::json!({
            "type": "web_fetch_tool_result",
            "tool_use_id": "wf_1",
            "content": {
                "type": "web_fetch_result",
                "url": "https://example.com",
                "retrieved_at": "2024-01-01",
                "content": {
                    "type": "document",
                    "source": {
                        "type": "text",
                        "media_type": "text/html",
                        "data": "page content"
                    }
                }
            }
        });
        data.push_str(&sse(&format!(
            r#"{{"type":"content_block_start","index":0,"content_block":{}}}"#,
            content_block
        )));
        data.push_str(&sse(r#"{"type":"content_block_stop","index":0}"#));
        data.push_str(&sse(
            r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":3}}"#,
        ));
        data.push_str(&sse(r#"{"type":"message_stop"}"#));

        let parts = collect(&data, false).await;

        let tool_results: Vec<_> = parts
            .iter()
            .filter(|p| matches!(p, LanguageModelStreamPart::ToolResult(_)))
            .collect();
        assert_eq!(tool_results.len(), 1);

        if let LanguageModelStreamPart::ToolResult(tr) = tool_results[0] {
            assert_eq!(tr.tool_call_id, "wf_1");
            assert_eq!(tr.tool_name, "web_fetch");
            assert_eq!(tr.is_error, Some(false));
        }
    }

    #[tokio::test]
    async fn test_code_execution_tool_result() {
        let mut data = String::new();
        data.push_str(&sse(
            r#"{"type":"message_start","message":{"usage":{"input_tokens":5}}}"#,
        ));
        data.push_str(&sse(
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"code_execution_tool_result","tool_use_id":"ce_1","content":{"type":"code_execution_tool_result","stdout":"hello\n","stderr":"","return_code":0}}}"#,
        ));
        data.push_str(&sse(r#"{"type":"content_block_stop","index":0}"#));
        data.push_str(&sse(
            r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":1}}"#,
        ));
        data.push_str(&sse(r#"{"type":"message_stop"}"#));

        let parts = collect(&data, false).await;

        let tool_results: Vec<_> = parts
            .iter()
            .filter(|p| matches!(p, LanguageModelStreamPart::ToolResult(_)))
            .collect();
        assert_eq!(tool_results.len(), 1);

        if let LanguageModelStreamPart::ToolResult(tr) = tool_results[0] {
            assert_eq!(tr.tool_call_id, "ce_1");
            assert_eq!(tr.tool_name, "code_execution");
            assert_eq!(tr.is_error, Some(false));
        }
    }

    #[tokio::test]
    async fn test_code_execution_error_result() {
        let mut data = String::new();
        data.push_str(&sse(
            r#"{"type":"message_start","message":{"usage":{"input_tokens":5}}}"#,
        ));
        data.push_str(&sse(
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"code_execution_tool_result","tool_use_id":"ce_2","content":{"type":"code_execution_tool_result_error","error_code":"timeout"}}}"#,
        ));
        data.push_str(&sse(r#"{"type":"content_block_stop","index":0}"#));
        data.push_str(&sse(
            r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":1}}"#,
        ));
        data.push_str(&sse(r#"{"type":"message_stop"}"#));

        let parts = collect(&data, false).await;

        let tool_results: Vec<_> = parts
            .iter()
            .filter(|p| matches!(p, LanguageModelStreamPart::ToolResult(_)))
            .collect();
        assert_eq!(tool_results.len(), 1);

        if let LanguageModelStreamPart::ToolResult(tr) = tool_results[0] {
            assert_eq!(tr.is_error, Some(true));
        }
    }
}
