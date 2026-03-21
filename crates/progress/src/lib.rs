//! Progress tracking and display
//!
//! Thread-safe progress tracking that can be updated from multiple parallel
//! operations and displayed in real-time.
//!
//! ## Components
//!
//! - [`AtomicProgress`] - Thread-safe counters for bytes/files processed
//! - [`ProgressDisplay`] - Display thread that polls and prints progress
//! - [`format`] - Formatting helpers for bytes, duration, throughput

mod display;
mod format;
mod tracker;

#[cfg(test)]
mod display_test;
#[cfg(test)]
mod format_test;
#[cfg(test)]
mod tracker_test;

pub use display::{DisplayConfig, ProgressDisplay, ProgressDisplayBuilder};
pub use format::{
    format_bytes, format_duration, format_eta, format_size_range, format_throughput,
    truncate_filename,
};
pub use tracker::{AtomicProgress, ProgressSnapshot};
