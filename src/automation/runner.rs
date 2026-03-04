//! Sync runner - executes automation sync jobs
//!
//! Handles the actual sync execution for automations, including:
//! - Running sync jobs for each path in an automation
//! - Progress tracking and updates
//! - Error handling and result accumulation

use std::path::{Path, PathBuf};

use tokio::sync::mpsc;

use crate::cache::{
    Automation, PathMapping, PathProgress, PathStatus, Progress, ResultSummary, RunResult, Settings,
};
use crate::hasher::HashAlgorithm;
use crate::scanner::{ScanOptions, scan_collect};
use crate::sync::{SyncJob, SyncOptions, SyncResult};

use super::executor::Executor;
use super::types::ExecutorError;

impl Executor {
    /// Run an automation and return the run ID
    pub(super) async fn run_automation(
        &self,
        automation: &Automation,
        mount_point: &Path,
        trigger: crate::cache::TriggerType,
    ) -> Result<i64, ExecutorError> {
        use std::time::Instant;

        // Check if already running
        {
            let running = self.running.read().await;
            if running.contains_key(&automation.id) {
                return Err(ExecutorError::AlreadyRunning(automation.id));
            }
        }

        // Also check for running runs in the database
        if let Some(_run) = self.db.find_running_run(automation.id)? {
            return Err(ExecutorError::AlreadyRunning(automation.id));
        }

        // Build initial progress
        let initial_progress = self.build_initial_progress(automation, mount_point)?;

        // Create run record
        let run = self
            .db
            .start_run(automation.id, trigger, Some(initial_progress.clone()))?;

        tracing::info!(
            run_id = run.id,
            automation = %automation.name,
            trigger = ?trigger,
            "Started run"
        );

        // Set up cancellation
        let (cancel_tx, cancel_rx) = mpsc::channel(1);

        // Track as running
        {
            let mut running = self.running.write().await;
            running.insert(
                automation.id,
                super::executor::RunningAutomation {
                    run_id: run.id,
                    automation_id: automation.id,
                    started_at: Instant::now(),
                    cancel_tx,
                },
            );
        }

        // Execute the sync
        let result = self
            .execute_sync(run.id, automation, mount_point, initial_progress, cancel_rx)
            .await;

        // Remove from running
        {
            let mut running = self.running.write().await;
            running.remove(&automation.id);
        }

        // Handle result
        match result {
            Ok(run_result) => {
                self.db.complete_run_success(run.id, &run_result)?;

                tracing::info!(
                    run_id = run.id,
                    files = run_result.summary.files_added
                        + run_result.summary.files_modified
                        + run_result.summary.files_deleted,
                    bytes = run_result.summary.bytes_transferred,
                    "Run completed successfully"
                );

                Ok(run.id)
            }
            Err(ExecutorError::Sync(msg)) if msg.contains("cancelled") => {
                let partial_result = RunResult::default();
                self.db.mark_run_partial(run.id, &partial_result, None)?;

                tracing::info!(run_id = run.id, "Run was cancelled");
                Ok(run.id)
            }
            Err(e) => {
                let error_result = RunResult {
                    summary: ResultSummary::default(),
                    changes: vec![],
                    errors: vec![e.to_string()],
                };
                self.db.complete_run_failed(run.id, &error_result)?;

                tracing::error!(run_id = run.id, error = %e, "Run failed");
                Err(e)
            }
        }
    }

    /// Build initial progress for all paths in an automation
    pub(super) fn build_initial_progress(
        &self,
        automation: &Automation,
        mount_point: &Path,
    ) -> Result<Progress, ExecutorError> {
        let mut progress = Vec::new();

        for path_mapping in &automation.paths {
            let source = PathBuf::from(&path_mapping.source);

            // Get file count and size from source
            let (files_total, bytes_total) = if source.exists() {
                let entries = scan_collect(&source, ScanOptions::default())
                    .map_err(|e| ExecutorError::Sync(format!("Scan error: {}", e)))?;
                (
                    entries.len() as u64,
                    entries.iter().map(|e| e.size).sum::<u64>(),
                )
            } else {
                (0, 0)
            };

            let dest = if path_mapping.dest.is_empty() {
                mount_point.to_string_lossy().to_string()
            } else {
                mount_point
                    .join(&path_mapping.dest)
                    .to_string_lossy()
                    .to_string()
            };

            progress.push(PathProgress {
                source: path_mapping.source.clone(),
                dest,
                status: PathStatus::Pending,
                files_done: 0,
                files_total,
                bytes_done: 0,
                bytes_total,
                error: None,
            });
        }

        Ok(progress)
    }

    /// Execute sync for all paths in an automation
    pub(super) async fn execute_sync(
        &self,
        run_id: i64,
        automation: &Automation,
        mount_point: &Path,
        mut progress: Progress,
        mut cancel_rx: mpsc::Receiver<()>,
    ) -> Result<RunResult, ExecutorError> {
        let mut total_result = RunResult::default();

        for (idx, path_mapping) in automation.paths.iter().enumerate() {
            // Check for cancellation
            if cancel_rx.try_recv().is_ok() {
                return Err(ExecutorError::Sync("Sync cancelled".to_string()));
            }

            // Update progress status
            progress[idx].status = PathStatus::Running;
            self.update_progress(run_id, &progress).await?;

            // Run sync for this path
            let source = PathBuf::from(&path_mapping.source);
            let dest = if path_mapping.dest.is_empty() {
                mount_point.to_path_buf()
            } else {
                mount_point.join(&path_mapping.dest)
            };

            tracing::debug!(source = %source.display(), dest = %dest.display(), "Syncing");

            match self
                .sync_single_path(
                    &source,
                    &dest,
                    path_mapping,
                    &automation.settings,
                    &mut cancel_rx,
                )
                .await
            {
                Ok(result) => {
                    // Update progress
                    progress[idx].status = PathStatus::Complete;
                    progress[idx].files_done = progress[idx].files_total;
                    progress[idx].bytes_done = progress[idx].bytes_total;
                    self.update_progress(run_id, &progress).await?;

                    // Accumulate results
                    total_result.summary.files_added += result.files_transferred as u64;
                    total_result.summary.files_deleted += result.files_deleted as u64;
                    total_result.summary.files_unchanged += result.files_unchanged as u64;
                    total_result.summary.bytes_transferred += result.bytes_transferred;
                }
                Err(e) => {
                    progress[idx].status = PathStatus::Failed;
                    progress[idx].error = Some(e.to_string());
                    self.update_progress(run_id, &progress).await?;

                    total_result
                        .errors
                        .push(format!("{}: {}", path_mapping.source, e));

                    // Continue with other paths even if one fails
                    tracing::error!(source = %path_mapping.source, error = %e, "Failed to sync");
                }
            }
        }

        Ok(total_result)
    }

    /// Sync a single path
    async fn sync_single_path(
        &self,
        source: &Path,
        dest: &Path,
        _path_mapping: &PathMapping,
        settings: &Settings,
        _cancel_rx: &mut mpsc::Receiver<()>,
    ) -> Result<SyncResult, ExecutorError> {
        // Check source exists
        if !source.exists() {
            return Err(ExecutorError::Sync(format!(
                "Source does not exist: {}",
                source.display()
            )));
        }

        // Create destination if needed
        if !dest.exists() {
            std::fs::create_dir_all(dest)?;
        }

        // Build sync options
        let sync_options = SyncOptions {
            use_checksum: false,
            verify: settings.verify,
            delete_orphans: settings.delete_orphans,
            hash_algorithm: HashAlgorithm::Xxh3,
            max_depth: None,
            parallel_copies: 0, // Auto
            preserve_permissions: false,
            exclude_patterns: Vec::new(),
            dry_run: false,
            chunked: false, // Automations use standard copy for now
            chunk_threshold: 100 * 1024 * 1024, // 100MB
        };

        // Run sync (blocking operation, run in spawn_blocking)
        let source = source.to_path_buf();
        let dest = dest.to_path_buf();

        let result = tokio::task::spawn_blocking(move || {
            let job = SyncJob::new(&source, &dest, sync_options)
                .map_err(|e| ExecutorError::Sync(e.to_string()))?;

            // Run with no-op progress callback (we track progress at path level)
            job.run(|_progress| {})
                .map_err(|e| ExecutorError::Sync(e.to_string()))
        })
        .await
        .map_err(|e| ExecutorError::Sync(format!("Task join error: {}", e)))??;

        Ok(result)
    }

    /// Update progress in database and call callback
    pub(super) async fn update_progress(
        &self,
        run_id: i64,
        progress: &Progress,
    ) -> Result<(), ExecutorError> {
        self.db.update_run_progress(run_id, progress)?;

        self.progress_callback.call(run_id, progress);

        Ok(())
    }
}
