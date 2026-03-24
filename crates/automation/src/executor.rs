//! Automation executor - handles events and runs automations
//!
//! The executor listens for events (USB mount, file changes) and triggers
//! matching automations, tracking progress and managing run lifecycle.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{Mutex, RwLock};

use cache::{Automation, CacheDb, Run, TriggerType};

use super::types::{AutomationEvent, ExecutorConfig, ExecutorError, SyncCallback};

/// State of a running automation
#[derive(Debug)]
pub(super) struct RunningAutomation {
    pub run_id: i64,
    pub automation_id: i64,
    #[allow(dead_code)]
    pub started_at: Instant,
    pub cancel_tx: crossfire::MAsyncTx<crossfire::mpsc::Array<()>>,
}

/// The automation executor
pub struct Executor {
    /// Database connection
    pub(super) db: Arc<CacheDb>,

    /// Configuration (reserved for future use)
    #[allow(dead_code)]
    pub(super) config: ExecutorConfig,

    /// Currently running automations (`automation_id` -> state)
    pub(super) running: Arc<RwLock<HashMap<i64, RunningAutomation>>>,

    /// Debounce state for file changes (`automation_id` -> `last_event_time`)
    pub(super) file_change_debounce: Arc<Mutex<HashMap<i64, Instant>>>,

    /// Progress callback
    pub(super) progress_callback: Arc<SyncCallback>,

    /// Currently mounted USB devices (serial -> `mount_point`)
    pub(super) mounted_devices: Arc<RwLock<HashMap<String, PathBuf>>>,
}

impl Executor {
    /// Create a new executor
    pub fn new(config: ExecutorConfig) -> Result<Self, ExecutorError> {
        let db = CacheDb::open().map_err(ExecutorError::Database)?;

        Ok(Self {
            db: Arc::new(db),
            config,
            running: Arc::new(RwLock::new(HashMap::new())),
            file_change_debounce: Arc::new(Mutex::new(HashMap::new())),
            progress_callback: Arc::new(SyncCallback::none()),
            mounted_devices: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Create executor with existing database connection
    pub fn with_db(db: CacheDb, config: ExecutorConfig) -> Self {
        Self {
            db: Arc::new(db),
            config,
            running: Arc::new(RwLock::new(HashMap::new())),
            file_change_debounce: Arc::new(Mutex::new(HashMap::new())),
            progress_callback: Arc::new(SyncCallback::none()),
            mounted_devices: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Set the progress callback
    pub fn set_progress_callback<F>(&mut self, callback: F)
    where
        F: Fn(i64, &cache::Progress) + Send + Sync + 'static,
    {
        self.progress_callback = Arc::new(SyncCallback::new(callback));
    }

    /// Handle an automation event
    #[tracing::instrument(skip(self))]
    pub async fn handle_event(&self, event: AutomationEvent) -> Result<Vec<i64>, ExecutorError> {
        match event {
            AutomationEvent::UsbMounted {
                serial,
                mount_point,
                volume_name,
            } => {
                self.handle_usb_mounted(&serial, &mount_point, volume_name.as_deref())
                    .await
            }
            AutomationEvent::UsbUnmounted {
                serial,
                mount_point,
            } => self.handle_usb_unmounted(&serial, &mount_point).await,
            AutomationEvent::FilesChanged {
                watch_root,
                changed_paths,
            } => self.handle_files_changed(&watch_root, &changed_paths).await,
            AutomationEvent::Manual { automation_id } => self
                .run_automation_manual(automation_id)
                .await
                .map(|id| vec![id]),
        }
    }

    /// Handle USB mount event
    async fn handle_usb_mounted(
        &self,
        serial: &str,
        mount_point: &Path,
        _volume_name: Option<&str>,
    ) -> Result<Vec<i64>, ExecutorError> {
        // Track the mounted device
        {
            let mut mounted = self.mounted_devices.write().await;
            mounted.insert(serial.to_string(), mount_point.to_path_buf());
        }

        // Find automations triggered by this device
        let automations = self.db.find_automations_by_serial(serial)?;

        if automations.is_empty() {
            return Ok(vec![]);
        }

        tracing::info!(
            "USB device {} mounted at {} - {} automation(s) to run",
            serial,
            mount_point.display(),
            automations.len()
        );

        // Run matching automations
        let mut run_ids = Vec::new();
        for automation in automations {
            if !automation.triggers.on_mount {
                continue;
            }

            match self
                .run_automation(&automation, mount_point, TriggerType::Mount)
                .await
            {
                Ok(run_id) => run_ids.push(run_id),
                Err(ExecutorError::AlreadyRunning(_)) => {
                    tracing::debug!(
                        automation_id = automation.id,
                        "Automation already running, skipping"
                    );
                }
                Err(e) => {
                    tracing::error!(automation_id = automation.id, error = %e, "Failed to run automation");
                }
            }
        }

        Ok(run_ids)
    }

    /// Handle USB unmount event
    async fn handle_usb_unmounted(
        &self,
        serial: &str,
        _mount_point: &Path,
    ) -> Result<Vec<i64>, ExecutorError> {
        // Remove from tracked devices
        {
            let mut mounted = self.mounted_devices.write().await;
            mounted.remove(serial);
        }

        // Cancel any running automations for this device
        let running = self.running.read().await;
        for (automation_id, state) in running.iter() {
            // Check if this automation uses this device
            if let Ok(Some(automation)) = self.db.get_automation(*automation_id)
                && automation.dest_device_serial.as_deref() == Some(serial)
            {
                tracing::info!("Cancelling automation {} due to USB unmount", automation_id);
                let _ = state.cancel_tx.send(()).await;
            }
        }

        Ok(vec![])
    }

    /// Handle file change event
    async fn handle_files_changed(
        &self,
        watch_root: &Path,
        _changed_paths: &[PathBuf],
    ) -> Result<Vec<i64>, ExecutorError> {
        // Find automations triggered by this path
        let automations = self
            .db
            .find_automations_by_watch_path(&watch_root.to_string_lossy())?;

        if automations.is_empty() {
            return Ok(vec![]);
        }

        tracing::info!(
            "Files changed in {} - {} automation(s) to check",
            watch_root.display(),
            automations.len()
        );

        // Run matching automations (if destination is available and not debounced)
        let mut run_ids = Vec::new();
        let now = Instant::now();

        for automation in automations {
            // Check per-automation debounce
            {
                let mut debounce = self.file_change_debounce.lock().await;
                if let Some(last) = debounce.get(&automation.id)
                    && now.duration_since(*last)
                        < Duration::from_millis(automation.settings.debounce_ms)
                {
                    tracing::debug!(
                        "Debouncing file change for automation {} ({}ms)",
                        automation.id,
                        automation.settings.debounce_ms
                    );
                    continue;
                }
                debounce.insert(automation.id, now);
            }

            // Check destination is available
            let Some(dest_mount) = self.get_destination_mount(&automation).await else {
                tracing::debug!(automation_id = automation.id, "Destination not available");
                continue;
            };

            match self
                .run_automation(&automation, &dest_mount, TriggerType::Change)
                .await
            {
                Ok(run_id) => run_ids.push(run_id),
                Err(ExecutorError::AlreadyRunning(_)) => {
                    tracing::debug!(
                        automation_id = automation.id,
                        "Automation already running, skipping"
                    );
                }
                Err(e) => {
                    tracing::error!(automation_id = automation.id, error = %e, "Failed to run automation");
                }
            }
        }

        Ok(run_ids)
    }

    /// Run an automation manually
    pub async fn run_automation_manual(&self, automation_id: i64) -> Result<i64, ExecutorError> {
        let automation = self
            .db
            .get_automation(automation_id)?
            .ok_or(ExecutorError::AutomationNotFound(automation_id))?;

        // Get destination mount
        let dest_mount = self
            .get_destination_mount(&automation)
            .await
            .ok_or_else(|| {
                ExecutorError::DestinationNotAvailable(
                    automation
                        .dest_volume_name
                        .clone()
                        .unwrap_or_else(|| "unknown".to_string()),
                )
            })?;

        self.run_automation(&automation, &dest_mount, TriggerType::Manual)
            .await
    }

    /// Get destination mount point for an automation
    pub(super) async fn get_destination_mount(&self, automation: &Automation) -> Option<PathBuf> {
        // If direct path is set, use it
        if let Some(ref path) = automation.dest_path {
            let path = PathBuf::from(path);
            if path.exists() {
                return Some(path);
            }
        }

        // Check if the device is mounted
        if let Some(ref serial) = automation.dest_device_serial {
            let mounted = self.mounted_devices.read().await;
            if let Some(mount_point) = mounted.get(serial) {
                return Some(mount_point.clone());
            }

            // Check /Volumes for the volume name
            if let Some(ref volume_name) = automation.dest_volume_name {
                let volume_path = PathBuf::from("/Volumes").join(volume_name);
                if volume_path.exists() {
                    return Some(volume_path);
                }
            }
        }

        None
    }

    /// Get currently running automations
    pub async fn get_running(&self) -> Vec<(i64, i64)> {
        let running = self.running.read().await;
        running
            .values()
            .map(|r| (r.automation_id, r.run_id))
            .collect()
    }

    /// Cancel a running automation
    pub async fn cancel(&self, automation_id: i64) -> Result<(), ExecutorError> {
        let running = self.running.read().await;
        if let Some(state) = running.get(&automation_id) {
            state
                .cancel_tx
                .send(())
                .await
                .map_err(|_| ExecutorError::Sync("Failed to send cancel signal".to_string()))?;
            Ok(())
        } else {
            Err(ExecutorError::AutomationNotFound(automation_id))
        }
    }

    /// Mark interrupted runs on startup
    pub fn recover_interrupted_runs(&self) -> Result<u64, ExecutorError> {
        let count = self.db.mark_interrupted_on_startup()?;

        if count > 0 {
            tracing::info!("Marked {} interrupted runs as partial", count);
        }

        Ok(count)
    }

    /// Get resumable runs
    pub fn get_resumable_runs(&self) -> Result<Vec<Run>, ExecutorError> {
        self.db
            .find_resumable_runs()
            .map_err(ExecutorError::Database)
    }

    /// Register a USB device as mounted (for tracking)
    pub async fn register_mounted_device(&self, serial: &str, mount_point: &Path) {
        let mut mounted = self.mounted_devices.write().await;
        mounted.insert(serial.to_string(), mount_point.to_path_buf());
    }

    /// Unregister a USB device
    pub async fn unregister_mounted_device(&self, serial: &str) {
        let mut mounted = self.mounted_devices.write().await;
        mounted.remove(serial);
    }

    /// Get database reference
    pub fn db(&self) -> &CacheDb {
        &self.db
    }
}

#[cfg(test)]
#[path = "executor_test.rs"]
mod tests;
