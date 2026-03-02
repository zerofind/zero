//! Split database architecture for zero
//!
//! This module provides the main entry point for database operations with a split
//! architecture:
//!
//! - `ControlDb` - Settings, automations, runs, storage registry (etch-backed)
//! - `StorageDb` - Per-storage file hashes (etch-backed via StorageState)
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

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::ChecksumBatch;
use super::control_state::{
    ControlState, EraseJob, IndexedRoot, open_control_store,
};
use super::storage_state::{
    CacheEntry, CachedFileEntry, StorageState, StorageStats, cache_entry_to_file_entry,
    open_storage_store,
};
use super::storages::{DeviceInfo, Storage, StorageIdentifier};

use crate::cache::automations::{Automation, NewAutomation};
use crate::cache::runs::{
    Progress, Run, RunResult, RunStatus, TriggerType,
};

/// Error type for cache operations
#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("Etch error: {0}")]
    Etch(String),

    #[error("Failed to create directory: {path}")]
    CreateDir {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Storage not found: {0}")]
    StorageNotFound(String),

    #[error("Failed to delete storage database: {path}")]
    DeleteDb {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

// =============================================================================
// ControlDb - Etch-backed control database
// =============================================================================

/// Control database for settings, automations, and storage registry.
/// Backed by etch `Store<ControlState>`.
pub struct ControlDb {
    store: etchdb::Store<ControlState, etchdb::WalBackend<ControlState>>,
    db_path: PathBuf,
    storages_dir: PathBuf,
}

impl ControlDb {
    /// Open the control database at the default location
    pub fn open() -> Result<Self, CacheError> {
        let db_dir = default_db_dir();
        Self::open_at(&db_dir)
    }

    /// Open the control database at a specific directory
    pub fn open_at(db_dir: &Path) -> Result<Self, CacheError> {
        // Ensure directories exist
        fs::create_dir_all(db_dir).map_err(|e| CacheError::CreateDir {
            path: db_dir.to_string_lossy().to_string(),
            source: e,
        })?;

        let storages_dir = db_dir.join("storages");
        fs::create_dir_all(&storages_dir).map_err(|e| CacheError::CreateDir {
            path: storages_dir.to_string_lossy().to_string(),
            source: e,
        })?;

        let control_dir = db_dir.join("control");
        let store = open_control_store(&control_dir)
            .map_err(|e| CacheError::Etch(e.to_string()))?;

        Ok(Self {
            store,
            db_path: control_dir,
            storages_dir,
        })
    }

    /// Open an in-memory control database (for testing)
    pub fn open_memory() -> Result<Self, CacheError> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let base = std::env::temp_dir().join(format!(
            "zero_test_{}_{}",
            std::process::id(),
            id,
        ));
        let control_dir = base.join("control");
        let storages_dir = base.join("storages");
        fs::create_dir_all(&storages_dir).map_err(|e| CacheError::CreateDir {
            path: storages_dir.to_string_lossy().to_string(),
            source: e,
        })?;
        let store = open_control_store(&control_dir)
            .map_err(|e| CacheError::Etch(e.to_string()))?;
        Ok(Self {
            store,
            db_path: base,
            storages_dir,
        })
    }

    /// Get the path to the control database
    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    /// Get the storages directory
    pub fn storages_dir(&self) -> &Path {
        &self.storages_dir
    }

    // ==================== Storage Registry Operations ====================

    /// Get or create a storage entry
    pub fn get_or_create_storage(
        &self,
        path: &Path,
        device_info: Option<DeviceInfo>,
    ) -> Result<Storage, CacheError> {
        let path = path.to_path_buf();
        let storage = self.store.write(|tx| {
            let mut state_clone = tx.state.clone();
            let s = state_clone.get_or_create_storage(&path, device_info.clone());
            tx.put_storage(&s);
            Ok(s)
        }).map_err(|e| CacheError::Etch(e.to_string()))?;
        Ok(storage)
    }

    /// Get a storage by its ID
    pub fn get_storage_by_id(&self, id: i64) -> Result<Option<Storage>, CacheError> {
        let state = self.store.read();
        Ok(state.get_storage_by_id(id).cloned())
    }

    /// Get a storage by device serial
    pub fn get_storage_by_serial(&self, serial: &str) -> Result<Option<Storage>, CacheError> {
        let state = self.store.read();
        Ok(state.get_storage_by_serial(serial).cloned())
    }

    /// Get a storage by device UUID
    pub fn get_storage_by_uuid(&self, uuid: &str) -> Result<Option<Storage>, CacheError> {
        let state = self.store.read();
        Ok(state.get_storage_by_uuid(uuid).cloned())
    }

    /// Get a storage by path
    pub fn get_storage_by_path(&self, path: &Path) -> Result<Option<Storage>, CacheError> {
        let state = self.store.read();
        Ok(state.get_storage_by_path(path).cloned())
    }

    /// Get a storage by identifier
    pub fn get_storage_by_identifier(
        &self,
        identifier: &StorageIdentifier,
    ) -> Result<Option<Storage>, CacheError> {
        let state = self.store.read();
        Ok(state.get_storage_by_identifier(identifier).cloned())
    }

    /// List all storages
    pub fn list_storages(&self) -> Result<Vec<Storage>, CacheError> {
        let state = self.store.read();
        Ok(state.list_storages().into_iter().cloned().collect())
    }

    /// Update storage statistics
    pub fn update_storage_stats(
        &self,
        id: i64,
        file_count: i64,
        total_bytes: i64,
    ) -> Result<(), CacheError> {
        self.store.write(|tx| {
            let mut state_clone = tx.state.clone();
            state_clone.update_storage_stats(id, file_count, total_bytes);
            if let Some(s) = state_clone.get_storage_by_id(id) {
                tx.put_storage(s);
            }
            Ok(())
        }).map_err(|e| CacheError::Etch(e.to_string()))?;
        Ok(())
    }

    /// Delete a storage and its database file
    pub fn delete_storage(&self, id: i64) -> Result<(), CacheError> {
        // Get the db_filename before deleting
        let db_filename = self.store.write(|tx| {
            let mut state_clone = tx.state.clone();
            let filename = state_clone.delete_storage(id);
            tx.delete_storage(id);
            Ok(filename)
        }).map_err(|e| CacheError::Etch(e.to_string()))?;

        // Delete the storage database directory if it exists
        if let Some(filename) = db_filename {
            let db_path = self.storages_dir.join(&filename);
            if db_path.exists() {
                if db_path.is_dir() {
                    fs::remove_dir_all(&db_path).map_err(|e| CacheError::DeleteDb {
                        path: db_path.to_string_lossy().to_string(),
                        source: e,
                    })?;
                } else {
                    fs::remove_file(&db_path).map_err(|e| CacheError::DeleteDb {
                        path: db_path.to_string_lossy().to_string(),
                        source: e,
                    })?;
                }
            }
        }

        Ok(())
    }

    /// Open a storage database for a registered storage
    pub fn open_storage_db(&self, storage: &Storage) -> Result<StorageDb, CacheError> {
        let db_path = self.storages_dir.join(&storage.db_filename);
        StorageDb::open_at(&db_path)
    }

    /// Open a storage database by storage ID
    pub fn open_storage_db_by_id(&self, id: i64) -> Result<StorageDb, CacheError> {
        let storage = self
            .get_storage_by_id(id)?
            .ok_or_else(|| CacheError::StorageNotFound(format!("id={}", id)))?;
        self.open_storage_db(&storage)
    }

    /// Get or create a storage and open its database
    pub fn get_or_create_storage_db(
        &self,
        path: &Path,
        device_info: Option<DeviceInfo>,
    ) -> Result<(Storage, StorageDb), CacheError> {
        let storage = self.get_or_create_storage(path, device_info)?;
        let db = self.open_storage_db(&storage)?;
        Ok((storage, db))
    }

    // ==================== Automation Operations ====================

    /// List all automations
    pub fn list_automations(&self) -> Result<Vec<Automation>, CacheError> {
        let state = self.store.read();
        Ok(state.list_automations().into_iter().cloned().collect())
    }

    /// List enabled automations
    pub fn list_enabled_automations(&self) -> Result<Vec<Automation>, CacheError> {
        let state = self.store.read();
        Ok(state.list_enabled_automations().into_iter().cloned().collect())
    }

    /// Get an automation by ID
    pub fn get_automation(&self, id: i64) -> Result<Option<Automation>, CacheError> {
        let state = self.store.read();
        Ok(state.get_automation(id).cloned())
    }

    /// Create a new automation
    pub fn create_automation(&self, new: NewAutomation) -> Result<Automation, CacheError> {
        let automation = self.store.write(|tx| {
            let mut state_clone = tx.state.clone();
            let a = state_clone.create_automation(new);
            tx.put_automation(&a);
            Ok(a)
        }).map_err(|e| CacheError::Etch(e.to_string()))?;
        Ok(automation)
    }

    /// Update an automation
    pub fn update_automation(&self, id: i64, new: NewAutomation) -> Result<(), CacheError> {
        self.store.write(|tx| {
            let mut state_clone = tx.state.clone();
            state_clone.update_automation(id, new);
            if let Some(a) = state_clone.get_automation(id) {
                tx.put_automation(a);
            }
            Ok(())
        }).map_err(|e| CacheError::Etch(e.to_string()))?;
        Ok(())
    }

    /// Delete an automation and its runs
    pub fn delete_automation(&self, id: i64) -> Result<(), CacheError> {
        self.store.write(|tx| {
            // Collect run IDs to delete from current committed state
            let run_ids: Vec<i64> = tx.state.runs.iter()
                .filter(|(_, r)| r.automation_id == id)
                .map(|(rid, _)| *rid)
                .collect();
            for rid in run_ids {
                tx.delete_run(rid);
            }
            tx.delete_automation(id);
            Ok(())
        }).map_err(|e| CacheError::Etch(e.to_string()))?;
        Ok(())
    }

    /// Enable or disable an automation
    pub fn set_automation_enabled(&self, id: i64, enabled: bool) -> Result<(), CacheError> {
        self.store.write(|tx| {
            let mut state_clone = tx.state.clone();
            state_clone.set_automation_enabled(id, enabled);
            if let Some(a) = state_clone.get_automation(id) {
                tx.put_automation(a);
            }
            Ok(())
        }).map_err(|e| CacheError::Etch(e.to_string()))?;
        Ok(())
    }

    /// Find automations by device serial (on_mount trigger)
    pub fn find_automations_by_serial(&self, serial: &str) -> Result<Vec<Automation>, CacheError> {
        let state = self.store.read();
        Ok(state.find_automations_by_serial(serial).into_iter().cloned().collect())
    }

    /// Find automations by watch path (on_change trigger)
    pub fn find_automations_by_watch_path(&self, path: &str) -> Result<Vec<Automation>, CacheError> {
        let state = self.store.read();
        Ok(state.find_automations_by_watch_path(path).into_iter().cloned().collect())
    }

    // ==================== Run Operations ====================

    /// Start a new run
    pub fn start_run(
        &self,
        automation_id: i64,
        trigger: TriggerType,
        initial_progress: Option<Progress>,
    ) -> Result<Run, CacheError> {
        let run = self.store.write(|tx| {
            let mut state_clone = tx.state.clone();
            let r = state_clone.start_run(automation_id, trigger, initial_progress);
            tx.put_run(&r);
            Ok(r)
        }).map_err(|e| CacheError::Etch(e.to_string()))?;
        Ok(run)
    }

    /// Get a run by ID
    pub fn get_run(&self, id: i64) -> Result<Option<Run>, CacheError> {
        let state = self.store.read();
        Ok(state.get_run(id).cloned())
    }

    /// Get the latest run for an automation
    pub fn get_latest_run(&self, automation_id: i64) -> Result<Option<Run>, CacheError> {
        let state = self.store.read();
        Ok(state.get_latest_run(automation_id).cloned())
    }

    /// List runs for an automation
    pub fn list_runs_for_automation(
        &self,
        automation_id: i64,
        limit: usize,
    ) -> Result<Vec<Run>, CacheError> {
        let state = self.store.read();
        Ok(state.list_runs_for_automation(automation_id, limit).into_iter().cloned().collect())
    }

    /// Find running run for an automation
    pub fn find_running_run(&self, automation_id: i64) -> Result<Option<Run>, CacheError> {
        let state = self.store.read();
        Ok(state.find_running_run(automation_id).cloned())
    }

    /// Find all resumable runs
    pub fn find_resumable_runs(&self) -> Result<Vec<Run>, CacheError> {
        let state = self.store.read();
        Ok(state.find_resumable_runs().into_iter().cloned().collect())
    }

    /// Update run progress
    pub fn update_run_progress(&self, id: i64, progress: &Progress) -> Result<(), CacheError> {
        self.store.write(|tx| {
            let mut state_clone = tx.state.clone();
            state_clone.update_run_progress(id, progress);
            if let Some(r) = state_clone.get_run(id) {
                tx.put_run(r);
            }
            Ok(())
        }).map_err(|e| CacheError::Etch(e.to_string()))?;
        Ok(())
    }

    /// Complete a run with success
    pub fn complete_run_success(&self, id: i64, result: &RunResult) -> Result<(), CacheError> {
        self.store.write(|tx| {
            let mut state_clone = tx.state.clone();
            state_clone.complete_run_success(id, result);
            if let Some(r) = state_clone.get_run(id) {
                tx.put_run(r);
            }
            Ok(())
        }).map_err(|e| CacheError::Etch(e.to_string()))?;
        Ok(())
    }

    /// Complete a run with failure
    pub fn complete_run_failed(&self, id: i64, result: &RunResult) -> Result<(), CacheError> {
        self.store.write(|tx| {
            let mut state_clone = tx.state.clone();
            state_clone.complete_run_failed(id, result);
            if let Some(r) = state_clone.get_run(id) {
                tx.put_run(r);
            }
            Ok(())
        }).map_err(|e| CacheError::Etch(e.to_string()))?;
        Ok(())
    }

    /// Mark a run as partial (interrupted but resumable)
    pub fn mark_run_partial(
        &self,
        id: i64,
        result: &RunResult,
        resume_state: Option<&str>,
    ) -> Result<(), CacheError> {
        self.store.write(|tx| {
            let mut state_clone = tx.state.clone();
            state_clone.mark_run_partial(id, result, resume_state);
            if let Some(r) = state_clone.get_run(id) {
                tx.put_run(r);
            }
            Ok(())
        }).map_err(|e| CacheError::Etch(e.to_string()))?;
        Ok(())
    }

    /// Cancel a run
    pub fn cancel_run(&self, id: i64) -> Result<(), CacheError> {
        self.store.write(|tx| {
            let mut state_clone = tx.state.clone();
            state_clone.cancel_run(id);
            if let Some(r) = state_clone.get_run(id) {
                tx.put_run(r);
            }
            Ok(())
        }).map_err(|e| CacheError::Etch(e.to_string()))?;
        Ok(())
    }

    /// Mark all running runs as interrupted (startup recovery)
    pub fn mark_interrupted_on_startup(&self) -> Result<u64, CacheError> {
        let count = self.store.write(|tx| {
            // Find running runs from committed state
            let running_ids: Vec<i64> = tx.state.runs.iter()
                .filter(|(_, r)| r.status == RunStatus::Running)
                .map(|(id, _)| *id)
                .collect();
            let c = running_ids.len() as u64;
            let now = crate::util::now_timestamp();
            for id in running_ids {
                if let Some(r) = tx.state.runs.get(&id) {
                    let mut updated = r.clone();
                    updated.status = RunStatus::Partial;
                    updated.completed_at = Some(now);
                    updated.resumable = true;
                    tx.put_run(&updated);
                }
            }
            Ok(c)
        }).map_err(|e| CacheError::Etch(e.to_string()))?;
        Ok(count)
    }

    /// Delete runs older than N days
    pub fn delete_runs_older_than(&self, days: i64) -> Result<u64, CacheError> {
        let count = self.store.write(|tx| {
            let cutoff = crate::util::now_timestamp() - (days * 24 * 60 * 60);
            let ids_to_delete: Vec<i64> = tx.state.runs.iter()
                .filter(|(_, r)| {
                    r.status != RunStatus::Running
                        && r.completed_at.is_some_and(|t| t < cutoff)
                })
                .map(|(id, _)| *id)
                .collect();
            let c = ids_to_delete.len() as u64;
            for id in ids_to_delete {
                tx.delete_run(id);
            }
            Ok(c)
        }).map_err(|e| CacheError::Etch(e.to_string()))?;
        Ok(count)
    }

    // ==================== Indexed Root Operations ====================

    /// Insert or update an indexed root
    pub fn upsert_indexed_root(&self, path: &str, root: &IndexedRoot) -> Result<(), CacheError> {
        self.store.write(|tx| {
            tx.put_indexed_root(path, root);
            Ok(())
        }).map_err(|e| CacheError::Etch(e.to_string()))?;
        Ok(())
    }

    /// Get an indexed root by path
    pub fn get_indexed_root(&self, path: &str) -> Result<Option<IndexedRoot>, CacheError> {
        let state = self.store.read();
        Ok(state.get_indexed_root(path).cloned())
    }

    /// List all indexed roots
    pub fn list_indexed_roots(&self) -> Result<Vec<(String, IndexedRoot)>, CacheError> {
        let state = self.store.read();
        Ok(state.list_indexed_roots().into_iter().map(|(k, v)| (k.to_string(), v.clone())).collect())
    }

    /// Remove an indexed root
    pub fn remove_indexed_root(&self, path: &str) -> Result<Option<IndexedRoot>, CacheError> {
        let removed = {
            let state = self.store.read();
            state.get_indexed_root(path).cloned()
        };
        if removed.is_some() {
            self.store.write(|tx| {
                tx.delete_indexed_root(path);
                Ok(())
            }).map_err(|e| CacheError::Etch(e.to_string()))?;
        }
        Ok(removed)
    }

    // ==================== Erase Job Operations ====================

    /// Create a new erase job (auto-assigns ID)
    pub fn create_erase_job(&self, job: EraseJob) -> Result<EraseJob, CacheError> {
        let created = self.store.write(|tx| {
            let mut state_clone = tx.state.clone();
            let j = state_clone.create_erase_job(job);
            tx.put_erase_job(&j);
            Ok(j)
        }).map_err(|e| CacheError::Etch(e.to_string()))?;
        Ok(created)
    }

    /// Get an erase job by ID
    pub fn get_erase_job(&self, id: i64) -> Result<Option<EraseJob>, CacheError> {
        let state = self.store.read();
        Ok(state.get_erase_job(id).cloned())
    }

    /// Find an erase job by BSD device name
    pub fn find_erase_job_by_bsd_name(&self, bsd_name: &str) -> Result<Option<EraseJob>, CacheError> {
        let state = self.store.read();
        Ok(state.find_erase_job_by_bsd_name(bsd_name).cloned())
    }

    /// Find an erase job matching device identifiers
    pub fn find_erase_job_matching(
        &self,
        bsd_name: Option<&str>,
        volume_uuid: Option<&str>,
        serial_number: Option<&str>,
        size_bytes: u64,
    ) -> Result<Option<EraseJob>, CacheError> {
        let state = self.store.read();
        Ok(state.find_erase_job_matching(bsd_name, volume_uuid, serial_number, size_bytes).cloned())
    }

    /// Find an erase job by volume name or mount point
    pub fn find_erase_job_by_volume_name_or_mount_point(
        &self,
        name: &str,
    ) -> Result<Option<EraseJob>, CacheError> {
        let state = self.store.read();
        Ok(state.find_erase_job_by_volume_name_or_mount_point(name).cloned())
    }

    /// Update an erase job
    pub fn update_erase_job(&self, job: &EraseJob) -> Result<(), CacheError> {
        self.store.write(|tx| {
            tx.put_erase_job(job);
            Ok(())
        }).map_err(|e| CacheError::Etch(e.to_string()))?;
        Ok(())
    }

    /// Delete an erase job
    pub fn delete_erase_job(&self, id: i64) -> Result<(), CacheError> {
        self.store.write(|tx| {
            tx.delete_erase_job(id);
            Ok(())
        }).map_err(|e| CacheError::Etch(e.to_string()))?;
        Ok(())
    }

    /// List all erase jobs
    pub fn list_erase_jobs(&self) -> Result<Vec<EraseJob>, CacheError> {
        let state = self.store.read();
        Ok(state.list_erase_jobs().into_iter().cloned().collect())
    }

    /// List active (incomplete) erase jobs
    pub fn list_active_erase_jobs(&self) -> Result<Vec<EraseJob>, CacheError> {
        let state = self.store.read();
        Ok(state.list_active_erase_jobs().into_iter().cloned().collect())
    }
}

// =============================================================================
// StorageDb - Per-storage file hashes (etch-backed)
// =============================================================================

/// Per-storage database for file hashes, backed by etch `Store<StorageState>`.
pub struct StorageDb {
    store: etchdb::Store<StorageState, etchdb::WalBackend<StorageState>>,
    db_path: PathBuf,
}

impl StorageDb {
    /// Open a storage database at a specific path (directory for etch store)
    pub fn open_at(db_path: &Path) -> Result<Self, CacheError> {
        let store = open_storage_store(db_path)
            .map_err(|e| CacheError::Etch(e.to_string()))?;
        Ok(Self {
            store,
            db_path: db_path.to_path_buf(),
        })
    }

    /// Open an in-memory storage database (for testing)
    pub fn open_memory() -> Result<Self, CacheError> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let base = std::env::temp_dir().join(format!(
            "zero_storage_test_{}_{}",
            std::process::id(),
            id,
        ));
        let store = open_storage_store(&base)
            .map_err(|e| CacheError::Etch(e.to_string()))?;
        Ok(Self {
            store,
            db_path: base,
        })
    }

    /// Get the database file path
    pub fn path(&self) -> &Path {
        &self.db_path
    }

    /// Get the database path
    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    // ==================== File Operations ====================

    /// Lookup a cached file by path
    pub fn lookup_file(&self, path: &str) -> Result<Option<CachedFileEntry>, CacheError> {
        let state = self.store.read();
        Ok(state.get(path).cloned())
    }

    /// Lookup a cached file only if valid (size+mtime match)
    pub fn lookup_valid_file(
        &self,
        path: &str,
        size: i64,
        mtime: i64,
    ) -> Result<Option<CachedFileEntry>, CacheError> {
        let state = self.store.read();
        Ok(state.get_valid(path, size, mtime).cloned())
    }

    /// Insert or update a file entry
    pub fn upsert_file(&self, entry: &CacheEntry) -> Result<(), CacheError> {
        let file_entry = cache_entry_to_file_entry(entry);
        self.store.write(|tx| {
            tx.put(&file_entry);
            Ok(())
        }).map_err(|e| CacheError::Etch(e.to_string()))?;
        Ok(())
    }

    /// Delete a file entry
    pub fn delete_file(&self, path: &str) -> Result<(), CacheError> {
        self.store.write(|tx| {
            tx.delete(path);
            Ok(())
        }).map_err(|e| CacheError::Etch(e.to_string()))?;
        Ok(())
    }

    /// Count all files
    pub fn count_files(&self) -> Result<i64, CacheError> {
        let state = self.store.read();
        Ok(state.file_count())
    }

    /// Get total bytes of all files
    pub fn total_bytes(&self) -> Result<i64, CacheError> {
        let state = self.store.read();
        Ok(state.total_bytes())
    }

    /// Find files by XXH3 hash
    pub fn find_by_hash_xxh3(&self, hash: &[u8]) -> Result<Vec<CachedFileEntry>, CacheError> {
        let state = self.store.read();
        Ok(state.find_by_hash_xxh3(hash).into_iter().cloned().collect())
    }

    /// Find files by Blake3 hash
    pub fn find_by_hash_blake3(&self, hash: &[u8]) -> Result<Vec<CachedFileEntry>, CacheError> {
        let state = self.store.read();
        Ok(state.find_by_hash_blake3(hash).into_iter().cloned().collect())
    }

    /// Find files by size (for dedup pre-filter)
    pub fn find_by_size(&self, size: i64) -> Result<Vec<CachedFileEntry>, CacheError> {
        let state = self.store.read();
        Ok(state.find_by_size(size).into_iter().cloned().collect())
    }

    /// List all files (paginated)
    pub fn list_files(&self, limit: i64, offset: i64) -> Result<Vec<CachedFileEntry>, CacheError> {
        let state = self.store.read();
        Ok(state.list(limit as usize, offset as usize).into_iter().cloned().collect())
    }

    /// Prune entries for files that no longer exist
    pub fn prune_missing(&self, storage_path: &Path) -> Result<Vec<String>, CacheError> {
        // First, find missing paths by reading state
        let missing: Vec<String> = {
            let state = self.store.read();
            state.iter()
                .filter(|(_, entry)| !storage_path.join(&entry.path).exists())
                .map(|(path, _)| path.clone())
                .collect()
        };

        // Then delete them in a write transaction
        if !missing.is_empty() {
            let missing_clone = missing.clone();
            self.store.write(|tx| {
                for path in &missing_clone {
                    tx.delete(path);
                }
                Ok(())
            }).map_err(|e| CacheError::Etch(e.to_string()))?;
        }

        Ok(missing)
    }

    /// Get storage statistics
    pub fn get_stats(&self) -> Result<StorageStats, CacheError> {
        let state = self.store.read();
        Ok(StorageStats {
            file_count: state.file_count(),
            total_bytes: state.total_bytes(),
            hashed_count: state.hashed_count(),
        })
    }

    // ==================== Batch Operations ====================

    /// Create a new batch writer
    pub fn new_batch(&self) -> ChecksumBatch {
        ChecksumBatch::new()
    }

    /// Create a new batch writer with custom size
    pub fn new_batch_with_size(&self, batch_size: usize) -> ChecksumBatch {
        ChecksumBatch::with_batch_size(batch_size)
    }

    /// Flush a batch to the database
    pub fn flush_batch(&self, batch: &mut ChecksumBatch) -> Result<usize, CacheError> {
        let entries = batch.take_entries();
        if entries.is_empty() {
            return Ok(0);
        }
        let count = entries.len();
        self.store.write(|tx| {
            for entry in &entries {
                let file_entry = cache_entry_to_file_entry(entry);
                tx.put(&file_entry);
            }
            Ok(())
        }).map_err(|e| CacheError::Etch(e.to_string()))?;
        Ok(count)
    }
}

// =============================================================================
// CacheManager - Convenience wrapper managing multiple storage DBs
// =============================================================================

/// Manages control database and multiple storage databases
pub struct CacheManager {
    control: ControlDb,
    storage_dbs: Arc<std::sync::RwLock<HashMap<i64, Arc<StorageDb>>>>,
}

impl CacheManager {
    /// Create a new cache manager
    pub fn open() -> Result<Self, CacheError> {
        let control = ControlDb::open()?;
        Ok(Self {
            control,
            storage_dbs: Arc::new(std::sync::RwLock::new(HashMap::new())),
        })
    }

    /// Create a new cache manager at a specific directory
    pub fn open_at(db_dir: &Path) -> Result<Self, CacheError> {
        let control = ControlDb::open_at(db_dir)?;
        Ok(Self {
            control,
            storage_dbs: Arc::new(std::sync::RwLock::new(HashMap::new())),
        })
    }

    /// Get the control database
    pub fn control(&self) -> &ControlDb {
        &self.control
    }

    /// Get or open a storage database by storage ID
    pub fn storage_db(&self, storage_id: i64) -> Result<Arc<StorageDb>, CacheError> {
        // Check if already open
        {
            let cache = self.storage_dbs.read().unwrap();
            if let Some(db) = cache.get(&storage_id) {
                return Ok(Arc::clone(db));
            }
        }

        // Open the database
        let storage = self
            .control
            .get_storage_by_id(storage_id)?
            .ok_or_else(|| CacheError::StorageNotFound(format!("id={}", storage_id)))?;

        let db = self.control.open_storage_db(&storage)?;
        let db = Arc::new(db);

        // Cache it
        {
            let mut cache = self.storage_dbs.write().unwrap();
            cache.insert(storage_id, Arc::clone(&db));
        }

        Ok(db)
    }

    /// Get or create a storage and its database
    pub fn get_or_create_storage(
        &self,
        path: &Path,
        device_info: Option<DeviceInfo>,
    ) -> Result<(Storage, Arc<StorageDb>), CacheError> {
        let storage = self.control.get_or_create_storage(path, device_info)?;
        let db = self.storage_db(storage.id)?;
        Ok((storage, db))
    }

    /// Close a cached storage database
    pub fn close_storage_db(&self, storage_id: i64) {
        let mut cache = self.storage_dbs.write().unwrap();
        cache.remove(&storage_id);
    }

    /// Close all cached storage databases
    pub fn close_all_storage_dbs(&self) {
        let mut cache = self.storage_dbs.write().unwrap();
        cache.clear();
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Get the default database directory (`~/.zero/db/`)
fn default_db_dir() -> PathBuf {
    crate::dirs::db_dir().unwrap_or_else(|| PathBuf::from("."))
}

/// Check if a cached file is still valid based on size and mtime
pub fn is_cache_valid(cached: &CachedFileEntry, size: i64, mtime: i64) -> bool {
    cached.is_valid_for(size, mtime)
}

/// Check if a cached file is valid for filesystem metadata
pub fn is_cache_valid_for_metadata(cached: &CachedFileEntry, metadata: &std::fs::Metadata) -> bool {
    cached.is_valid_for_metadata(metadata)
}

// =============================================================================
// Legacy compatibility - CacheDb alias
// =============================================================================

/// Legacy alias for ControlDb (for backward compatibility during migration)
pub type CacheDb = ControlDb;

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
#[path = "db_test.rs"]
mod tests;
