use std::path::Path;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, Content, ServerCapabilities, ServerInfo};
use rmcp::{ServerHandler, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;

use zero::prelude::{IndexManager, SearchResult};

// -- MCP Server --------------------------------------------------------------

#[derive(Clone)]
pub(crate) struct ZeroMcpServer {
    index: std::sync::Arc<IndexManager>,
    tool_router: ToolRouter<Self>,
}

impl ZeroMcpServer {
    pub fn new(manager: IndexManager) -> Self {
        Self {
            index: std::sync::Arc::new(manager),
            tool_router: Self::tool_router(),
        }
    }

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
}

fn format_size(bytes: u64) -> String {
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
    // Simple ISO-ish format without pulling in chrono
    let secs = unix_ts;
    let days = secs / 86400;
    let remaining = secs % 86400;
    let hours = remaining / 3600;
    let mins = (remaining % 3600) / 60;

    // Days since Unix epoch → approximate date
    // Good enough for display; exact calendar math not critical here
    let (year, month, day) = days_to_ymd(days);
    format!("{year:04}-{month:02}-{day:02} {hours:02}:{mins:02}")
}

fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    // Algorithm from http://howardhinnant.github.io/date_algorithms.html
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

// -- Tool parameter types ----------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchFilesParams {
    /// Search query (filename or path fragment)
    pub query: String,
    /// Maximum results to return (default: 20, max: 200)
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchByTypeParams {
    /// File type category: images, videos, audio, documents, code, archives, fonts, applications, folders
    pub file_type: String,
    /// Maximum results to return (default: 20, max: 200)
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListDirectoryParams {
    /// Absolute path to the directory
    pub path: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FileInfoParams {
    /// Absolute path to the file
    pub path: String,
}

// -- Tool implementations ----------------------------------------------------

#[tool_router]
impl ZeroMcpServer {
    #[tool(
        description = "Search files by name or path across all indexed locations. Returns matching file paths with size and modification time."
    )]
    async fn search_files(
        &self,
        Parameters(params): Parameters<SearchFilesParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let limit = params.limit.unwrap_or(20).min(200);
        let results = self.index.search(&params.query, limit);
        let text = Self::format_results(&results);
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        description = "Search files by type category. Categories: images, videos, audio, documents, code, archives, fonts, applications, folders."
    )]
    async fn search_by_type(
        &self,
        Parameters(params): Parameters<SearchByTypeParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let limit = params.limit.unwrap_or(20).min(200);
        let results = self.index.search_by_type(&params.file_type, limit);
        let text = Self::format_results(&results);
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        description = "List contents of a directory. Returns name, type, size, and modification time for each entry."
    )]
    async fn list_directory(
        &self,
        Parameters(params): Parameters<ListDirectoryParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let path = Path::new(&params.path);
        if !path.is_absolute() {
            return Ok(CallToolResult::error(vec![Content::text(
                "Path must be absolute",
            )]));
        }

        let entries = match std::fs::read_dir(path) {
            Ok(entries) => entries,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read directory: {e}"
                ))]));
            }
        };

        let mut lines = Vec::new();
        for entry in entries.flatten() {
            let meta = entry.metadata().ok();
            let name = entry.file_name().to_string_lossy().to_string();
            let is_dir = meta.as_ref().is_some_and(|m| m.is_dir());
            let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
            let kind = if is_dir { "dir" } else { "file" };
            lines.push(format!("{name}\t{kind}\t{}", format_size(size)));
        }

        lines.sort();
        let text = if lines.is_empty() {
            "Directory is empty.".to_string()
        } else {
            lines.join("\n")
        };
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        description = "Get metadata for a specific file: size, type, modification time, permissions."
    )]
    async fn file_info(
        &self,
        Parameters(params): Parameters<FileInfoParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let path = Path::new(&params.path);
        let meta = match std::fs::metadata(path) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read file info: {e}"
                ))]));
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
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let text = format!(
            "path: {}\ntype: {kind}\nsize: {}\nmodified: {}",
            params.path,
            format_size(meta.len()),
            format_time(mtime),
        );
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }
}

// -- ServerHandler -----------------------------------------------------------

#[tool_handler]
impl ServerHandler for ZeroMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            "Zero file search MCP server — search files across locally indexed \
                 directories. All operations are read-only and local. Use search_files for \
                 name/path queries, search_by_type for category filters, list_directory to \
                 browse folders, and file_info for metadata.",
        )
    }
}
