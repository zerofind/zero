//! Progress display thread for real-time progress output
//!
//! Provides a display thread that polls progress counters
//! and prints formatted progress to stderr at regular intervals.

/// Minimum throughput (bytes/sec) to display ETA
/// Below this, we show "calculating..." instead of a potentially misleading ETA
const MIN_THROUGHPUT_FOR_ETA: f64 = 1000.0; // 1 KB/s

/// Maximum ETA in seconds (7 days) - anything above shows ">7 days"
const MAX_ETA_SECS: f64 = 7.0 * 24.0 * 3600.0;

use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use super::AtomicProgress;
use super::format::{
    format_eta, format_size_range, format_throughput, smooth_throughput, truncate_filename,
};

/// Configuration for progress display
#[derive(Debug, Clone)]
pub struct DisplayConfig {
    /// Prefix shown before progress (e.g., "Copying", "Hashing")
    pub prefix: String,
    /// How often to print progress (default: 500ms)
    pub print_interval_ms: u64,
    /// How often to poll progress (default: 100ms)
    pub poll_interval_ms: u64,
    /// Maximum filename length before truncation
    pub max_filename_len: usize,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            prefix: String::new(),
            print_interval_ms: 500,
            poll_interval_ms: 100,
            max_filename_len: 50,
        }
    }
}

/// Builder for creating a `ProgressDisplay`
pub struct ProgressDisplayBuilder {
    progress: Arc<AtomicProgress>,
    config: DisplayConfig,
}

impl ProgressDisplayBuilder {
    pub fn new(progress: Arc<AtomicProgress>) -> Self {
        Self {
            progress,
            config: DisplayConfig::default(),
        }
    }

    pub fn with_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.config.prefix = prefix.into();
        self
    }

    pub fn with_print_interval(mut self, ms: u64) -> Self {
        self.config.print_interval_ms = ms;
        self
    }

    pub fn with_poll_interval(mut self, ms: u64) -> Self {
        self.config.poll_interval_ms = ms;
        self
    }

    pub fn with_max_filename_len(mut self, len: usize) -> Self {
        self.config.max_filename_len = len;
        self
    }

    pub fn start(self) -> ProgressDisplay {
        ProgressDisplay::start_with_config(self.progress, self.config)
    }
}

/// A running progress display thread
pub struct ProgressDisplay {
    stop_flag: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl ProgressDisplay {
    pub fn builder(progress: Arc<AtomicProgress>) -> ProgressDisplayBuilder {
        ProgressDisplayBuilder::new(progress)
    }

    fn start_with_config(progress: Arc<AtomicProgress>, config: DisplayConfig) -> Self {
        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_flag_clone = Arc::clone(&stop_flag);

        let handle = thread::spawn(move || {
            Self::display_loop(progress, config, stop_flag_clone);
        });

        Self {
            stop_flag,
            handle: Some(handle),
        }
    }

    pub fn stop(mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
        // Clear the line after stopping
        eprint!("\r{}\r", " ".repeat(100));
        let _ = std::io::stderr().flush();
    }

    fn display_loop(
        progress: Arc<AtomicProgress>,
        config: DisplayConfig,
        stop_flag: Arc<AtomicBool>,
    ) {
        let mut last_print = Instant::now();
        let mut last_bytes = 0u64;
        let mut last_sample_time = Instant::now();
        let mut smoothed_throughput = 0.0f64;
        let poll_duration = Duration::from_millis(config.poll_interval_ms);

        loop {
            if stop_flag.load(Ordering::Relaxed) {
                break;
            }

            thread::sleep(poll_duration);

            if last_print.elapsed().as_millis() < u128::from(config.print_interval_ms) {
                continue;
            }
            last_print = Instant::now();

            let bytes_done = progress.bytes_done();
            let bytes_total = progress.bytes_total();
            let files_done = progress.files_done();
            let files_total = progress.files_total();
            let current_file = progress.current_file();

            // Don't print if nothing started yet
            if bytes_done == 0 && current_file.is_empty() {
                continue;
            }

            // Calculate instantaneous throughput over the sample period
            let sample_elapsed = last_sample_time.elapsed().as_secs_f64();
            let bytes_delta = bytes_done.saturating_sub(last_bytes);
            let instant_throughput = if sample_elapsed > 0.0 {
                bytes_delta as f64 / sample_elapsed
            } else {
                0.0
            };

            // Smooth the throughput (alpha=0.3 gives good stability)
            smoothed_throughput = smooth_throughput(instant_throughput, smoothed_throughput, 0.3);

            // Update for next sample
            last_bytes = bytes_done;
            last_sample_time = Instant::now();

            let line = Self::format_progress_line_with_throughput(
                &config,
                bytes_done,
                bytes_total,
                files_done,
                files_total,
                &current_file,
                smoothed_throughput,
            );

            eprint!("\r{line}");
            let _ = std::io::stderr().flush();
        }
    }

    fn format_progress_line_with_throughput(
        config: &DisplayConfig,
        bytes_done: u64,
        bytes_total: u64,
        files_done: usize,
        files_total: usize,
        current_file: &str,
        throughput: f64,
    ) -> String {
        let percent = if bytes_total > 0 {
            (bytes_done as f64 / bytes_total as f64) * 100.0
        } else {
            100.0
        };

        let size_range = format_size_range(bytes_done, bytes_total);

        let (speed_str, eta_str) = if throughput >= MIN_THROUGHPUT_FOR_ETA {
            let remaining_bytes = bytes_total.saturating_sub(bytes_done) as f64;
            let eta_secs = remaining_bytes / throughput;

            let eta_display = if eta_secs > MAX_ETA_SECS {
                ">7 days".to_string()
            } else {
                format_eta(Some(eta_secs))
            };

            (format_throughput(throughput), eta_display)
        } else if throughput > 0.0 {
            // Throughput is non-zero but too low for reliable ETA
            (format_throughput(throughput), "calculating...".to_string())
        } else {
            ("-- MB/s".to_string(), "calculating...".to_string())
        };

        let filename_display = truncate_filename(current_file, config.max_filename_len);

        let prefix = if config.prefix.is_empty() {
            String::new()
        } else {
            format!("{}: ", config.prefix)
        };

        format!(
            "  {prefix}[{files_done}/{files_total}] {percent:.1}% ({size_range}) {speed_str} {eta_str} | {filename_display}        "
        )
    }
}

impl Drop for ProgressDisplay {
    fn drop(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}
