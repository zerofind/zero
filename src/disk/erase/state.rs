//! Erase state persistence for resume functionality
//!
//! This module handles saving and loading erase operation state to enable
//! resuming interrupted wipes. State is persisted in ControlDb (etch-backed)
//! for crash safety and single-source-of-truth.

use std::io;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::cache::{
    ControlDb, EraseDeviceId, EraseJob, EraseJobProgress, EraseJobSeed, EraseJobSettings,
};
use crate::util::format_bytes;

use super::schemes::EraseLevel;
use super::wipe::Verify;

/// Type aliases for backward compatibility
pub type DeviceIdentifier = EraseDeviceId;
pub type EraseSettings = EraseJobSettings;
pub type EraseProgressState = EraseJobProgress;
pub type StageSeed = EraseJobSeed;

/// Persistent state for an erase operation, backed by ControlDb
#[derive(Debug, Clone)]
pub struct EraseState {
    job: EraseJob,
}

impl EraseState {
    /// Create a new erase state for a fresh operation
    pub fn new(
        device: DeviceIdentifier,
        level: EraseLevel,
        verify: Verify,
        block_size: usize,
    ) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let scheme = level.scheme();

        Self {
            job: EraseJob {
                id: 0, // assigned on first save
                version: 1,
                device,
                settings: EraseJobSettings {
                    level: level.name().to_string(),
                    total_passes: scheme.pass_count(),
                    verify: format!("{:?}", verify),
                    block_size,
                },
                progress: EraseJobProgress {
                    current_stage: 0,
                    stage_position: 0,
                    in_verification: false,
                    stage_seeds: Vec::new(),
                    completed_stages: Vec::new(),
                    total_bytes_written: 0,
                },
                updated_at: now,
                started_at: now,
            },
        }
    }

    /// Wrap an existing EraseJob
    fn from_job(job: EraseJob) -> Self {
        Self { job }
    }

    // ==================== Accessors ====================

    pub fn version(&self) -> u32 {
        self.job.version
    }

    pub fn device(&self) -> &DeviceIdentifier {
        &self.job.device
    }

    pub fn settings(&self) -> &EraseSettings {
        &self.job.settings
    }

    pub fn progress(&self) -> &EraseProgressState {
        &self.job.progress
    }

    pub fn updated_at(&self) -> u64 {
        self.job.updated_at
    }

    pub fn started_at(&self) -> u64 {
        self.job.started_at
    }

    // ==================== Domain Methods ====================

    /// Update progress
    pub fn update_progress(
        &mut self,
        stage: usize,
        position: u64,
        in_verification: bool,
        total_written: u64,
    ) {
        self.job.progress.current_stage = stage;
        self.job.progress.stage_position = position;
        self.job.progress.in_verification = in_verification;
        self.job.progress.total_bytes_written = total_written;
        self.job.updated_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
    }

    /// Record a seed used for a random stage
    pub fn record_seed(&mut self, stage: usize, seed: [u8; 32]) {
        self.job.progress.stage_seeds.retain(|s| s.stage != stage);
        self.job
            .progress
            .stage_seeds
            .push(EraseJobSeed { stage, seed });
    }

    /// Get the seed for a stage (if recorded)
    pub fn get_seed(&self, stage: usize) -> Option<[u8; 32]> {
        self.job
            .progress
            .stage_seeds
            .iter()
            .find(|s| s.stage == stage)
            .map(|s| s.seed)
    }

    /// Mark a stage as completed
    pub fn mark_stage_complete(&mut self, stage: usize) {
        if !self.job.progress.completed_stages.contains(&stage) {
            self.job.progress.completed_stages.push(stage);
        }
        self.job.progress.stage_position = 0;
        self.job.progress.in_verification = false;
    }

    /// Check if a stage is completed
    pub fn is_stage_complete(&self, stage: usize) -> bool {
        self.job.progress.completed_stages.contains(&stage)
    }

    /// Check if we can resume from this state for the given device
    pub fn can_resume_for(&self, bsd_name: &str, size_bytes: u64, level: &str) -> bool {
        if self.job.device.bsd_name != bsd_name {
            return false;
        }
        if self.job.device.size_bytes != size_bytes {
            return false;
        }
        if self.job.settings.level != level {
            return false;
        }
        if self.job.progress.completed_stages.len() >= self.job.settings.total_passes {
            return false;
        }
        true
    }

    /// Check if a device currently has a different volume name than when erase started
    pub fn device_has_new_label(&self, current_volume_name: Option<&str>) -> bool {
        match (&self.job.device.volume_name, current_volume_name) {
            (Some(original), Some(current)) if original != current => true,
            (None, Some(_)) => true,
            _ => false,
        }
    }

    /// Get elapsed time since the erase started, in seconds
    pub fn elapsed_seconds(&self) -> u64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now.saturating_sub(self.job.started_at)
    }

    /// Check if this is a recent erase (within the given number of hours)
    pub fn is_recent(&self, hours: u64) -> bool {
        self.elapsed_seconds() < hours * 3600
    }

    /// Get a display-friendly summary of the state
    pub fn summary(&self) -> String {
        let completed = self.job.progress.completed_stages.len();
        let total = self.job.settings.total_passes;
        let percent = if self.job.device.size_bytes > 0 {
            let stage_progress =
                self.job.progress.stage_position as f64 / self.job.device.size_bytes as f64;
            let overall = (completed as f64 + stage_progress) / total as f64 * 100.0;
            format!("{:.1}%", overall)
        } else {
            "0%".to_string()
        };

        format!(
            "Stage {}/{}, Position {}, Overall: {}",
            self.job.progress.current_stage + 1,
            total,
            format_bytes(self.job.progress.stage_position),
            percent
        )
    }

    // ==================== Persistence (ControlDb) ====================

    /// Save state to ControlDb
    pub fn save(&mut self, db: &ControlDb) -> io::Result<()> {
        if self.job.id == 0 {
            // New job — create in ControlDb (auto-assigns ID)
            self.job = db
                .create_erase_job(self.job.clone())
                .map_err(|e| io::Error::other(e.to_string()))?;
        } else {
            db.update_erase_job(&self.job)
                .map_err(|e| io::Error::other(e.to_string()))?;
        }
        Ok(())
    }

    /// Load state from ControlDb for a device
    pub fn load(db: &ControlDb, bsd_name: &str) -> io::Result<Option<Self>> {
        db.find_erase_job_by_bsd_name(bsd_name)
            .map(|opt| opt.map(Self::from_job))
            .map_err(|e| io::Error::other(e.to_string()))
    }

    /// Delete state from ControlDb for a device
    pub fn delete(db: &ControlDb, bsd_name: &str) -> io::Result<()> {
        if let Some(job) = db
            .find_erase_job_by_bsd_name(bsd_name)
            .map_err(|e| io::Error::other(e.to_string()))?
        {
            db.delete_erase_job(job.id)
                .map_err(|e| io::Error::other(e.to_string()))?;
        }
        Ok(())
    }

    /// Find any existing state that matches a device by various identifiers
    pub fn find_matching_state(
        db: &ControlDb,
        bsd_name: Option<&str>,
        volume_uuid: Option<&str>,
        serial_number: Option<&str>,
        size_bytes: u64,
    ) -> io::Result<Option<Self>> {
        db.find_erase_job_matching(bsd_name, volume_uuid, serial_number, size_bytes)
            .map(|opt| opt.map(Self::from_job))
            .map_err(|e| io::Error::other(e.to_string()))
    }

    /// Find pending erase state by volume name or mount point
    pub fn find_by_volume_name_or_mount_point(
        db: &ControlDb,
        name_or_path: &str,
    ) -> io::Result<Option<Self>> {
        db.find_erase_job_by_volume_name_or_mount_point(name_or_path)
            .map(|opt| opt.map(Self::from_job))
            .map_err(|e| io::Error::other(e.to_string()))
    }
}

impl DeviceIdentifier {
    /// Create from volume info
    pub fn from_volume_info(
        bsd_name: &str,
        volume_uuid: Option<&str>,
        serial_number: Option<&str>,
        size_bytes: u64,
        volume_name: Option<&str>,
        mount_point: Option<&str>,
    ) -> Self {
        Self {
            bsd_name: bsd_name.to_string(),
            volume_uuid: volume_uuid.map(|s| s.to_string()),
            serial_number: serial_number.map(|s| s.to_string()),
            size_bytes,
            volume_name: volume_name.map(|s| s.to_string()),
            mount_point: mount_point.map(|s| s.to_string()),
        }
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
