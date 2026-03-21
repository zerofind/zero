//! Batch operations for efficient bulk writes to per-storage databases
//!
//! Collects entries in memory and writes them in batches via the etch store,
//! dramatically improving performance by reducing WAL flushes.
//!
//! ## Usage
//!
//! ```ignore
//! use cache::{ChecksumBatch, CacheEntry, StorageDb};
//!
//! let mut batch = ChecksumBatch::new();
//!
//! for file in files {
//!     let entry = CacheEntry::with_xxh3(file.path, file.size, file.mtime, hash);
//!     batch.add(entry);
//!
//!     if batch.should_flush() {
//!         storage_db.flush_batch(&mut batch)?;
//!     }
//! }
//!
//! // Flush remaining entries
//! storage_db.flush_batch(&mut batch)?;
//! ```

use super::storage_state::CacheEntry;

/// Default batch size (number of entries before auto-flush)
pub const DEFAULT_BATCH_SIZE: usize = 1000;

/// Batched checksum writer for efficient bulk inserts
///
/// Collects file entries in memory and writes them in batches
/// using a single etch transaction per batch.
pub struct ChecksumBatch {
    /// Pending entries waiting to be flushed
    entries: Vec<CacheEntry>,
    /// Maximum entries before suggesting flush
    batch_size: usize,
    /// Total entries flushed so far
    total_flushed: usize,
}

impl ChecksumBatch {
    /// Create a new batch writer with default batch size
    pub fn new() -> Self {
        Self::with_batch_size(DEFAULT_BATCH_SIZE)
    }

    /// Create a new batch writer with custom batch size
    pub fn with_batch_size(batch_size: usize) -> Self {
        Self {
            entries: Vec::with_capacity(batch_size),
            batch_size,
            total_flushed: 0,
        }
    }

    /// Add an entry to the batch
    ///
    /// Note: This does not automatically flush. Call `should_flush()` and
    /// then `StorageDb::flush_batch()` to manage when writes happen.
    pub fn add(&mut self, entry: CacheEntry) {
        self.entries.push(entry);
    }

    /// Check if the batch should be flushed
    pub fn should_flush(&self) -> bool {
        self.entries.len() >= self.batch_size
    }

    /// Get the number of pending entries
    pub fn pending_count(&self) -> usize {
        self.entries.len()
    }

    /// Get the total number of entries flushed so far
    pub fn total_flushed(&self) -> usize {
        self.total_flushed
    }

    /// Check if the batch is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Take all pending entries out of the batch, returning them.
    /// The batch is cleared and ready for more entries.
    /// Records the count as flushed.
    pub fn take_entries(&mut self) -> Vec<CacheEntry> {
        let count = self.entries.len();
        self.total_flushed += count;
        std::mem::take(&mut self.entries)
    }
}

impl Default for ChecksumBatch {
    fn default() -> Self {
        Self::new()
    }
}
