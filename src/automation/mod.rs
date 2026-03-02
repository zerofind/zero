//! Automation module - connects watchers to sync execution
//!
//! This module provides:
//! - `Executor`: Runs automations when triggered by events
//! - Event handling for USB mount/unmount and file changes
//! - Progress tracking and run management
//!
//! ## Architecture
//!
//! ```text
//! Watcher Events (USB mount, file change)
//!     │
//!     ▼
//! Executor.handle_event()
//!     │
//!     ├── Find matching automations
//!     ├── Check if already running (skip if so)
//!     ├── Create run record
//!     ├── Execute sync for each path
//!     ├── Update progress in real-time
//!     └── Complete run (success/failed/partial)
//! ```
//!
//! ## Module Structure
//!
//! - `types`: Public types (AutomationEvent, ExecutorConfig, ExecutorError, SyncCallback)
//! - `executor`: Core executor with event handling and state management
//! - `runner`: Sync execution logic (run_automation, execute_sync, etc.)

mod executor;
mod runner;
mod types;

pub use executor::Executor;
pub use types::{AutomationEvent, ExecutorConfig, ExecutorError, SyncCallback, SyncCallbackFn};
