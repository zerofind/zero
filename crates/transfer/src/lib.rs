//! Transfer module - resilient file transfer with resume support
//!
//! This module provides two transfer strategies:
//!
//! ## Fast Local Copy (default)
//!
//! Uses `std::fs::copy()` for local file transfers. This is ~28x faster than
//! chunked copying because it leverages OS-level optimizations:
//! - APFS clonefile (instant copy on same volume)
//! - `sendfile/copy_file_range` syscalls
//! - Kernel-level buffering
//!
//! ## Chunked Copy (for network/resumable transfers)
//!
//! Uses manual chunked I/O with progress callbacks. Slower but provides:
//! - Fine-grained progress reporting
//! - Resume from exact byte offset
//! - Streaming hash computation
//!
//! Use chunked copy for network transfers or when resume granularity matters.
//!
//! ## State Persistence
//!
//! Transfer state is stored in a `.zero/` directory at the destination:
//! - `job.json` - Job metadata (source, dest, totals)
//! - `completed.log` - Append-only log of completed files
//!
//! On resume, completed files are skipped. Partial files are deleted and
//! recopied (simpler and fast enough with `std::fs::copy`).

mod chunk;
mod chunked;
mod chunked_types;
mod job;
mod permissions;
mod state;

// Fast local copy (default)
pub use chunk::{CopyError, CopyOptions, CopyProgress, CopyWithHashResult, DEFAULT_CHUNK_SIZE};
pub use chunk::{
    copy_file, copy_file_with_hash, copy_file_with_hash_progress, copy_file_with_progress,
    get_partial_size, remove_partial,
};

// Permission comparison and sync
pub use permissions::{
    PermissionCompareResult, PermissionMismatch, PermissionSyncResult, compare_permissions,
    sync_dir_permissions,
};

// Chunked copy for network/resumable transfers
pub use chunked::copy_chunked_with_resume;
pub use chunked_types::{
    ChunkedCopyError, ChunkedCopyOptions, ChunkedCopyResult, ChunkedProgress, ChunkedTransferState,
    DEFAULT_CHUNK_THRESHOLD, should_use_chunked,
};

// Job orchestration
pub use job::{TransferError, TransferJob, TransferOptions, TransferProgress, TransferResult};

// State persistence
pub use state::{CompletedEntry, JobState, StateError, TransferState};

// Re-export AtomicProgress from progress module for backwards compatibility
pub use progress::AtomicProgress;
