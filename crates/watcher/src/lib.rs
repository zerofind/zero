//! zero-watcher - File system and USB mount watching for zero
//!
//! This crate provides real-time monitoring of:
//! - File system changes (create, modify, delete, rename)
//! - USB drive mount/unmount events
//!
//! Designed for integration with the zero sync engine to enable:
//! - Auto-sync when files change in watched folders
//! - Auto-sync when specific USB drives are connected
//!
//! # Architecture
//!
//! The library is designed with future Swift FFI in mind:
//! - Events are serialized as JSON for easy cross-language communication
//! - Callbacks are used for event delivery
//! - All public APIs are designed to be C-FFI compatible in the future
//!
//! # Example
//!
//! ```no_run
//! use zero_watcher::{FileWatcher, UsbWatcher};
//! use std::path::Path;
//! use std::time::Duration;
//!
//! fn main() -> anyhow::Result<()> {
//!     // Watch a folder for changes
//!     let mut file_watcher = FileWatcher::new()?;
//!     file_watcher.watch(Path::new("/Users/me/Documents"))?;
//!
//!     // Watch for USB mount/unmount
//!     let mut usb_watcher = UsbWatcher::new()?;
//!
//!     // Process events (polling style)
//!     loop {
//!         if let Some(event) = file_watcher.try_next_event() {
//!             println!("File event: {:?}", event);
//!         }
//!         if let Some(event) = usb_watcher.try_next_event() {
//!             println!("USB event: {:?}", event);
//!         }
//!         std::thread::sleep(Duration::from_millis(10));
//!     }
//! }
//! ```

pub mod events;
pub mod file_watcher;
pub mod usb_watcher;

// Re-export main types
pub use events::{FileChangeKind, FileEvent, UsbEvent, UsbEventKind, WatchEvent};
pub use file_watcher::FileWatcher;
pub use usb_watcher::UsbWatcher;

/// Configuration for file watching
#[derive(Debug, Clone)]
pub struct FileWatchConfig {
    /// Debounce delay in milliseconds
    /// Multiple rapid changes to the same file will be coalesced
    pub debounce_ms: u64,

    /// Whether to watch recursively (include subdirectories)
    pub recursive: bool,

    /// Patterns to ignore (glob patterns)
    pub ignore_patterns: Vec<String>,
}

impl Default for FileWatchConfig {
    fn default() -> Self {
        Self {
            debounce_ms: 100,
            recursive: true,
            ignore_patterns: vec![
                ".git".to_string(),
                ".DS_Store".to_string(),
                "*.swp".to_string(),
                "*~".to_string(),
                ".zero".to_string(),
            ],
        }
    }
}

/// Configuration for USB watching
#[derive(Debug, Clone)]
#[derive(Default)]
pub struct UsbWatchConfig {
    /// Only report events for volumes matching these paths (if empty, report all)
    pub volume_filter: Vec<String>,

    /// Only report events for devices with these serial numbers (if empty, report all)
    pub serial_filter: Vec<String>,
}

