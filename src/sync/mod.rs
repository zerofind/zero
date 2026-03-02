//! Sync module - orchestrates scan → diff → transfer
//!
//! This is the main entry point for synchronization operations.
//! It combines the scanner, differ, and transfer modules to efficiently
//! sync only what's changed between source and destination.
//!
//! ## Usage
//!
//! ```ignore
//! use zero::sync::{SyncJob, SyncOptions};
//!
//! let options = SyncOptions::default();
//! let job = SyncJob::new("/source", "/dest", options)?;
//! let result = job.run(|progress| {
//!     println!("{:.1}% complete", progress.percent());
//! })?;
//! ```

mod job;
pub(crate) mod phases;
mod types;

pub use job::SyncJob;
pub use types::{
    DiffSummary, FolderStats, SyncAtomicProgress, SyncError, SyncOptions, SyncPhase,
    SyncPhaseAtomic, SyncProgress, SyncResult,
};
