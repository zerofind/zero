//! Service logging for debugging and tracing
//!
//! Provides file-based logging for the service mode, writing to
//! `~/Library/Logs/zero/service.log` on macOS.
//!
//! Log levels:
//! - ERROR: Failures that affect service operation
//! - WARN: Recoverable issues
//! - INFO: Service lifecycle events (start, stop, connections)
//! - DEBUG: Request/response details, event processing
//! - TRACE: Raw JSON-RPC messages (very verbose)

use std::fs::{File, OpenOptions, create_dir_all};
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

/// Log level for filtering messages
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Trace = 0,
    Debug = 1,
    Info = 2,
    Warn = 3,
    Error = 4,
}

impl LogLevel {
    fn as_str(self) -> &'static str {
        match self {
            LogLevel::Trace => "TRACE",
            LogLevel::Debug => "DEBUG",
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERROR",
        }
    }
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Service logger that writes to file
pub struct ServiceLogger {
    writer: Arc<Mutex<BufWriter<File>>>,
    min_level: LogLevel,
    log_path: PathBuf,
}

impl ServiceLogger {
    /// Create a new logger writing to the given path
    pub fn new(log_path: PathBuf, min_level: LogLevel) -> std::io::Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = log_path.parent() {
            create_dir_all(parent)?;
        }

        // Open file for appending (create if doesn't exist)
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)?;

        let writer = BufWriter::new(file);

        Ok(Self {
            writer: Arc::new(Mutex::new(writer)),
            min_level,
            log_path,
        })
    }

    /// Get the log file path
    pub fn log_path(&self) -> &PathBuf {
        &self.log_path
    }

    /// Log a message at the given level
    pub fn log(&self, level: LogLevel, target: &str, message: &str) {
        if level < self.min_level {
            return;
        }

        let timestamp = format_timestamp();
        let line = format!(
            "{} {} [{}] {}\n",
            timestamp,
            level.as_str(),
            target,
            message
        );

        if let Ok(mut writer) = self.writer.lock() {
            let _ = writer.write_all(line.as_bytes());
            let _ = writer.flush();
        }
    }

    /// Log at ERROR level
    pub fn error(&self, target: &str, message: &str) {
        self.log(LogLevel::Error, target, message);
    }

    /// Log at WARN level
    pub fn warn(&self, target: &str, message: &str) {
        self.log(LogLevel::Warn, target, message);
    }

    /// Log at INFO level
    pub fn info(&self, target: &str, message: &str) {
        self.log(LogLevel::Info, target, message);
    }

    /// Log at DEBUG level
    pub fn debug(&self, target: &str, message: &str) {
        self.log(LogLevel::Debug, target, message);
    }

    /// Log at TRACE level
    pub fn trace(&self, target: &str, message: &str) {
        self.log(LogLevel::Trace, target, message);
    }

    /// Rotate log file if it exceeds max size (10MB default)
    pub fn rotate_if_needed(&self, max_bytes: u64) -> std::io::Result<bool> {
        let metadata = std::fs::metadata(&self.log_path)?;

        if metadata.len() > max_bytes {
            // Rename current log to .old
            let old_path = self.log_path.with_extension("log.old");
            let _ = std::fs::remove_file(&old_path); // Ignore error if doesn't exist
            std::fs::rename(&self.log_path, &old_path)?;

            // Reopen fresh file
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.log_path)?;

            if let Ok(mut writer) = self.writer.lock() {
                *writer = BufWriter::new(file);
            }

            return Ok(true);
        }

        Ok(false)
    }
}

impl Clone for ServiceLogger {
    fn clone(&self) -> Self {
        Self {
            writer: Arc::clone(&self.writer),
            min_level: self.min_level,
            log_path: self.log_path.clone(),
        }
    }
}

/// Format current timestamp as ISO 8601
fn format_timestamp() -> String {
    let now = SystemTime::now();
    let duration = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();

    let secs = duration.as_secs();
    let millis = duration.subsec_millis();

    // Convert to datetime components (simplified, assumes UTC)
    let days_since_epoch = secs / 86400;
    let time_of_day = secs % 86400;

    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Calculate year/month/day from days since epoch
    // This is a simplified calculation
    let (year, month, day) = days_to_ymd(days_since_epoch);

    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}.{millis:03}Z")
}

/// Convert days since Unix epoch to year/month/day
fn days_to_ymd(days: u64) -> (u32, u32, u32) {
    // Days since 1970-01-01
    let mut remaining = days as i64;
    let mut year = 1970i32;

    // Find year
    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        year += 1;
    }

    // Find month
    let leap = is_leap_year(year);
    let days_in_months = if leap {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 1u32;
    for days_in_month in days_in_months {
        if remaining < days_in_month {
            break;
        }
        remaining -= days_in_month;
        month += 1;
    }

    let day = remaining as u32 + 1;

    (year as u32, month, day)
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

/// Get the default log directory for the service (`~/.zero/logs/`)
pub fn default_log_dir() -> Option<PathBuf> {
    foundation::dirs::logs_dir()
}

/// Set up service logging with default configuration
///
/// Returns the logger and the path to the log file.
pub fn setup_service_logging(verbose: bool) -> std::io::Result<ServiceLogger> {
    let log_dir = default_log_dir().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Could not determine log directory",
        )
    })?;

    let log_path = log_dir.join("service.log");

    let min_level = if verbose {
        LogLevel::Debug
    } else {
        LogLevel::Info
    };

    let logger = ServiceLogger::new(log_path, min_level)?;

    // Log startup
    logger.info(
        "service",
        &format!(
            "Logging initialized (level: {}, path: {})",
            min_level,
            logger.log_path().display()
        ),
    );

    Ok(logger)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_level_ordering() {
        assert!(LogLevel::Trace < LogLevel::Debug);
        assert!(LogLevel::Debug < LogLevel::Info);
        assert!(LogLevel::Info < LogLevel::Warn);
        assert!(LogLevel::Warn < LogLevel::Error);
    }

    #[test]
    fn test_days_to_ymd() {
        // 1970-01-01
        assert_eq!(days_to_ymd(0), (1970, 1, 1));
        // 2000-01-01 (leap year)
        assert_eq!(days_to_ymd(10957), (2000, 1, 1));
        // 2024-03-15
        assert_eq!(days_to_ymd(19797), (2024, 3, 15));
    }

    #[test]
    fn test_timestamp_format() {
        let ts = format_timestamp();
        // Should match ISO 8601 format
        assert!(ts.contains('T'));
        assert!(ts.ends_with('Z'));
        assert_eq!(ts.len(), 24); // YYYY-MM-DDTHH:MM:SS.mmmZ
    }
}
