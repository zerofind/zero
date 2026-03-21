//! Wipe engine - core disk wiping logic
//!
//! This module handles the actual wiping process:
//! - Running multiple passes with different patterns
//! - Progress reporting with detailed state
//! - Verification by reading back and comparing
//! - Bad block detection and skip logic
//! - Resume support via persistent state
//!
//! Adapted from the lethe crate (Apache 2.0 license).

use clap::ValueEnum;
use std::collections::HashSet;
use std::io;
use std::time::{Duration, Instant};

use cache::ControlDb;

use super::buffer::AlignedBuffer;
use super::platform::{self, StorageAccess};
use super::schemes::Scheme;
use super::stage::Stage;
use super::state::EraseState;
use super::{EraseError, EraseOptions, EraseTarget};

/// Verification mode for wipe operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum Verify {
    /// No verification (fastest)
    #[value(alias = "none", alias = "false", alias = "0")]
    No,

    /// Verify only the last pass (recommended)
    #[default]
    #[value(alias = "final", alias = "default")]
    Last,

    /// Verify every pass (slowest, most thorough)
    #[value(alias = "every", alias = "true", alias = "1")]
    All,
}

impl Verify {
    /// Parse from string
    pub fn parse_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "no" | "none" | "false" | "0" => Some(Verify::No),
            "last" | "final" | "default" => Some(Verify::Last),
            "all" | "every" | "true" | "1" => Some(Verify::All),
            _ => None,
        }
    }
}

/// Progress information for a wipe operation
#[derive(Debug, Clone)]
pub struct EraseProgress {
    /// Current stage index (0-based)
    pub stage: usize,

    /// Total number of stages
    pub total_stages: usize,

    /// Current pass number within stage (for display, 1-based)
    pub pass: usize,

    /// Total passes in the scheme
    pub total_passes: usize,

    /// Whether we're in verification mode
    pub verifying: bool,

    /// Bytes processed in current stage
    pub bytes_done: u64,

    /// Total bytes in device
    pub bytes_total: u64,

    /// Current write speed (bytes/sec)
    pub speed_bps: u64,

    /// Estimated time remaining (seconds)
    pub eta_seconds: u64,

    /// Number of bad blocks encountered
    pub bad_blocks: u32,

    /// Description of current operation
    pub description: String,
}

impl EraseProgress {
    /// Get progress as percentage (0-100)
    pub fn percent(&self) -> u8 {
        if self.bytes_total == 0 {
            100
        } else {
            ((self.bytes_done as f64 / self.bytes_total as f64) * 100.0) as u8
        }
    }

    /// Get overall progress across all stages (0.0 - 1.0)
    pub fn overall_progress(&self) -> f64 {
        if self.total_stages == 0 {
            return 1.0;
        }

        let stage_progress = self.bytes_done as f64 / self.bytes_total.max(1) as f64;
        let completed_stages = self.stage as f64;

        (completed_stages + stage_progress) / self.total_stages as f64
    }

    /// Format speed for display
    pub fn speed_display(&self) -> String {
        const MB: u64 = 1_000_000;
        const GB: u64 = 1_000_000_000;

        if self.speed_bps >= GB {
            format!("{:.1} GB/s", self.speed_bps as f64 / GB as f64)
        } else if self.speed_bps >= MB {
            format!("{:.1} MB/s", self.speed_bps as f64 / MB as f64)
        } else {
            format!("{} KB/s", self.speed_bps / 1000)
        }
    }

    /// Format ETA for display
    pub fn eta_display(&self) -> String {
        let secs = self.eta_seconds;

        if secs < 60 {
            format!("{secs}s")
        } else if secs < 3600 {
            format!("{}m {}s", secs / 60, secs % 60)
        } else {
            let hours = secs / 3600;
            let mins = (secs % 3600) / 60;
            format!("{hours}h {mins}m")
        }
    }
}

/// Result of a completed wipe operation
#[derive(Debug, Clone)]
pub struct EraseResult {
    /// Whether the operation completed successfully
    pub success: bool,

    /// Total time taken
    pub duration: Duration,

    /// Number of bytes written (including all passes)
    pub bytes_written: u64,

    /// Number of bad blocks encountered and skipped
    pub bad_blocks: u32,

    /// Average write speed across all passes
    pub average_speed_bps: u64,

    /// Whether verification passed
    pub verified: bool,

    /// Any warnings generated during the operation
    pub warnings: Vec<String>,
}

/// Internal state for wipe operation
struct WipeState {
    /// Current position in device
    position: u64,

    /// Bad blocks encountered (block numbers)
    bad_blocks: HashSet<u64>,

    /// Total bytes written across all passes
    total_bytes_written: u64,

    /// Start time for speed calculation
    stage_start: Instant,

    /// Bytes written since last speed update
    bytes_since_update: u64,

    /// Last speed calculation
    last_speed: u64,

    /// Time of last speed update
    last_update: Instant,
}

impl WipeState {
    fn new() -> Self {
        let now = Instant::now();
        Self {
            position: 0,
            bad_blocks: HashSet::new(),
            total_bytes_written: 0,
            stage_start: now,
            bytes_since_update: 0,
            last_speed: 0,
            last_update: now,
        }
    }

    fn reset_for_stage(&mut self) {
        self.position = 0;
        self.stage_start = Instant::now();
        self.bytes_since_update = 0;
        self.last_update = self.stage_start;
    }

    fn block_number(&self, block_size: usize) -> u64 {
        self.position / block_size as u64
    }

    fn is_bad_block(&self, block_size: usize) -> bool {
        self.bad_blocks.contains(&self.block_number(block_size))
    }

    fn mark_bad_block(&mut self, block_size: usize) {
        self.bad_blocks.insert(self.block_number(block_size));
    }

    fn bad_block_count(&self) -> u32 {
        self.bad_blocks.len() as u32
    }

    fn update_speed(&mut self, bytes_written: u64) {
        self.bytes_since_update += bytes_written;
        self.total_bytes_written += bytes_written;

        let elapsed = self.last_update.elapsed();
        if elapsed >= Duration::from_millis(500) {
            let secs = elapsed.as_secs_f64();
            if secs > 0.0 {
                self.last_speed = (self.bytes_since_update as f64 / secs) as u64;
            }
            self.bytes_since_update = 0;
            self.last_update = Instant::now();
        }
    }

    fn calculate_eta(&self, total_size: u64, remaining_stages: usize) -> u64 {
        if self.last_speed == 0 {
            return 0;
        }

        let remaining_in_stage = total_size.saturating_sub(self.position);
        let remaining_bytes = remaining_in_stage + (remaining_stages as u64 * total_size);

        remaining_bytes / self.last_speed
    }
}

/// Run the wipe operation
pub fn run_wipe<F>(
    target: &EraseTarget,
    scheme: &Scheme,
    options: &EraseOptions,
    progress_callback: F,
) -> Result<EraseResult, EraseError>
where
    F: FnMut(EraseProgress),
{
    run_wipe_with_resume(target, scheme, options, None, progress_callback)
}

/// Run the wipe operation with cancellation support
///
/// This variant accepts an `is_cancelled` callback that is checked between blocks.
/// If cancellation is detected, the operation saves state and returns `EraseError::Cancelled`.
/// The operation can be resumed later with `run_wipe_with_resume`.
pub fn run_wipe_cancellable<F, C>(
    target: &EraseTarget,
    scheme: &Scheme,
    options: &EraseOptions,
    resume_state: Option<EraseState>,
    progress_callback: F,
    is_cancelled: C,
) -> Result<EraseResult, EraseError>
where
    F: FnMut(EraseProgress),
    C: Fn() -> bool + Clone,
{
    run_wipe_with_resume_and_cancel(
        target,
        scheme,
        options,
        resume_state,
        progress_callback,
        Some(is_cancelled),
    )
}

/// Run the wipe operation with resume support
///
/// If `resume_state` is provided, the operation will resume from the saved position.
/// The state is automatically saved periodically during the operation.
pub fn run_wipe_with_resume<F>(
    target: &EraseTarget,
    scheme: &Scheme,
    options: &EraseOptions,
    resume_state: Option<EraseState>,
    progress_callback: F,
) -> Result<EraseResult, EraseError>
where
    F: FnMut(EraseProgress),
{
    run_wipe_with_resume_and_cancel::<F, fn() -> bool>(
        target,
        scheme,
        options,
        resume_state,
        progress_callback,
        None,
    )
}

/// Run the wipe operation with resume and optional cancellation support
///
/// If `resume_state` is provided, the operation will resume from the saved position.
/// The state is automatically saved periodically during the operation.
/// If `is_cancelled` is provided, it will be checked between blocks and the operation
/// will stop with state saved if cancellation is requested.
fn run_wipe_with_resume_and_cancel<F, C>(
    target: &EraseTarget,
    scheme: &Scheme,
    options: &EraseOptions,
    resume_state: Option<EraseState>,
    mut progress_callback: F,
    is_cancelled: Option<C>,
) -> Result<EraseResult, EraseError>
where
    F: FnMut(EraseProgress),
    C: Fn() -> bool + Clone,
{
    let start_time = Instant::now();
    let mut state = WipeState::new();
    let mut warnings = target.warnings.clone();

    // Open ControlDb for state persistence
    let control_db = ControlDb::open()
        .map_err(|e| io::Error::other(format!("Failed to open ControlDb: {e}")))?;

    // Extract BSD name for state file
    let bsd_name = target
        .device_path
        .strip_prefix("/dev/r")
        .or_else(|| target.device_path.strip_prefix("/dev/"))
        .unwrap_or(&target.device_path)
        .to_string();

    // Initialize or restore persistent state
    let mut persistent_state = resume_state.unwrap_or_else(|| {
        use cache::EraseDeviceId;
        let device = EraseDeviceId::from_volume_info(
            &bsd_name,
            None, // volume_uuid - we could extract this if needed
            None, // serial_number
            target.size_bytes,
            Some(&target.name),
            target.mount_point.as_deref(),
        );
        EraseState::new(device, options.level, options.verify, options.block_size)
    });

    // Determine starting point from resume state
    let start_stage = persistent_state.progress().current_stage;
    let start_position = persistent_state.progress().stage_position;
    let start_in_verification = persistent_state.progress().in_verification;

    // Unmount the volume first
    if let Some(ref mount_point) = target.mount_point {
        platform::unmount_volume(mount_point)
            .map_err(|e| EraseError::UnmountFailed(format!("{mount_point}: {e}")))?;
    }

    // Open device for direct I/O
    let mut access = platform::open_device(&target.device_path)
        .map_err(|e| EraseError::OpenFailed(format!("{}: {}", target.device_path, e)))?;

    let total_stages = scheme.stages.len();
    let mut verified = false;

    // Track last save time for periodic state persistence
    let mut last_save_time = Instant::now();
    let save_interval = Duration::from_secs(10); // Save every 10 seconds

    // Track last progress report time
    let mut last_progress_time = Instant::now();
    let progress_interval = Duration::from_millis(100); // Report progress every 100ms

    // Run each stage
    for (stage_idx, stage) in scheme.stages.iter().enumerate() {
        // Skip completed stages when resuming
        if persistent_state.is_stage_complete(stage_idx) {
            continue;
        }

        // For the starting stage, restore position
        let resume_position = if stage_idx == start_stage && !start_in_verification {
            start_position
        } else {
            0
        };

        state.reset_for_stage();
        state.position = resume_position;
        state.total_bytes_written = persistent_state.progress().total_bytes_written;

        // Get or generate seed for random stages
        let stage_to_run = if stage.is_random() {
            if let Some(seed) = persistent_state.get_seed(stage_idx) {
                // Use saved seed for resume/verification consistency
                Stage::random_with_seed(seed)
            } else {
                // Generate and record new seed
                let new_stage = Stage::random();
                if let Stage::Random { seed } = &new_stage {
                    persistent_state.record_seed(stage_idx, *seed);
                }
                new_stage
            }
        } else {
            stage.clone()
        };

        // Determine if we need to verify this stage
        let should_verify = match options.verify {
            Verify::No => false,
            Verify::Last => stage_idx == total_stages - 1,
            Verify::All => true,
        };

        // Check for cancellation before starting stage
        if let Some(ref cancel_fn) = is_cancelled
            && cancel_fn()
        {
            // Save state before cancelling
            persistent_state.update_progress(
                stage_idx,
                state.position,
                false,
                state.total_bytes_written,
            );
            let _ = persistent_state.save(&control_db);
            return Err(EraseError::Cancelled);
        }

        // Fill pass (skip if resuming into verification)
        if !(stage_idx == start_stage && start_in_verification) {
            let description = format!(
                "Pass {}/{}: {}",
                stage_idx + 1,
                total_stages,
                stage.description()
            );

            let cancel_check = is_cancelled.clone();
            let fill_result = run_fill_pass_with_state_cancellable(
                &mut *access,
                &stage_to_run,
                target.size_bytes,
                options.block_size,
                &mut state,
                resume_position,
                cancel_check,
                |wipe_state: &WipeState| {
                    // Periodic progress reporting during fill
                    let now = Instant::now();
                    if now.duration_since(last_progress_time) >= progress_interval {
                        last_progress_time = now;
                        progress_callback(EraseProgress {
                            stage: stage_idx,
                            total_stages,
                            pass: stage_idx + 1,
                            total_passes: total_stages,
                            verifying: false,
                            bytes_done: wipe_state.position,
                            bytes_total: target.size_bytes,
                            speed_bps: wipe_state.last_speed,
                            eta_seconds: wipe_state
                                .calculate_eta(target.size_bytes, total_stages - stage_idx - 1),
                            bad_blocks: wipe_state.bad_block_count(),
                            description: description.clone(),
                        });
                    }

                    // Periodic state saving
                    if now.duration_since(last_save_time) >= save_interval {
                        last_save_time = now;
                        persistent_state.update_progress(
                            stage_idx,
                            wipe_state.position,
                            false,
                            wipe_state.total_bytes_written,
                        );
                        let _ = persistent_state.save(&control_db);
                    }
                },
            );

            // Final progress report for this stage
            progress_callback(EraseProgress {
                stage: stage_idx,
                total_stages,
                pass: stage_idx + 1,
                total_passes: total_stages,
                verifying: false,
                bytes_done: state.position,
                bytes_total: target.size_bytes,
                speed_bps: state.last_speed,
                eta_seconds: state.calculate_eta(target.size_bytes, total_stages - stage_idx - 1),
                bad_blocks: state.bad_block_count(),
                description: description.clone(),
            });

            // Handle cancellation
            if let Err(EraseError::Cancelled) = fill_result {
                persistent_state.update_progress(
                    stage_idx,
                    state.position,
                    false,
                    state.total_bytes_written,
                );
                let _ = persistent_state.save(&control_db);
                return Err(EraseError::Cancelled);
            }
            fill_result?;
        }

        // Verification pass
        if should_verify {
            let verify_resume_position = if stage_idx == start_stage && start_in_verification {
                start_position
            } else {
                0
            };

            state.position = verify_resume_position;
            state.stage_start = Instant::now();

            let verify_desc = format!("Verifying pass {}/{}", stage_idx + 1, total_stages);

            // Update state to show we're in verification
            persistent_state.update_progress(
                stage_idx,
                verify_resume_position,
                true,
                state.total_bytes_written,
            );
            let _ = persistent_state.save(&control_db);

            let cancel_check = is_cancelled.clone();
            let verify_result = run_verify_pass_with_state_cancellable(
                &mut *access,
                &stage_to_run,
                target.size_bytes,
                options.block_size,
                &mut state,
                verify_resume_position,
                cancel_check,
                |wipe_state: &WipeState| {
                    // Periodic progress reporting during verify
                    let now = Instant::now();
                    if now.duration_since(last_progress_time) >= progress_interval {
                        last_progress_time = now;
                        progress_callback(EraseProgress {
                            stage: stage_idx,
                            total_stages,
                            pass: stage_idx + 1,
                            total_passes: total_stages,
                            verifying: true,
                            bytes_done: wipe_state.position,
                            bytes_total: target.size_bytes,
                            speed_bps: wipe_state.last_speed,
                            eta_seconds: wipe_state
                                .calculate_eta(target.size_bytes, total_stages - stage_idx - 1),
                            bad_blocks: wipe_state.bad_block_count(),
                            description: verify_desc.clone(),
                        });
                    }

                    // Periodic state saving
                    if now.duration_since(last_save_time) >= save_interval {
                        last_save_time = now;
                        persistent_state.update_progress(
                            stage_idx,
                            wipe_state.position,
                            true,
                            wipe_state.total_bytes_written,
                        );
                        let _ = persistent_state.save(&control_db);
                    }
                },
            );

            // Final progress report for verification
            progress_callback(EraseProgress {
                stage: stage_idx,
                total_stages,
                pass: stage_idx + 1,
                total_passes: total_stages,
                verifying: true,
                bytes_done: state.position,
                bytes_total: target.size_bytes,
                speed_bps: state.last_speed,
                eta_seconds: state.calculate_eta(target.size_bytes, total_stages - stage_idx - 1),
                bad_blocks: state.bad_block_count(),
                description: verify_desc.clone(),
            });

            // Handle cancellation
            if let Err(EraseError::Cancelled) = verify_result {
                persistent_state.update_progress(
                    stage_idx,
                    state.position,
                    true,
                    state.total_bytes_written,
                );
                let _ = persistent_state.save(&control_db);
                return Err(EraseError::Cancelled);
            }
            verify_result?;

            verified = true;
        }

        // Mark stage complete and save
        persistent_state.mark_stage_complete(stage_idx);
        let _ = persistent_state.save(&control_db);
    }

    // Add warning if there were bad blocks
    if !state.bad_blocks.is_empty() {
        warnings.push(format!(
            "{} bad blocks were skipped - device may be failing",
            state.bad_blocks.len()
        ));
    }

    // Clean up state on successful completion
    let _ = EraseState::delete(&control_db, &bsd_name);

    let duration = start_time.elapsed();
    let average_speed = if duration.as_secs() > 0 {
        state.total_bytes_written / duration.as_secs()
    } else {
        state.total_bytes_written
    };

    Ok(EraseResult {
        success: true,
        duration,
        bytes_written: state.total_bytes_written,
        bad_blocks: state.bad_blocks.len() as u32,
        average_speed_bps: average_speed,
        verified,
        warnings,
    })
}

/// Run a single fill pass with resume and cancellation support
#[allow(clippy::too_many_arguments)]
fn run_fill_pass_with_state_cancellable<C, P>(
    access: &mut dyn StorageAccess,
    stage: &Stage,
    total_size: u64,
    block_size: usize,
    state: &mut WipeState,
    start_position: u64,
    is_cancelled: Option<C>,
    mut on_progress: P,
) -> Result<(), EraseError>
where
    C: Fn() -> bool,
    P: FnMut(&WipeState),
{
    let mut stream = stage.stream(total_size, block_size, start_position);

    // Set state position for resume
    state.position = start_position;

    // Counter for periodic cancellation checks (every N blocks)
    let mut blocks_since_cancel_check = 0u32;
    const CANCEL_CHECK_INTERVAL: u32 = 64; // Check every 64 blocks (~64MB with 1MB blocks)

    while stream.advance() {
        // Periodic cancellation check
        blocks_since_cancel_check += 1;
        if blocks_since_cancel_check >= CANCEL_CHECK_INTERVAL {
            blocks_since_cancel_check = 0;
            if let Some(ref cancel_fn) = is_cancelled
                && cancel_fn()
            {
                return Err(EraseError::Cancelled);
            }
        }

        let chunk = stream.get().ok_or_else(|| EraseError::WriteFailed {
            position: state.position,
            message: "Stream returned no data".to_string(),
        })?;

        // Skip bad blocks
        if state.is_bad_block(block_size) {
            state.position += chunk.len() as u64;
            continue;
        }

        // Seek to position
        access
            .seek(state.position)
            .map_err(|e| handle_io_error(e, state, block_size, "seek"))?;

        // Write chunk
        match access.write(chunk) {
            Ok(()) => {
                state.position += chunk.len() as u64;
                state.total_bytes_written += chunk.len() as u64;
                state.update_speed(chunk.len() as u64);

                // Report progress periodically
                on_progress(state);
            }
            Err(e) => {
                if is_io_error(&e) {
                    state.mark_bad_block(block_size);
                    state.position += chunk.len() as u64;
                    continue;
                }
                return Err(EraseError::WriteFailed {
                    position: state.position,
                    message: e.to_string(),
                });
            }
        }
    }

    // Flush to ensure data is on disk
    access.flush().map_err(|e| EraseError::WriteFailed {
        position: state.position,
        message: format!("Flush failed: {e}"),
    })?;

    Ok(())
}

/// Run a verification pass with cancellation support
#[allow(clippy::too_many_arguments)]
fn run_verify_pass_with_state_cancellable<C, P>(
    access: &mut dyn StorageAccess,
    stage: &Stage,
    total_size: u64,
    block_size: usize,
    state: &mut WipeState,
    start_position: u64,
    is_cancelled: Option<C>,
    mut on_progress: P,
) -> Result<(), EraseError>
where
    C: Fn() -> bool,
    P: FnMut(&WipeState),
{
    let mut stream = stage.stream(total_size, block_size, start_position);
    let mut read_buf = AlignedBuffer::new(block_size, block_size);

    // Set state position for resume
    state.position = start_position;

    // Counter for periodic cancellation checks
    let mut blocks_since_cancel_check = 0u32;
    const CANCEL_CHECK_INTERVAL: u32 = 64;

    while stream.advance() {
        // Periodic cancellation check
        blocks_since_cancel_check += 1;
        if blocks_since_cancel_check >= CANCEL_CHECK_INTERVAL {
            blocks_since_cancel_check = 0;
            if let Some(ref cancel_fn) = is_cancelled
                && cancel_fn()
            {
                return Err(EraseError::Cancelled);
            }
        }
        let expected = stream.get().ok_or_else(|| EraseError::WriteFailed {
            position: state.position,
            message: "Stream returned no data".to_string(),
        })?;

        // Skip bad blocks
        if state.is_bad_block(block_size) {
            state.position += expected.len() as u64;
            continue;
        }

        // Seek to position
        access
            .seek(state.position)
            .map_err(|e| EraseError::WriteFailed {
                position: state.position,
                message: format!("Seek failed: {e}"),
            })?;

        // Read chunk
        let read_slice = &mut read_buf.as_mut_slice()[..expected.len()];
        let bytes_read = access
            .read(read_slice)
            .map_err(|e| EraseError::WriteFailed {
                position: state.position,
                message: format!("Read failed: {e}"),
            })?;

        if bytes_read != expected.len() {
            return Err(EraseError::VerificationFailed {
                position: state.position,
                expected: 0,
                actual: 0,
            });
        }

        // Compare
        if read_slice != expected {
            // Find first mismatched byte
            for (i, (&exp, &act)) in expected.iter().zip(read_slice.iter()).enumerate() {
                if exp != act {
                    return Err(EraseError::VerificationFailed {
                        position: state.position + i as u64,
                        expected: exp,
                        actual: act,
                    });
                }
            }
        }

        state.position += expected.len() as u64;
        state.update_speed(expected.len() as u64);

        // Report progress periodically
        on_progress(state);
    }

    Ok(())
}

/// Check if an I/O error is a "bad block" type error
fn is_io_error(e: &std::io::Error) -> bool {
    matches!(e.raw_os_error(), Some(libc::EIO | libc::ESPIPE))
}

/// Handle an I/O error, possibly marking a bad block
fn handle_io_error(
    e: std::io::Error,
    state: &mut WipeState,
    block_size: usize,
    operation: &str,
) -> EraseError {
    if is_io_error(&e) {
        state.mark_bad_block(block_size);
        // Don't return error for bad blocks, we'll skip them
    }
    EraseError::WriteFailed {
        position: state.position,
        message: format!("{operation} failed: {e}"),
    }
}

#[cfg(test)]
#[path = "wipe_test.rs"]
mod wipe_test;
