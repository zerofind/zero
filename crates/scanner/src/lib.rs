//! Scanner module - parallel directory tree scanning
//!
//! This is the foundation module that everything else builds on.
//! It provides fast, parallel scanning of directory trees with
//! metadata collection using jwalk.
//!
//! ## Progress Tracking
//!
//! For real-time progress during crawls, use `scan_with_progress` or
//! `scan_collect_with_progress` with a `CrawlProgress` instance:
//!
//! ```no_run
//! use std::sync::Arc;
//! use std::path::Path;
//! use scanner::{scan_with_progress, ScanOptions, CrawlProgress};
//!
//! let progress = Arc::new(CrawlProgress::new());
//! let entries = scan_with_progress(
//!     Path::new("/some/path"),
//!     &ScanOptions::default(),
//!     Some(progress),
//! ).unwrap();
//! ```

mod entry;
mod filter;
mod walker;

pub use entry::FileEntry;
pub use filter::{DEFAULT_IGNORE_PATTERNS, ScanFilter};
pub use walker::{
    CrawlProgress, ScanError, ScanOptions, scan, scan_collect, scan_collect_with_progress,
    scan_with_progress,
};
