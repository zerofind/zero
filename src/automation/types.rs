//! Types for the automation system

use std::path::PathBuf;

use crate::cache::{CacheError, Progress};

/// Events that can trigger automations
#[derive(Debug, Clone)]
pub enum AutomationEvent {
    /// USB device mounted
    UsbMounted {
        /// Device serial number
        serial: String,
        /// Mount point path
        mount_point: PathBuf,
        /// Volume name
        volume_name: Option<String>,
    },

    /// USB device unmounted
    UsbUnmounted {
        /// Device serial number
        serial: String,
        /// Mount point path (may no longer exist)
        mount_point: PathBuf,
    },

    /// Files changed in a watched path
    FilesChanged {
        /// Root path being watched
        watch_root: PathBuf,
        /// Paths that changed
        changed_paths: Vec<PathBuf>,
    },

    /// Manual trigger
    Manual {
        /// Automation ID to run
        automation_id: i64,
    },
}

/// Configuration for the executor
#[derive(Debug, Clone)]
pub struct ExecutorConfig {
    /// Maximum concurrent automations
    pub max_concurrent: usize,

    /// Whether to verify checksums after copy
    pub default_verify: bool,
}

impl Default for ExecutorConfig {
    fn default() -> Self {
        Self {
            max_concurrent: 2,
            default_verify: true,
        }
    }
}

/// Errors from the executor
#[derive(Debug, thiserror::Error)]
pub enum ExecutorError {
    #[error("Database error: {0}")]
    Database(#[from] CacheError),

    #[error("Sync error: {0}")]
    Sync(String),

    #[error("Automation not found: {0}")]
    AutomationNotFound(i64),

    #[error("Already running: automation {0} has an active run")]
    AlreadyRunning(i64),

    #[error("Destination not available: {0}")]
    DestinationNotAvailable(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Callback type for sync progress updates
pub type SyncCallbackFn = Box<dyn Fn(i64, &Progress) + Send + Sync>;

/// Wrapper for optional callback
pub struct SyncCallback(Option<SyncCallbackFn>);

impl SyncCallback {
    pub fn new<F>(f: F) -> Self
    where
        F: Fn(i64, &Progress) + Send + Sync + 'static,
    {
        Self(Some(Box::new(f)))
    }

    pub fn none() -> Self {
        Self(None)
    }

    pub fn call(&self, run_id: i64, progress: &Progress) {
        if let Some(ref f) = self.0 {
            f(run_id, progress);
        }
    }
}

impl std::fmt::Debug for SyncCallback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "SyncCallback({})",
            if self.0.is_some() { "Some" } else { "None" }
        )
    }
}
