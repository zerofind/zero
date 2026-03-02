//! Cache module - split database architecture for zero
//!
//! This module provides persistent storage using etch with a split architecture:
//!
//! - `control/` - Etch store for settings, automations, runs, storage registry (UI-owned)
//! - `storages/{identifier}/` - Per-storage etch stores for file hashes (daemon-owned)
//!
//! ## Architecture
//!
//! ```text
//! ~/.local/share/zero/
//! ├── control/                      # Etch store (snapshot.postcard + wal.bin)
//! │   ├── storages (registry)
//! │   ├── automations
//! │   └── runs
//! │
//! └── storages/
//!     ├── serial_ABC123/            # USB drive by serial (etch dir)
//!     ├── uuid_disk0s2/             # Internal drive by UUID (etch dir)
//!     └── path_a1b2c3d4/            # Network/folder by path hash (etch dir)
//! ```
//!
//! ## Benefits
//!
//! - **Lock isolation**: UI uses control store, daemon uses storage stores
//! - **Easy cleanup**: Delete storage dir = all data gone
//! - **Device ID stability**: Serial numbers survive disk renames
//! - **Parallel access**: Different threads access different stores
//!
//! ## Usage
//!
//! ```ignore
//! use zero::cache::{CacheManager, DeviceInfo, CacheEntry};
//!
//! // Open the cache manager
//! let manager = CacheManager::open()?;
//!
//! // Get or create a storage and its database
//! let device_info = DeviceInfo::usb("ABC123", Some("My USB Drive".to_string()));
//! let (storage, db) = manager.get_or_create_storage(
//!     Path::new("/Volumes/MyDrive"),
//!     Some(device_info)
//! )?;
//!
//! // Lookup a cached file (instant if metadata matches)
//! if let Some(cached) = db.lookup_valid_file("photos/vacation.jpg", size, mtime)? {
//!     println!("Cached hash: {:?}", cached.hash_xxh3);
//! }
//!
//! // Batch insert for efficiency
//! let mut batch = db.new_batch();
//! for file in files {
//!     batch.add(CacheEntry::with_xxh3(file.path, file.size, file.mtime, hash));
//!     if batch.should_flush() {
//!         db.flush_batch(&mut batch)?;
//!     }
//! }
//! db.flush_batch(&mut batch)?;
//! ```

pub mod automations;
mod batch;
pub mod control_state;
#[cfg(test)]
mod control_state_test;
mod db;
pub mod runs;
pub mod storage_state;
#[cfg(test)]
mod storage_state_test;
mod storages;

// Re-export main database types
pub use db::{CacheDb, CacheError, CacheManager, ControlDb, StorageDb};

// Re-export helper functions
pub use db::{is_cache_valid, is_cache_valid_for_metadata};

// Re-export batch operations
pub use batch::ChecksumBatch;

// Re-export file types (from storage_state, replacing old files module)
pub use storage_state::{CacheEntry, CachedFileEntry, StorageStats};

// Re-export storage types
pub use storages::{DeviceInfo, MountType, Storage, StorageIdentifier};

// Re-export automation types
pub use automations::{Automation, NewAutomation, PathMapping, Settings, Triggers};

// Re-export etch-backed control state
pub use control_state::{
    ControlState, ControlTx, EraseDeviceId, EraseJob, EraseJobProgress, EraseJobSeed,
    EraseJobSettings, IndexedRoot, open_control_store,
};

// Re-export etch-backed storage state
pub use storage_state::{StorageState, StorageTx, cache_entry_to_file_entry, open_storage_store};

// Re-export run types
pub use runs::{
    FileChange, PathProgress, PathStatus, Progress, ResultSummary, Run, RunResult, RunStatus,
    TriggerType,
};
