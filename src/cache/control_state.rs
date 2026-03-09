//! Etch-backed persistence for the control database
//!
//! Uses `Store<ControlState>` for settings, automations, runs, storage registry,
//! indexed roots, and erase jobs.
//!
//! WAL collections:
//! - 0: storages (key = i64 id)
//! - 1: automations (key = i64 id)
//! - 2: runs (key = i64 id)
//! - 3: indexed_roots (key = root path as bytes)
//! - 4: erase_jobs (key = i64 id)

use std::collections::BTreeMap;
use std::path::Path;

use etchdb::{EtchKey, Op, Replayable, Store, Transactable, WalBackend};
use serde::{Deserialize, Serialize};

use super::automations::{Automation, NewAutomation};
use super::runs::{Run, RunResult, RunStatus, TriggerType};
use super::storages::{DeviceInfo, Storage, StorageIdentifier};
use crate::util::now_timestamp;

const STORAGES: u8 = 0;
const AUTOMATIONS: u8 = 1;
const RUNS: u8 = 2;
const INDEXED_ROOTS: u8 = 3;
const ERASE_JOBS: u8 = 4;
const FILE_USAGE: u8 = 5;

// ==================== IndexedRoot ====================

/// Metadata for a single indexed root directory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedRoot {
    /// SHA-256 hash of root path, used as etch directory name
    pub hash: String,
    /// Number of files in this index
    pub file_count: usize,
    /// Number of directories in this index
    pub dir_count: usize,
    /// Total bytes of indexed files
    pub total_bytes: u64,
    /// When this index was last built (Unix timestamp)
    pub last_indexed: u64,
}

// ==================== EraseJob ====================

/// Persistent state for an erase operation stored in ControlDb
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EraseJob {
    pub id: i64,
    pub version: u32,
    pub device: EraseDeviceId,
    pub settings: EraseJobSettings,
    pub progress: EraseJobProgress,
    pub updated_at: u64,
    pub started_at: u64,
}

/// Device identification for an erase job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EraseDeviceId {
    pub bsd_name: String,
    pub volume_uuid: Option<String>,
    pub serial_number: Option<String>,
    pub size_bytes: u64,
    pub volume_name: Option<String>,
    pub mount_point: Option<String>,
}

/// Erase settings that must match for resume
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EraseJobSettings {
    pub level: String,
    pub total_passes: usize,
    pub verify: String,
    pub block_size: usize,
}

/// Progress state for resuming an erase
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EraseJobProgress {
    pub current_stage: usize,
    pub stage_position: u64,
    pub in_verification: bool,
    pub stage_seeds: Vec<EraseJobSeed>,
    pub completed_stages: Vec<usize>,
    pub total_bytes_written: u64,
}

/// Seed information for a random erase stage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EraseJobSeed {
    pub stage: usize,
    pub seed: [u8; 32],
}

/// Etch state holding all control data
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct ControlState {
    pub storages: BTreeMap<i64, Storage>,
    pub automations: BTreeMap<i64, Automation>,
    pub runs: BTreeMap<i64, Run>,
    pub indexed_roots: BTreeMap<String, IndexedRoot>,
    pub erase_jobs: BTreeMap<i64, EraseJob>,
    #[serde(default)]
    pub usage_store: super::usage::UsageStore,

    next_storage_id: i64,
    next_automation_id: i64,
    next_run_id: i64,
    next_erase_job_id: i64,
}

impl ControlState {
    // ==================== Storage Operations ====================

    /// Get or create a storage entry
    pub fn get_or_create_storage(
        &mut self,
        path: &Path,
        device_info: Option<DeviceInfo>,
    ) -> Storage {
        let info = device_info.unwrap_or_default();
        let identifier = info.to_identifier(path);

        // Try to find existing
        if let Some(storage) = self.find_storage_by_identifier(&identifier) {
            let id = storage.id;
            // Update last seen
            if let Some(s) = self.storages.get_mut(&id) {
                s.last_mount_path = Some(path.to_string_lossy().to_string());
                s.last_seen_at = Some(now_timestamp());
            }
            return self.storages[&id].clone();
        }

        // Create new
        self.next_storage_id += 1;
        let id = self.next_storage_id;
        let now = now_timestamp();
        let db_filename = identifier.db_filename();
        let path_str = path.to_string_lossy().to_string();

        let (device_serial, device_uuid, path_hash) = match &identifier {
            StorageIdentifier::DeviceSerial(s) => (Some(s.clone()), None, None),
            StorageIdentifier::DeviceUuid(u) => (None, Some(u.clone()), None),
            StorageIdentifier::PathHash(h) => (None, None, Some(h.clone())),
        };

        let storage = Storage {
            id,
            device_serial,
            device_uuid,
            path_hash,
            display_name: info.display_name,
            last_mount_path: Some(path_str),
            mount_type: info.mount_type,
            db_filename,
            file_count: 0,
            total_bytes: 0,
            last_seen_at: Some(now),
            created_at: now,
        };

        self.storages.insert(id, storage.clone());
        storage
    }

    /// Find storage by identifier (serial > uuid > path_hash)
    fn find_storage_by_identifier(&self, identifier: &StorageIdentifier) -> Option<&Storage> {
        match identifier {
            StorageIdentifier::DeviceSerial(s) => self
                .storages
                .values()
                .find(|st| st.device_serial.as_deref() == Some(s)),
            StorageIdentifier::DeviceUuid(u) => self
                .storages
                .values()
                .find(|st| st.device_uuid.as_deref() == Some(u)),
            StorageIdentifier::PathHash(h) => self
                .storages
                .values()
                .find(|st| st.path_hash.as_deref() == Some(h)),
        }
    }

    /// Get a storage by ID
    pub fn get_storage_by_id(&self, id: i64) -> Option<&Storage> {
        self.storages.get(&id)
    }

    /// Get a storage by serial
    pub fn get_storage_by_serial(&self, serial: &str) -> Option<&Storage> {
        self.storages
            .values()
            .find(|s| s.device_serial.as_deref() == Some(serial))
    }

    /// Get a storage by UUID
    pub fn get_storage_by_uuid(&self, uuid: &str) -> Option<&Storage> {
        self.storages
            .values()
            .find(|s| s.device_uuid.as_deref() == Some(uuid))
    }

    /// Get a storage by path
    pub fn get_storage_by_path(&self, path: &Path) -> Option<&Storage> {
        let identifier = StorageIdentifier::from_path(path);
        self.find_storage_by_identifier(&identifier)
    }

    /// Get a storage by identifier
    pub fn get_storage_by_identifier(&self, identifier: &StorageIdentifier) -> Option<&Storage> {
        self.find_storage_by_identifier(identifier)
    }

    /// List all storages
    pub fn list_storages(&self) -> Vec<&Storage> {
        self.storages.values().collect()
    }

    /// Update storage stats
    pub fn update_storage_stats(&mut self, id: i64, file_count: i64, total_bytes: i64) {
        if let Some(s) = self.storages.get_mut(&id) {
            s.file_count = file_count;
            s.total_bytes = total_bytes;
        }
    }

    /// Delete a storage, returns its db_filename
    pub fn delete_storage(&mut self, id: i64) -> Option<String> {
        self.storages.remove(&id).map(|s| s.db_filename)
    }

    // ==================== Automation Operations ====================

    /// Create a new automation
    pub fn create_automation(&mut self, new: NewAutomation) -> Automation {
        self.next_automation_id += 1;
        let id = self.next_automation_id;
        let now = now_timestamp();

        let automation = Automation {
            id,
            name: new.name,
            dest_device_serial: new.dest_device_serial,
            dest_volume_name: new.dest_volume_name,
            dest_path: new.dest_path,
            triggers: new.triggers,
            paths: new.paths,
            settings: new.settings,
            enabled: true,
            created_at: now,
            updated_at: now,
        };

        self.automations.insert(id, automation.clone());
        automation
    }

    /// Get an automation by ID
    pub fn get_automation(&self, id: i64) -> Option<&Automation> {
        self.automations.get(&id)
    }

    /// List all automations
    pub fn list_automations(&self) -> Vec<&Automation> {
        let mut list: Vec<_> = self.automations.values().collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        list
    }

    /// List enabled automations
    pub fn list_enabled_automations(&self) -> Vec<&Automation> {
        let mut list: Vec<_> = self.automations.values().filter(|a| a.enabled).collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        list
    }

    /// Find automations by device serial (on_mount trigger)
    pub fn find_automations_by_serial(&self, serial: &str) -> Vec<&Automation> {
        self.automations
            .values()
            .filter(|a| {
                a.enabled && a.dest_device_serial.as_deref() == Some(serial) && a.triggers.on_mount
            })
            .collect()
    }

    /// Find automations by watch path (on_change trigger)
    pub fn find_automations_by_watch_path(&self, path: &str) -> Vec<&Automation> {
        self.automations
            .values()
            .filter(|a| {
                a.enabled
                    && a.triggers.on_change
                    && a.paths
                        .iter()
                        .any(|p| p.source == path || path.starts_with(&format!("{}/", p.source)))
            })
            .collect()
    }

    /// Update an automation
    pub fn update_automation(&mut self, id: i64, new: NewAutomation) {
        if let Some(a) = self.automations.get_mut(&id) {
            a.name = new.name;
            a.dest_device_serial = new.dest_device_serial;
            a.dest_volume_name = new.dest_volume_name;
            a.dest_path = new.dest_path;
            a.triggers = new.triggers;
            a.paths = new.paths;
            a.settings = new.settings;
            a.updated_at = now_timestamp();
        }
    }

    /// Enable or disable an automation
    pub fn set_automation_enabled(&mut self, id: i64, enabled: bool) {
        if let Some(a) = self.automations.get_mut(&id) {
            a.enabled = enabled;
            a.updated_at = now_timestamp();
        }
    }

    /// Delete an automation and its runs
    pub fn delete_automation(&mut self, id: i64) {
        self.automations.remove(&id);
        self.runs.retain(|_, r| r.automation_id != id);
    }

    // ==================== Run Operations ====================

    /// Start a new run
    pub fn start_run(
        &mut self,
        automation_id: i64,
        trigger: TriggerType,
        initial_progress: Option<super::runs::Progress>,
    ) -> Run {
        self.next_run_id += 1;
        let id = self.next_run_id;
        let now = now_timestamp();

        let run = Run {
            id,
            automation_id,
            started_at: now,
            completed_at: None,
            status: RunStatus::Running,
            trigger: Some(trigger),
            progress: initial_progress,
            result: None,
            resumable: false,
            resume_state: None,
        };

        self.runs.insert(id, run.clone());
        run
    }

    /// Get a run by ID
    pub fn get_run(&self, id: i64) -> Option<&Run> {
        self.runs.get(&id)
    }

    /// Get the latest run for an automation
    pub fn get_latest_run(&self, automation_id: i64) -> Option<&Run> {
        self.runs
            .values()
            .filter(|r| r.automation_id == automation_id)
            .max_by_key(|r| r.started_at)
    }

    /// List runs for an automation
    pub fn list_runs_for_automation(&self, automation_id: i64, limit: usize) -> Vec<&Run> {
        let mut runs: Vec<_> = self
            .runs
            .values()
            .filter(|r| r.automation_id == automation_id)
            .collect();
        runs.sort_by_key(|b| std::cmp::Reverse(b.started_at));
        runs.truncate(limit);
        runs
    }

    /// Find running run for an automation
    pub fn find_running_run(&self, automation_id: i64) -> Option<&Run> {
        self.runs
            .values()
            .find(|r| r.automation_id == automation_id && r.status == RunStatus::Running)
    }

    /// Find all resumable runs
    pub fn find_resumable_runs(&self) -> Vec<&Run> {
        let mut runs: Vec<_> = self.runs.values().filter(|r| r.resumable).collect();
        runs.sort_by_key(|b| std::cmp::Reverse(b.started_at));
        runs
    }

    /// Update run progress
    pub fn update_run_progress(&mut self, id: i64, progress: &super::runs::Progress) {
        if let Some(r) = self.runs.get_mut(&id) {
            r.progress = Some(progress.clone());
        }
    }

    /// Complete a run with success
    pub fn complete_run_success(&mut self, id: i64, result: &RunResult) {
        if let Some(r) = self.runs.get_mut(&id) {
            r.status = RunStatus::Success;
            r.completed_at = Some(now_timestamp());
            r.result = Some(result.clone());
            r.resumable = false;
        }
    }

    /// Complete a run with failure
    pub fn complete_run_failed(&mut self, id: i64, result: &RunResult) {
        if let Some(r) = self.runs.get_mut(&id) {
            r.status = RunStatus::Failed;
            r.completed_at = Some(now_timestamp());
            r.result = Some(result.clone());
            r.resumable = false;
        }
    }

    /// Mark a run as partial (interrupted but resumable)
    pub fn mark_run_partial(&mut self, id: i64, result: &RunResult, resume_state: Option<&str>) {
        if let Some(r) = self.runs.get_mut(&id) {
            r.status = RunStatus::Partial;
            r.completed_at = Some(now_timestamp());
            r.result = Some(result.clone());
            r.resumable = true;
            r.resume_state = resume_state.map(|s| s.to_string());
        }
    }

    /// Cancel a run
    pub fn cancel_run(&mut self, id: i64) {
        if let Some(r) = self.runs.get_mut(&id) {
            r.status = RunStatus::Cancelled;
            r.completed_at = Some(now_timestamp());
            r.resumable = false;
        }
    }

    /// Clear resumable flag
    pub fn clear_run_resumable(&mut self, id: i64) {
        if let Some(r) = self.runs.get_mut(&id) {
            r.resumable = false;
            r.resume_state = None;
        }
    }

    /// Mark all running runs as interrupted (startup recovery)
    pub fn mark_interrupted_on_startup(&mut self) -> u64 {
        let now = now_timestamp();
        let mut count = 0u64;
        for r in self.runs.values_mut() {
            if r.status == RunStatus::Running {
                r.status = RunStatus::Partial;
                r.completed_at = Some(now);
                r.resumable = true;
                count += 1;
            }
        }
        count
    }

    /// Delete runs older than N days
    pub fn delete_runs_older_than(&mut self, days: i64) -> u64 {
        let cutoff = now_timestamp() - (days * 24 * 60 * 60);
        let before = self.runs.len();
        self.runs.retain(|_, r| {
            r.status == RunStatus::Running || r.completed_at.is_none_or(|t| t >= cutoff)
        });
        (before - self.runs.len()) as u64
    }

    /// Delete all runs for an automation
    pub fn delete_runs_for_automation(&mut self, automation_id: i64) -> u64 {
        let before = self.runs.len();
        self.runs.retain(|_, r| r.automation_id != automation_id);
        (before - self.runs.len()) as u64
    }

    // ==================== Indexed Root Operations ====================

    /// Insert or update an indexed root entry
    pub fn upsert_indexed_root(&mut self, path: &str, root: IndexedRoot) {
        self.indexed_roots.insert(path.to_string(), root);
    }

    /// Get an indexed root by path
    pub fn get_indexed_root(&self, path: &str) -> Option<&IndexedRoot> {
        self.indexed_roots.get(path)
    }

    /// List all indexed roots
    pub fn list_indexed_roots(&self) -> Vec<(&str, &IndexedRoot)> {
        self.indexed_roots
            .iter()
            .map(|(k, v)| (k.as_str(), v))
            .collect()
    }

    /// Remove an indexed root, returning it if it existed
    pub fn remove_indexed_root(&mut self, path: &str) -> Option<IndexedRoot> {
        self.indexed_roots.remove(path)
    }

    // ==================== Erase Job Operations ====================

    /// Create a new erase job with auto-assigned ID
    pub fn create_erase_job(&mut self, mut job: EraseJob) -> EraseJob {
        self.next_erase_job_id += 1;
        job.id = self.next_erase_job_id;
        self.erase_jobs.insert(job.id, job.clone());
        job
    }

    /// Get an erase job by ID
    pub fn get_erase_job(&self, id: i64) -> Option<&EraseJob> {
        self.erase_jobs.get(&id)
    }

    /// Find an erase job by BSD device name
    pub fn find_erase_job_by_bsd_name(&self, bsd_name: &str) -> Option<&EraseJob> {
        self.erase_jobs
            .values()
            .find(|j| j.device.bsd_name == bsd_name)
    }

    /// Find an erase job matching device identifiers
    pub fn find_erase_job_matching(
        &self,
        bsd_name: Option<&str>,
        volume_uuid: Option<&str>,
        serial_number: Option<&str>,
        size_bytes: u64,
    ) -> Option<&EraseJob> {
        self.erase_jobs.values().find(|j| {
            let matches = (bsd_name.is_some() && bsd_name == Some(&j.device.bsd_name))
                || (volume_uuid.is_some() && volume_uuid == j.device.volume_uuid.as_deref())
                || (serial_number.is_some() && serial_number == j.device.serial_number.as_deref());
            matches && j.device.size_bytes == size_bytes
        })
    }

    /// Find an erase job by volume name or mount point (for resume after unmount)
    pub fn find_erase_job_by_volume_name_or_mount_point(
        &self,
        name_or_path: &str,
    ) -> Option<&EraseJob> {
        let volume_name = if let Some(stripped) = name_or_path.strip_prefix("/Volumes/") {
            stripped
        } else {
            name_or_path
        };

        self.erase_jobs.values().find(|j| {
            let matches_name = j.device.volume_name.as_deref() == Some(volume_name);
            let matches_mount = j.device.mount_point.as_deref() == Some(name_or_path);
            // Only match incomplete jobs
            (matches_name || matches_mount)
                && j.progress.completed_stages.len() < j.settings.total_passes
        })
    }

    /// Update an erase job in-place
    pub fn update_erase_job(&mut self, job: &EraseJob) {
        self.erase_jobs.insert(job.id, job.clone());
    }

    /// Delete an erase job
    pub fn delete_erase_job(&mut self, id: i64) {
        self.erase_jobs.remove(&id);
    }

    /// List all erase jobs
    pub fn list_erase_jobs(&self) -> Vec<&EraseJob> {
        self.erase_jobs.values().collect()
    }

    /// List active (incomplete) erase jobs
    pub fn list_active_erase_jobs(&self) -> Vec<&EraseJob> {
        self.erase_jobs
            .values()
            .filter(|j| j.progress.completed_stages.len() < j.settings.total_passes)
            .collect()
    }

    // ==================== Usage Store Operations ====================

    /// Record a file open event for frequency tracking
    pub fn record_file_open(&mut self, path: &str, now: u64) {
        self.usage_store.record_open(path, now);
    }

    /// Get frequency bonus for a file path (0–150)
    pub fn frequency_bonus(&self, path: &str, now: u64) -> u32 {
        self.usage_store.frequency_bonus(path, now)
    }
}

// ==================== Etch Integration ====================

fn i64_from_bytes(key: &[u8]) -> etchdb::Result<i64> {
    i64::from_bytes(key)
}

fn string_from_bytes(key: &[u8]) -> etchdb::Result<String> {
    String::from_utf8(key.to_vec()).map_err(|e| etchdb::Error::WalCorrupted {
        offset: 0,
        reason: format!("bad utf8 key: {e}"),
    })
}

impl Replayable for ControlState {
    fn apply(&mut self, ops: &[Op]) -> etchdb::Result<()> {
        for op in ops {
            match op {
                Op::Put {
                    collection,
                    key,
                    value,
                } => match *collection {
                    STORAGES => {
                        let id = i64_from_bytes(key)?;
                        let storage: Storage = postcard::from_bytes(value).map_err(|e| {
                            etchdb::Error::WalCorrupted {
                                offset: 0,
                                reason: format!("bad storage: {e}"),
                            }
                        })?;
                        self.storages.insert(id, storage);
                        if id >= self.next_storage_id {
                            self.next_storage_id = id;
                        }
                    }
                    AUTOMATIONS => {
                        let id = i64_from_bytes(key)?;
                        let automation: Automation = postcard::from_bytes(value).map_err(|e| {
                            etchdb::Error::WalCorrupted {
                                offset: 0,
                                reason: format!("bad automation: {e}"),
                            }
                        })?;
                        self.automations.insert(id, automation);
                        if id >= self.next_automation_id {
                            self.next_automation_id = id;
                        }
                    }
                    RUNS => {
                        let id = i64_from_bytes(key)?;
                        let run: Run = postcard::from_bytes(value).map_err(|e| {
                            etchdb::Error::WalCorrupted {
                                offset: 0,
                                reason: format!("bad run: {e}"),
                            }
                        })?;
                        self.runs.insert(id, run);
                        if id >= self.next_run_id {
                            self.next_run_id = id;
                        }
                    }
                    INDEXED_ROOTS => {
                        let path = string_from_bytes(key)?;
                        let root: IndexedRoot = postcard::from_bytes(value).map_err(|e| {
                            etchdb::Error::WalCorrupted {
                                offset: 0,
                                reason: format!("bad indexed_root: {e}"),
                            }
                        })?;
                        self.indexed_roots.insert(path, root);
                    }
                    ERASE_JOBS => {
                        let id = i64_from_bytes(key)?;
                        let job: EraseJob = postcard::from_bytes(value).map_err(|e| {
                            etchdb::Error::WalCorrupted {
                                offset: 0,
                                reason: format!("bad erase_job: {e}"),
                            }
                        })?;
                        self.erase_jobs.insert(id, job);
                        if id >= self.next_erase_job_id {
                            self.next_erase_job_id = id;
                        }
                    }
                    FILE_USAGE => {
                        let store: super::usage::UsageStore =
                            postcard::from_bytes(value).map_err(|e| {
                                etchdb::Error::WalCorrupted {
                                    offset: 0,
                                    reason: format!("bad usage: {e}"),
                                }
                            })?;
                        self.usage_store = store;
                    }
                    _ => {}
                },
                Op::Delete { collection, key } => match *collection {
                    STORAGES => {
                        let id = i64_from_bytes(key)?;
                        self.storages.remove(&id);
                    }
                    AUTOMATIONS => {
                        let id = i64_from_bytes(key)?;
                        self.automations.remove(&id);
                    }
                    RUNS => {
                        let id = i64_from_bytes(key)?;
                        self.runs.remove(&id);
                    }
                    INDEXED_ROOTS => {
                        let path = string_from_bytes(key)?;
                        self.indexed_roots.remove(&path);
                    }
                    ERASE_JOBS => {
                        let id = i64_from_bytes(key)?;
                        self.erase_jobs.remove(&id);
                    }
                    FILE_USAGE => {
                        self.usage_store = super::usage::UsageStore::new();
                    }
                    _ => {}
                },
            }
        }
        Ok(())
    }
}

/// Transaction overlay for control writes
pub struct ControlOverlay {
    ops: Vec<Op>,
}

/// Transaction handle for control operations
pub struct ControlTx<'a> {
    pub state: &'a ControlState,
    overlay: ControlOverlay,
}

impl<'a> ControlTx<'a> {
    /// Put a storage entry
    pub fn put_storage(&mut self, storage: &Storage) {
        let value = postcard::to_allocvec(storage).expect("Storage serialization");
        self.overlay.ops.push(Op::Put {
            collection: STORAGES,
            key: storage.id.to_bytes(),
            value,
        });
    }

    /// Delete a storage entry
    pub fn delete_storage(&mut self, id: i64) {
        self.overlay.ops.push(Op::Delete {
            collection: STORAGES,
            key: id.to_bytes(),
        });
    }

    /// Put an automation entry
    pub fn put_automation(&mut self, automation: &Automation) {
        let value = postcard::to_allocvec(automation).expect("Automation serialization");
        self.overlay.ops.push(Op::Put {
            collection: AUTOMATIONS,
            key: automation.id.to_bytes(),
            value,
        });
    }

    /// Delete an automation entry
    pub fn delete_automation(&mut self, id: i64) {
        self.overlay.ops.push(Op::Delete {
            collection: AUTOMATIONS,
            key: id.to_bytes(),
        });
    }

    /// Put a run entry
    pub fn put_run(&mut self, run: &Run) {
        let value = postcard::to_allocvec(run).expect("Run serialization");
        self.overlay.ops.push(Op::Put {
            collection: RUNS,
            key: run.id.to_bytes(),
            value,
        });
    }

    /// Delete a run entry
    pub fn delete_run(&mut self, id: i64) {
        self.overlay.ops.push(Op::Delete {
            collection: RUNS,
            key: id.to_bytes(),
        });
    }

    /// Put an indexed root entry (keyed by path)
    pub fn put_indexed_root(&mut self, path: &str, root: &IndexedRoot) {
        let value = postcard::to_allocvec(root).expect("IndexedRoot serialization");
        self.overlay.ops.push(Op::Put {
            collection: INDEXED_ROOTS,
            key: path.as_bytes().to_vec(),
            value,
        });
    }

    /// Delete an indexed root entry
    pub fn delete_indexed_root(&mut self, path: &str) {
        self.overlay.ops.push(Op::Delete {
            collection: INDEXED_ROOTS,
            key: path.as_bytes().to_vec(),
        });
    }

    /// Put an erase job entry
    pub fn put_erase_job(&mut self, job: &EraseJob) {
        let value = postcard::to_allocvec(job).expect("EraseJob serialization");
        self.overlay.ops.push(Op::Put {
            collection: ERASE_JOBS,
            key: job.id.to_bytes(),
            value,
        });
    }

    /// Delete an erase job entry
    pub fn delete_erase_job(&mut self, id: i64) {
        self.overlay.ops.push(Op::Delete {
            collection: ERASE_JOBS,
            key: id.to_bytes(),
        });
    }

    /// Put usage store (entire store as single key)
    pub fn put_usage_store(&mut self, store: &super::usage::UsageStore) {
        let value = postcard::to_allocvec(store).expect("UsageStore serialization");
        self.overlay.ops.push(Op::Put {
            collection: FILE_USAGE,
            key: b"usage".to_vec(),
            value,
        });
    }
}

impl Transactable for ControlState {
    type Tx<'a> = ControlTx<'a>;
    type Overlay = ControlOverlay;

    fn begin_tx(&self) -> ControlTx<'_> {
        ControlTx {
            state: self,
            overlay: ControlOverlay { ops: Vec::new() },
        }
    }

    fn finish_tx(tx: ControlTx<'_>) -> (Vec<Op>, ControlOverlay) {
        let ops = tx.overlay.ops.clone();
        (ops, tx.overlay)
    }

    fn apply_overlay(&mut self, overlay: ControlOverlay) {
        // Replay ops against in-memory state
        let _ = self.apply(&overlay.ops);
    }
}

/// Open an etch store for the control database
pub fn open_control_store(
    dir: &Path,
) -> Result<Store<ControlState, WalBackend<ControlState>>, std::io::Error> {
    std::fs::create_dir_all(dir)?;
    Store::<ControlState, WalBackend<ControlState>>::open_wal(dir.to_path_buf())
        .map_err(|e| std::io::Error::other(e.to_string()))
}
