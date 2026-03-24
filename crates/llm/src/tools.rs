use std::path::Path;
use std::sync::Arc;

use llm_kit_core::ToolSet;
use llm_kit_provider_utils::tool::{Tool, ToolExecutionOutput};
use search::SearchResult;
use serde_json::{Value, json};
use tracing::debug;

use crate::SharedIndex;

#[cfg(test)]
#[path = "tools_test.rs"]
mod tools_test;

/// Build the file tools.
///
/// `search_files` and `search_by_type` use the shared index reference —
/// they gracefully return a "still loading" message when it's `None`.
/// `list_directory` and `file_info` work unconditionally (pure filesystem).
pub fn build_tools(index: SharedIndex) -> ToolSet {
    let mut tools = ToolSet::new();

    add_search_files(index.clone(), &mut tools);
    add_search_by_type(index, &mut tools);
    add_list_directory(&mut tools);
    add_file_info(&mut tools);

    tools
}

fn add_search_files(index: SharedIndex, tools: &mut ToolSet) {
    tools.insert(
        "search_files".to_string(),
        Tool::function(json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query (filename or path fragment)"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum results to return (default: 20, max: 200)"
                }
            },
            "required": ["query"]
        }))
        .with_description(
            "Search files by name or path across all indexed locations. \
             Returns matching file paths with size and modification time.",
        )
        .with_execute(Arc::new(move |input: Value, _opts| {
            let index = index.clone();
            ToolExecutionOutput::Single(Box::pin(async move {
                let guard = index.read().map_err(|e| json!({"error": e.to_string()}))?;
                let Some(mgr) = guard.as_ref() else {
                    debug!("search_files: index not yet available");
                    return Ok(json!({"result":
                        "Search index is still loading. Use list_directory to browse a specific folder."}));
                };
                let query = input.get("query").and_then(|v| v.as_str()).unwrap_or("");
                let limit = input.get("limit").and_then(Value::as_u64).unwrap_or(20).min(200) as usize;
                let results = mgr.search(query, limit);
                debug!(query, results = results.len(), "search_files");
                Ok(json!({"result": format_results(&results)}))
            }))
        })),
    );
}

fn add_search_by_type(index: SharedIndex, tools: &mut ToolSet) {
    tools.insert(
        "search_by_type".to_string(),
        Tool::function(json!({
            "type": "object",
            "properties": {
                "file_type": {
                    "type": "string",
                    "description": "File type category: images, videos, audio, documents, code, archives, fonts, applications, folders"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum results to return (default: 20, max: 200)"
                }
            },
            "required": ["file_type"]
        }))
        .with_description(
            "Search files by type category. Categories: images, videos, audio, \
             documents, code, archives, fonts, applications, folders.",
        )
        .with_execute(Arc::new(move |input: Value, _opts| {
            let index = index.clone();
            ToolExecutionOutput::Single(Box::pin(async move {
                let guard = index.read().map_err(|e| json!({"error": e.to_string()}))?;
                let Some(mgr) = guard.as_ref() else {
                    debug!("search_by_type: index not yet available");
                    return Ok(json!({"result":
                        "Search index is still loading. Use list_directory to browse a specific folder."}));
                };
                let file_type = input.get("file_type").and_then(|v| v.as_str()).unwrap_or("");
                let limit = input.get("limit").and_then(Value::as_u64).unwrap_or(20).min(200) as usize;
                let results = mgr.search_by_type(file_type, limit);
                debug!(file_type, results = results.len(), "search_by_type");
                Ok(json!({"result": format_results(&results)}))
            }))
        })),
    );
}

fn add_list_directory(tools: &mut ToolSet) {
    tools.insert(
        "list_directory".to_string(),
        Tool::function(json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Absolute path to the directory"
                }
            },
            "required": ["path"]
        }))
        .with_description(
            "List contents of a directory. Returns name, type, and size for each entry.",
        )
        .with_execute(Arc::new(move |input: Value, _opts| {
            ToolExecutionOutput::Single(Box::pin(async move {
                let path_str = input.get("path").and_then(|v| v.as_str()).unwrap_or("");
                let path = Path::new(path_str);

                if !path.is_absolute() {
                    return Err(json!({"error": "Path must be absolute"}));
                }

                let entries = match std::fs::read_dir(path) {
                    Ok(entries) => entries,
                    Err(e) => {
                        return Err(json!({"error": format!("Failed to read directory: {e}")}));
                    }
                };

                let mut lines = Vec::new();
                for entry in entries.flatten() {
                    let meta = entry.metadata().ok();
                    let name = entry.file_name().to_string_lossy().to_string();
                    let is_dir = meta.as_ref().is_some_and(std::fs::Metadata::is_dir);
                    let size = meta.as_ref().map_or(0, std::fs::Metadata::len);
                    let kind = if is_dir { "dir" } else { "file" };
                    lines.push(format!("{name}\t{kind}\t{}", format_size(size)));
                }

                lines.sort();
                let text = if lines.is_empty() {
                    "Directory is empty.".to_string()
                } else {
                    lines.join("\n")
                };
                Ok(json!({"result": text}))
            }))
        })),
    );
}

fn add_file_info(tools: &mut ToolSet) {
    tools.insert(
        "file_info".to_string(),
        Tool::function(json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Absolute path to the file"
                }
            },
            "required": ["path"]
        }))
        .with_description("Get metadata for a specific file: size, type, modification time.")
        .with_execute(Arc::new(move |input: Value, _opts| {
            ToolExecutionOutput::Single(Box::pin(async move {
                let path_str = input.get("path").and_then(|v| v.as_str()).unwrap_or("");
                let path = Path::new(path_str);

                let meta = match std::fs::metadata(path) {
                    Ok(m) => m,
                    Err(e) => {
                        return Err(json!({"error": format!("Failed to read file info: {e}")}));
                    }
                };

                let kind = if meta.is_dir() {
                    "directory"
                } else if meta.is_symlink() {
                    "symlink"
                } else {
                    "file"
                };

                let mtime = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_secs());

                let text = format!(
                    "path: {path_str}\ntype: {kind}\nsize: {}\nmodified: {}",
                    format_size(meta.len()),
                    format_time(mtime),
                );
                Ok(json!({"result": text}))
            }))
        })),
    );
}

// -- Format helpers -----------------------------------------------------------

fn format_results(results: &[SearchResult]) -> String {
    if results.is_empty() {
        return "No results found.".to_string();
    }
    results
        .iter()
        .map(|r| {
            let node = &r.node;
            format!(
                "{}\t{}\t{}",
                node.path,
                format_size(node.size),
                format_time(node.mtime),
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

fn format_time(unix_ts: u64) -> String {
    if unix_ts == 0 {
        return "unknown".to_string();
    }
    let secs = unix_ts;
    let days = secs / 86400;
    let remaining = secs % 86400;
    let hours = remaining / 3600;
    let mins = (remaining % 3600) / 60;

    let (year, month, day) = days_to_ymd(days);
    format!("{year:04}-{month:02}-{day:02} {hours:02}:{mins:02}")
}

fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    // Algorithm from http://howardhinnant.github.io/date_algorithms.html
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}
