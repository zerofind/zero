//! Transfer state persistence (etch-backed)
//!
//! Manages persistent state for transfer jobs to enable resume after interruption.
//! State is stored in an etch store (snapshot + WAL) in a `.zero` directory
//! at the destination.
//!
//! ## WAL Collections
//!
//! - Collection 0: job metadata (single entry, key = 0u64)
//! - Collection 1: completed entries (key = path string bytes)
//!
//! ## Design
//!
//! Etch WAL provides crash-safe append-only persistence.
//! Each `append_completed` becomes a single WAL op.
//! State is transient — deleted when the job completes via `cleanup()`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use etchdb::{EtchKey, Op, Replayable, Store, Transactable, WalBackend};
use serde::{Deserialize, Serialize};

/// WAL collection for job metadata
const JOB: u8 = 0;
/// WAL collection for completed entries
const COMPLETED: u8 = 1;
/// Directory name for state files
const STATE_DIR: &str = ".zero";

/// Error type for state operations
#[derive(Debug, thiserror::Error)]
pub enum StateError {
    #[error("Failed to create state directory: {0}")]
    CreateDirError(#[source] std::io::Error),

    #[error("State persistence error: {0}")]
    Etch(String),

    #[error("No active job found")]
    NoActiveJob,
}

/// Metadata about a transfer job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobState {
    /// Unique job ID (timestamp-based)
    pub id: String,
    /// Source directory
    pub source: PathBuf,
    /// Destination directory
    pub dest: PathBuf,
    /// When the job was started (Unix timestamp)
    pub started_at: u64,
    /// Total files to transfer
    pub total_files: usize,
    /// Total bytes to transfer
    pub total_bytes: u64,
}

impl JobState {
    /// Create a new job state
    pub fn new(source: PathBuf, dest: PathBuf, total_files: usize, total_bytes: u64) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        Self {
            id: format!("{}", timestamp),
            source,
            dest,
            started_at: timestamp,
            total_files,
            total_bytes,
        }
    }
}

/// Entry in the completed log
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletedEntry {
    /// Relative path from source/dest root
    pub path: PathBuf,
    /// File size in bytes
    pub size: u64,
    /// Hash of the file (hex string, may be empty)
    pub hash: String,
}

impl CompletedEntry {
    /// Parse a completed entry from a log line (legacy format)
    pub fn from_line(line: &str) -> Option<Self> {
        let parts: Vec<&str> = line.splitn(3, '|').collect();
        if parts.len() != 3 {
            return None;
        }

        Some(Self {
            path: PathBuf::from(parts[0]),
            size: parts[1].parse().ok()?,
            hash: parts[2].to_string(),
        })
    }

    /// Format as a log line (legacy format)
    pub fn to_line(&self) -> String {
        format!("{}|{}|{}", self.path.display(), self.size, self.hash)
    }
}

// =============================================================================
// Etch state
// =============================================================================

/// Etch-backed transfer job state
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct TransferJobState {
    pub job: Option<JobState>,
    pub completed: Vec<CompletedEntry>,
}

impl Replayable for TransferJobState {
    fn apply(&mut self, ops: &[Op]) -> etchdb::Result<()> {
        for op in ops {
            match op {
                Op::Put {
                    collection,
                    key,
                    value,
                } => match *collection {
                    JOB => {
                        let job: JobState =
                            postcard::from_bytes(value).map_err(etchdb::Error::Postcard)?;
                        self.job = Some(job);
                    }
                    COMPLETED => {
                        let entry: CompletedEntry =
                            postcard::from_bytes(value).map_err(etchdb::Error::Postcard)?;
                        self.completed.push(entry);
                    }
                    _ => {
                        let _ = key; // suppress unused warning
                    }
                },
                Op::Delete { collection, .. } => {
                    if *collection == JOB {
                        self.job = None;
                    }
                    // Completed entries are never individually deleted
                }
            }
        }
        Ok(())
    }
}

/// Transaction type for TransferJobState
pub struct TransferTx<'a> {
    #[allow(dead_code)]
    pub state: &'a TransferJobState,
    overlay: TransferOverlay,
}

/// Overlay capturing mutations during a transaction
#[derive(Default)]
pub struct TransferOverlay {
    ops: Vec<Op>,
    new_job: Option<JobState>,
    clear_job: bool,
    new_entries: Vec<CompletedEntry>,
}

impl<'a> TransferTx<'a> {
    /// Set the job metadata
    pub fn set_job(&mut self, job: &JobState) {
        let value = postcard::to_allocvec(job).expect("JobState serialization should not fail");
        self.overlay.ops.push(Op::Put {
            collection: JOB,
            key: 0u64.to_bytes(),
            value,
        });
        self.overlay.new_job = Some(job.clone());
    }

    /// Clear the job metadata
    #[allow(dead_code)]
    pub fn clear_job(&mut self) {
        self.overlay.ops.push(Op::Delete {
            collection: JOB,
            key: 0u64.to_bytes(),
        });
        self.overlay.clear_job = true;
    }

    /// Append a completed entry
    pub fn append_entry(&mut self, entry: &CompletedEntry) {
        let key = entry.path.to_string_lossy().as_bytes().to_vec();
        let value =
            postcard::to_allocvec(entry).expect("CompletedEntry serialization should not fail");
        self.overlay.ops.push(Op::Put {
            collection: COMPLETED,
            key,
            value,
        });
        self.overlay.new_entries.push(entry.clone());
    }
}

impl Transactable for TransferJobState {
    type Tx<'a> = TransferTx<'a>;
    type Overlay = TransferOverlay;

    fn begin_tx(&self) -> Self::Tx<'_> {
        TransferTx {
            state: self,
            overlay: TransferOverlay::default(),
        }
    }

    fn finish_tx(tx: Self::Tx<'_>) -> (Vec<Op>, Self::Overlay) {
        let ops = tx.overlay.ops.clone();
        (ops, tx.overlay)
    }

    fn apply_overlay(&mut self, overlay: Self::Overlay) {
        if overlay.clear_job {
            self.job = None;
        }
        if let Some(job) = overlay.new_job {
            self.job = Some(job);
        }
        self.completed.extend(overlay.new_entries);
    }
}

// =============================================================================
// TransferState — public API
// =============================================================================

/// Manages transfer state persistence via etch
pub struct TransferState {
    store: Store<TransferJobState, WalBackend<TransferJobState>>,
    state_dir: PathBuf,
}

impl TransferState {
    /// Create a new transfer state manager
    ///
    /// Creates the `.zero` directory and etch store if they don't exist.
    pub fn new(dest: &Path) -> Result<Self, StateError> {
        let state_dir = dest.join(STATE_DIR);

        std::fs::create_dir_all(&state_dir).map_err(StateError::CreateDirError)?;

        let store =
            Store::<TransferJobState, WalBackend<TransferJobState>>::open_wal(state_dir.clone())
                .map_err(|e| StateError::Etch(e.to_string()))?;

        Ok(Self { store, state_dir })
    }

    /// Get the state directory path
    pub fn state_dir(&self) -> &Path {
        &self.state_dir
    }

    /// Check if there's an active job
    pub fn has_active_job(&self) -> bool {
        self.store.read().job.is_some()
    }

    /// Load job state
    pub fn load_job(&self) -> Result<Option<JobState>, StateError> {
        Ok(self.store.read().job.clone())
    }

    /// Save job state
    pub fn save_job(&self, job: &JobState) -> Result<(), StateError> {
        self.store
            .write(|tx| {
                tx.set_job(job);
                Ok(())
            })
            .map_err(|e| StateError::Etch(e.to_string()))
    }

    /// Append a completed file entry
    ///
    /// Each entry becomes a single WAL op — crash-safe by design.
    pub fn append_completed(&self, entry: &CompletedEntry) -> Result<(), StateError> {
        self.store
            .write(|tx| {
                tx.append_entry(entry);
                Ok(())
            })
            .map_err(|e| StateError::Etch(e.to_string()))
    }

    /// Load all completed files as a set of paths for quick lookup
    ///
    /// Used on resume to know which files to skip.
    pub fn load_completed_set(&self) -> Result<HashSet<PathBuf>, StateError> {
        let state = self.store.read();
        Ok(state.completed.iter().map(|e| e.path.clone()).collect())
    }

    /// Load all completed entries (with full metadata)
    pub fn load_completed_entries(&self) -> Result<Vec<CompletedEntry>, StateError> {
        let state = self.store.read();
        Ok(state.completed.clone())
    }

    /// Get count of completed files
    pub fn completed_count(&self) -> Result<usize, StateError> {
        Ok(self.store.read().completed.len())
    }

    /// Clean up state files (call when job completes successfully)
    pub fn cleanup(&self) -> Result<(), StateError> {
        // Remove the entire state directory
        let _ = std::fs::remove_dir_all(&self.state_dir);
        Ok(())
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod tests;
