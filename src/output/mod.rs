//! Output module - unified output formatting for CLI and programmatic use
//!
//! Provides two output modes:
//! - Human: Pretty, readable output for terminal use
//! - JSON: Machine-readable output for programmatic use (Swift GUI, scripts)
//!
//! ## Usage
//!
//! ```ignore
//! use zero::output::{OutputMode, Outputter};
//!
//! let out = Outputter::new(OutputMode::Human);
//! out.println("Starting scan...");
//! out.result(&response);
//! ```

mod types;

pub use types::*;

use serde::Serialize;

/// Output mode for CLI
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputMode {
    /// Human-readable output with colors and formatting
    #[default]
    Human,
    /// JSON output for programmatic use
    Json,
}

/// Output handler that formats based on mode
#[derive(Debug, Clone)]
pub struct Outputter {
    mode: OutputMode,
}

impl Outputter {
    /// Create a new outputter with the given mode
    pub fn new(mode: OutputMode) -> Self {
        Self { mode }
    }

    /// Get the current output mode
    pub fn mode(&self) -> OutputMode {
        self.mode
    }

    /// Check if in JSON mode
    pub fn is_json(&self) -> bool {
        self.mode == OutputMode::Json
    }

    /// Print a line (human mode only)
    pub fn println(&self, msg: &str) {
        if self.mode == OutputMode::Human {
            println!("{}", msg);
        }
    }

    /// Print a formatted line (human mode only)
    pub fn printfmt(&self, args: std::fmt::Arguments) {
        if self.mode == OutputMode::Human {
            println!("{}", args);
        }
    }

    /// Print an empty line (human mode only)
    pub fn newline(&self) {
        if self.mode == OutputMode::Human {
            println!();
        }
    }

    /// Print a header/title (human mode only)
    pub fn header(&self, msg: &str) {
        if self.mode == OutputMode::Human {
            println!("{}", msg);
            println!();
        }
    }

    /// Print an info message (human mode only)
    pub fn info(&self, msg: &str) {
        if self.mode == OutputMode::Human {
            println!("{}", msg);
        }
    }

    /// Print a success message (human mode only)
    pub fn success(&self, msg: &str) {
        if self.mode == OutputMode::Human {
            println!("✓ {}", msg);
        }
    }

    /// Print a warning message (human mode only)
    pub fn warn(&self, msg: &str) {
        if self.mode == OutputMode::Human {
            eprintln!("⚠ {}", msg);
        }
    }

    /// Print an error message (human mode only)
    pub fn error(&self, msg: &str) {
        if self.mode == OutputMode::Human {
            eprintln!("✗ {}", msg);
        }
    }

    /// Print a key-value pair (human mode only)
    pub fn kv(&self, key: &str, value: impl std::fmt::Display) {
        if self.mode == OutputMode::Human {
            println!("  {}: {}", key, value);
        }
    }

    /// Print an indented line (human mode only)
    pub fn indented(&self, msg: &str) {
        if self.mode == OutputMode::Human {
            println!("  {}", msg);
        }
    }

    /// Print a list item (human mode only)
    pub fn list_item(&self, msg: &str) {
        if self.mode == OutputMode::Human {
            println!("  {}", msg);
        }
    }

    /// Output the final result (works in both modes)
    pub fn result<T: Serialize>(&self, response: &Response<T>) {
        match self.mode {
            OutputMode::Human => {
                // Human mode already printed everything, just show final status if error
                if !response.success
                    && let Some(ref err) = response.error {
                        eprintln!("Error: {}", err.message);
                    }
            }
            OutputMode::Json => {
                // Print the full JSON response
                if let Ok(json) = serde_json::to_string_pretty(response) {
                    println!("{}", json);
                }
            }
        }
    }

    /// Output the final result as compact JSON (single line, for streaming)
    pub fn result_compact<T: Serialize>(&self, response: &Response<T>) {
        if self.mode == OutputMode::Json {
            if let Ok(json) = serde_json::to_string(response) {
                println!("{}", json);
            }
        } else {
            self.result(response);
        }
    }

    /// Output a progress event (JSON mode only, for streaming)
    pub fn progress(&self, info: &ProgressInfo) {
        if self.mode == OutputMode::Json
            && let Ok(json) = serde_json::to_string(info) {
                println!("{{\"type\":\"progress\",{}}}", &json[1..json.len() - 1]);
            }
    }

    /// Output any serializable data as JSON (JSON mode only)
    pub fn json<T: Serialize>(&self, data: &T) {
        if self.mode == OutputMode::Json
            && let Ok(json) = serde_json::to_string_pretty(data) {
                println!("{}", json);
            }
    }
}

/// Convenience macro for formatted printing in human mode
#[macro_export]
macro_rules! outfmt {
    ($out:expr, $($arg:tt)*) => {
        $out.printfmt(format_args!($($arg)*))
    };
}

/// Output an error response, branching on JSON/human mode.
///
/// Basic usage (human side just prints the message with out.error):
/// ```ignore
/// cmd_error!(out, "erase", start.elapsed().as_millis() as u64,
///     "NO_VOLUME", format!("No volume found at path: {}", path.display()));
/// ```
///
/// With custom human block:
/// ```ignore
/// cmd_error!(out, "erase", duration, "NO_VOLUME", msg, {
///     out.error(&msg);
///     out.info("Try running with --help");
/// });
/// ```
#[macro_export]
macro_rules! cmd_error {
    // Basic: human side just prints the message
    ($out:expr, $cmd:expr, $duration:expr, $code:expr, $msg:expr) => {
        if $out.is_json() {
            let response: $crate::output::Response<()> = $crate::output::Response::error(
                $cmd,
                $duration,
                $crate::output::ErrorInfo::new($code, $msg),
            );
            $out.result(&response);
        } else {
            $out.error(&$msg);
        }
    };
    // With custom human block
    ($out:expr, $cmd:expr, $duration:expr, $code:expr, $msg:expr, $human:block) => {
        if $out.is_json() {
            let response: $crate::output::Response<()> = $crate::output::Response::error(
                $cmd,
                $duration,
                $crate::output::ErrorInfo::new($code, $msg),
            );
            $out.result(&response);
        } else $human
    };
}

/// Output a success response, branching on JSON/human mode.
///
/// ```ignore
/// cmd_success!(out, "erase", start.elapsed().as_millis() as u64, data, {
///     out.success("Volume erased successfully");
/// });
/// ```
#[macro_export]
macro_rules! cmd_success {
    ($out:expr, $cmd:expr, $duration:expr, $data:expr, $human:block) => {
        if $out.is_json() {
            let response = $crate::output::Response::success($cmd, $duration, $data);
            $out.result(&response);
        } else $human
    };
}

/// Format bytes as human-readable string using binary units (1 KB = 1024 bytes)
///
/// Uses binary (IEC) units which are standard for storage/file size display.
/// For throughput display (network speeds), see `progress::format::format_bytes`
/// which uses decimal (SI) units (1 KB = 1000 bytes).
pub use crate::util::format_bytes;

/// Format bytes as MB (for consistent output)
pub fn format_mb(bytes: u64) -> String {
    format!("{:.2} MB", bytes as f64 / 1_000_000.0)
}

/// Format duration in human-readable form
pub fn format_duration(duration: std::time::Duration) -> String {
    let secs = duration.as_secs_f64();
    if secs < 0.001 {
        format!("{:.2}µs", secs * 1_000_000.0)
    } else if secs < 1.0 {
        format!("{:.2}ms", secs * 1000.0)
    } else if secs < 60.0 {
        format!("{:.2}s", secs)
    } else {
        let mins = (secs / 60.0).floor();
        let remaining_secs = secs - (mins * 60.0);
        format!("{}m {:.1}s", mins as u64, remaining_secs)
    }
}

/// Format throughput in MB/s
pub fn format_throughput(bytes: u64, duration: std::time::Duration) -> String {
    let secs = duration.as_secs_f64();
    if secs > 0.0 {
        let mbps = (bytes as f64 / 1_000_000.0) / secs;
        format!("{:.1} MB/s", mbps)
    } else {
        "∞ MB/s".to_string()
    }
}

/// Format a count with commas (e.g., 1,234,567)
pub fn format_count(n: usize) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.insert(0, ',');
        }
        result.insert(0, c);
    }
    result
}

#[cfg(test)]
#[path = "mod_test.rs"]
mod mod_test;
