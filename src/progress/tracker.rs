//! Atomic progress counters for real-time progress tracking
//!
//! This module provides thread-safe progress tracking that can be updated
//! from multiple parallel operations and polled by a display thread.
//!
//! ## Design
//!
//! - Worker functions increment atomic counters as bytes are processed
//! - Display thread polls counters at regular intervals (e.g., 100ms)
//! - No callbacks or locks in the hot path - just atomic increments

use std::sync::RwLock;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Instant;

/// Thread-safe progress counters for file operations
///
/// All operations use `Ordering::Relaxed` since we don't need
/// synchronization guarantees - slightly stale values are fine for display.
#[derive(Debug)]
pub struct AtomicProgress {
    bytes_total: AtomicU64,
    bytes_done: AtomicU64,
    files_total: AtomicUsize,
    files_done: AtomicUsize,
    current_file: RwLock<String>,
    errors: AtomicUsize,
    started_at: Instant,
}

/// A snapshot of progress at a point in time (just data, no formatting)
#[derive(Debug, Clone)]
pub struct ProgressSnapshot {
    pub bytes_total: u64,
    pub bytes_done: u64,
    pub files_total: usize,
    pub files_done: usize,
    pub current_file: String,
    pub errors: usize,
    pub elapsed_secs: f64,
}

impl AtomicProgress {
    /// Create new progress tracker with known totals
    pub fn new(files_total: usize, bytes_total: u64) -> Self {
        Self {
            bytes_total: AtomicU64::new(bytes_total),
            bytes_done: AtomicU64::new(0),
            files_total: AtomicUsize::new(files_total),
            files_done: AtomicUsize::new(0),
            current_file: RwLock::new(String::new()),
            errors: AtomicUsize::new(0),
            started_at: Instant::now(),
        }
    }

    /// Create progress tracker with zero totals (set later)
    pub fn empty() -> Self {
        Self::new(0, 0)
    }

    // ==================== Setters (called once at start) ====================

    /// Set total bytes to process
    pub fn set_bytes_total(&self, total: u64) {
        self.bytes_total.store(total, Ordering::Relaxed);
    }

    /// Set total files to process
    pub fn set_files_total(&self, total: usize) {
        self.files_total.store(total, Ordering::Relaxed);
    }

    // ==================== Updates (called from worker threads) ====================

    /// Add bytes processed (called frequently during operations)
    #[inline]
    pub fn add_bytes(&self, bytes: u64) {
        self.bytes_done.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Mark a file as complete
    #[inline]
    pub fn file_done(&self) {
        self.files_done.fetch_add(1, Ordering::Relaxed);
    }

    /// Record an error
    #[inline]
    pub fn add_error(&self) {
        self.errors.fetch_add(1, Ordering::Relaxed);
    }

    /// Set current file being processed
    pub fn set_current_file(&self, path: &str) {
        if let Ok(mut current) = self.current_file.write() {
            current.clear();
            current.push_str(path);
        }
    }

    /// Clear current file (between files)
    pub fn clear_current_file(&self) {
        if let Ok(mut current) = self.current_file.write() {
            current.clear();
        }
    }

    // ==================== Readers (called from display thread) ====================

    #[inline]
    pub fn bytes_done(&self) -> u64 {
        self.bytes_done.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn bytes_total(&self) -> u64 {
        self.bytes_total.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn files_done(&self) -> usize {
        self.files_done.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn files_total(&self) -> usize {
        self.files_total.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn errors(&self) -> usize {
        self.errors.load(Ordering::Relaxed)
    }

    pub fn current_file(&self) -> String {
        self.current_file
            .read()
            .map(|s| s.clone())
            .unwrap_or_default()
    }

    pub fn elapsed_secs(&self) -> f64 {
        self.started_at.elapsed().as_secs_f64()
    }

    /// Get a consistent snapshot of all progress values
    pub fn snapshot(&self) -> ProgressSnapshot {
        ProgressSnapshot {
            bytes_total: self.bytes_total(),
            bytes_done: self.bytes_done(),
            files_total: self.files_total(),
            files_done: self.files_done(),
            current_file: self.current_file(),
            errors: self.errors(),
            elapsed_secs: self.elapsed_secs(),
        }
    }

    /// Check if operation is complete
    pub fn is_complete(&self) -> bool {
        self.files_done() >= self.files_total() && self.files_total() > 0
    }

    /// Reset all counters (for reuse)
    pub fn reset(&self, files_total: usize, bytes_total: u64) {
        self.bytes_total.store(bytes_total, Ordering::Relaxed);
        self.bytes_done.store(0, Ordering::Relaxed);
        self.files_total.store(files_total, Ordering::Relaxed);
        self.files_done.store(0, Ordering::Relaxed);
        self.errors.store(0, Ordering::Relaxed);
        self.clear_current_file();
    }
}

impl ProgressSnapshot {
    /// Percentage complete (0.0 to 100.0)
    pub fn percent(&self) -> f64 {
        if self.bytes_total == 0 {
            if self.files_total == 0 {
                100.0
            } else {
                (self.files_done as f64 / self.files_total as f64) * 100.0
            }
        } else {
            (self.bytes_done as f64 / self.bytes_total as f64) * 100.0
        }
    }

    /// Throughput in bytes per second
    pub fn throughput(&self) -> f64 {
        if self.elapsed_secs > 0.0 {
            self.bytes_done as f64 / self.elapsed_secs
        } else {
            0.0
        }
    }

    /// Estimated time remaining in seconds
    pub fn eta_secs(&self) -> Option<f64> {
        let throughput = self.throughput();
        if throughput > 0.0 {
            let remaining = self.bytes_total.saturating_sub(self.bytes_done);
            Some(remaining as f64 / throughput)
        } else {
            None
        }
    }
}

impl Default for AtomicProgress {
    fn default() -> Self {
        Self::empty()
    }
}

// Make AtomicProgress safe to share across threads
unsafe impl Send for AtomicProgress {}
unsafe impl Sync for AtomicProgress {}
