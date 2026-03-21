//! Secure erase command handler
//!
//! This command provides secure disk wiping functionality with multiple
//! security levels and comprehensive safety checks. Supports resume if
//! interrupted.

use std::io::{self, Write};
use std::time::Instant;

use cache::ControlDb;
use disk::VolumeInfo;
use disk::erase::platform;
use disk::erase::state::DeviceIdentifier;
use disk::erase::{
    EraseError, EraseLevel, EraseOptions, EraseProgress, EraseTarget, Verify, prepare_erase,
    state::EraseState,
};
use foundation::output::{Outputter, format_bytes};
use foundation::{cmd_error, cmd_success};

/// Arguments for the erase command
pub struct EraseArgs {
    pub path: std::path::PathBuf,
    pub level: EraseLevel,
    pub verify: Verify,
    pub force: bool,
    pub dry_run: bool,
}

pub fn cmd_erase(out: &Outputter, args: &EraseArgs) -> anyhow::Result<()> {
    let start = Instant::now();

    let level = args.level;
    let verify = args.verify;

    // Open ControlDb for erase state persistence
    let control_db = ControlDb::open()?;

    // Check if the path is a raw device (e.g., /dev/rdisk24s1 or /dev/disk24s1)
    // This allows restarting after interruption when the volume is no longer mounted
    let path_str = args.path.to_string_lossy();
    let is_raw_device = path_str.starts_with("/dev/rdisk") || path_str.starts_with("/dev/disk");

    if is_raw_device {
        // Direct device path - check for resume state first
        return cmd_erase_device(out, args, &path_str, level, verify, &control_db);
    }

    // Get volume info
    let volume = match VolumeInfo::for_path(&args.path) {
        Ok(v) => v,
        Err(e) => {
            // Volume not found - check if there's a pending erase for this path
            // This handles the case where user runs `zero erase /Volumes/Ghost`
            // after an interrupted erase (volume is unmounted during erase)
            if let Ok(Some(pending_state)) =
                EraseState::find_by_volume_name_or_mount_point(&control_db, &path_str)
            {
                // Found a pending erase! Check if it's safe to resume
                let device_path = format!("/dev/r{}", pending_state.device().bsd_name);

                // Verify the device still exists and get its current info
                match get_device_info(&device_path) {
                    Ok(device_info) => {
                        // Safety check: make sure the device hasn't been reformatted with a new label
                        if pending_state.device_has_new_label(device_info.volume_name.as_deref()) {
                            let msg = format!(
                                "Device {} now has a different volume name '{}'. \
                                 It may have been reformatted. \
                                 Use the device path directly if you're sure: {}",
                                pending_state.device().bsd_name,
                                device_info.volume_name.as_deref().unwrap_or_default(),
                                device_path
                            );
                            cmd_error!(
                                out,
                                "erase",
                                start.elapsed().as_millis() as u64,
                                "DEVICE_CHANGED",
                                msg,
                                {
                                    out.error(
                                        "Device appears to have been reformatted with a new name",
                                    );
                                    out.println(&format!(
                                        "  Original name: {}",
                                        pending_state
                                            .device()
                                            .volume_name
                                            .as_deref()
                                            .unwrap_or("(none)")
                                    ));
                                    out.println(&format!(
                                        "  Current name:  {}",
                                        device_info.volume_name.as_deref().unwrap_or("(none)")
                                    ));
                                    out.newline();
                                    out.info("If you're sure this is the same device, use the device path directly:");
                                    out.println(&format!(
                                        "  zero erase {} --level {}",
                                        device_path, args.level
                                    ));
                                }
                            );
                            return Ok(());
                        }

                        // Device looks safe - auto-resume (no confirmation needed, we verified it's the same device)
                        if !out.is_json() {
                            out.info(&format!(
                                "Volume '{path_str}' is not mounted - resuming interrupted erase..."
                            ));
                            out.println(&format!(
                                "  Device:      {} ({})",
                                device_path,
                                pending_state.device().bsd_name
                            ));
                            out.println(&format!("  Progress:    {}", pending_state.summary()));
                            out.newline();
                        }

                        // Resume the erase - pass skip_confirmation=true since we've verified
                        return cmd_erase_device_resume(
                            out,
                            args,
                            &device_path,
                            level,
                            verify,
                            &control_db,
                        );
                    }
                    Err(device_err) => {
                        // Device no longer exists
                        let msg = format!(
                            "Found pending erase for '{}' on device {}, but device is no longer available: {}",
                            path_str,
                            pending_state.device().bsd_name,
                            device_err
                        );
                        cmd_error!(
                            out,
                            "erase",
                            start.elapsed().as_millis() as u64,
                            "DEVICE_NOT_FOUND",
                            msg,
                            {
                                out.error(&format!("Volume not found: {e}"));
                                out.warn(&format!(
                                "Found interrupted erase for this volume on device {}, but device is no longer connected",
                                pending_state.device().bsd_name
                            ));
                                out.info("Reconnect the device and try again.");
                            }
                        );
                        return Ok(());
                    }
                }
            }

            // No pending state found - just report the error
            cmd_error!(
                out,
                "erase",
                start.elapsed().as_millis() as u64,
                "VOLUME_ERROR",
                format!("Failed to get volume info: {}", e)
            );
            return Ok(());
        }
    };

    // Prepare erase and get target info (this does safety checks)
    let target = match prepare_erase(&volume) {
        Ok(t) => t,
        Err(EraseError::SystemDisk(name)) => {
            let msg = format!(
                "SAFETY BLOCK: '{name}' is an internal/system disk. \
                 Refusing to erase to prevent accidental data loss."
            );
            cmd_error!(
                out,
                "erase",
                start.elapsed().as_millis() as u64,
                "SYSTEM_DISK",
                msg
            );
            return Ok(());
        }
        Err(EraseError::NotWritable(name)) => {
            let msg = format!("Volume '{name}' is not writable");
            cmd_error!(
                out,
                "erase",
                start.elapsed().as_millis() as u64,
                "NOT_WRITABLE",
                msg
            );
            return Ok(());
        }
        Err(e) => {
            cmd_error!(
                out,
                "erase",
                start.elapsed().as_millis() as u64,
                "PREPARE_ERROR",
                format!("Failed to prepare erase: {}", e)
            );
            return Ok(());
        }
    };

    let scheme = level.scheme();

    // Show what we're about to do
    if !out.is_json() {
        out.header("Secure Erase");
        out.newline();

        out.warn("⚠️  WARNING: THIS OPERATION IS IRREVERSIBLE ⚠️");
        out.newline();

        out.info("Target device:");
        out.println(&format!("  Name:        {}", target.name));
        out.println(&format!(
            "  Device:      {} ← use this to restart if interrupted",
            target.device_path
        ));
        out.println(&format!("  Size:        {}", target.size_display()));
        if let Some(ref mount) = target.mount_point {
            out.println(&format!("  Mount Point: {mount}"));
        }
        out.newline();

        out.info("Erase settings:");
        out.println(&format!(
            "  Level:       {} ({})",
            level.name(),
            level.description()
        ));
        out.println(&format!("  Method:      {}", scheme.name));
        out.println(&format!("  Passes:      {}", scheme.pass_count()));
        out.println(&format!("  Verify:      {verify:?}"));

        // Estimate time (assume 100 MB/s for USB drives, 500 MB/s for others)
        let estimated_speed = if volume.is_usb() { 100 } else { 500 };
        let time_estimate = scheme.estimate_time_display(target.size_bytes, estimated_speed);
        out.println(&format!("  Est. Time:   ~{time_estimate}"));
        out.newline();

        // Show warnings
        if !target.warnings.is_empty() {
            out.warn("Warnings:");
            for warning in &target.warnings {
                out.println(&format!("  • {warning}"));
            }
            out.newline();
        }
    }

    // Dry run - just show what would happen
    if args.dry_run {
        let data = serde_json::json!({
            "dry_run": true,
            "target": {
                "name": target.name,
                "device_path": target.device_path,
                "size_bytes": target.size_bytes,
                "is_flash": target.is_flash,
            },
            "settings": {
                "level": level.name(),
                "passes": scheme.pass_count(),
                "verify": format!("{:?}", verify),
            },
        });
        cmd_success!(out, "erase", start.elapsed().as_millis() as u64, data, {
            out.info("DRY RUN - no changes will be made");
            out.success("Would erase the above device with the specified settings");
        });
        return Ok(());
    }

    // Confirmation (unless --force)
    if !args.force && !out.is_json() {
        out.warn(&format!(
            "ALL DATA ON '{}' WILL BE PERMANENTLY DESTROYED!",
            target.name
        ));
        out.newline();

        // Require typing the volume name to confirm
        out.println(&format!(
            "To confirm, type the volume name exactly: {}",
            target.name
        ));
        print!("> ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim();

        if input != target.name {
            out.error("Confirmation failed - volume name did not match");
            out.info("Erase cancelled");
            return Ok(());
        }

        out.newline();
    }

    // Record telemetry for erase attempt
    let level_name = level.name().to_string();

    // Build options
    let options = EraseOptions::new(level)
        .with_verify(verify)
        .with_force(args.force);

    // Extract BSD name for state lookup
    let bsd_name = volume.bsd_name.clone();

    // Check for existing resume state
    let resume_state = match EraseState::load(&control_db, &bsd_name) {
        Ok(Some(state)) if state.can_resume_for(&bsd_name, target.size_bytes, level.name()) => {
            if !out.is_json() && !args.force {
                out.warn("Found interrupted erase operation!");
                out.println(&format!("  Progress: {}", state.summary()));
                out.println(&format!(
                    "  Started:  {} seconds ago",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs()
                        .saturating_sub(state.started_at())
                ));
                out.newline();

                out.println("Resume the interrupted operation? (y/n)");
                print!("> ");
                io::stdout().flush()?;

                let mut input = String::new();
                io::stdin().read_line(&mut input)?;
                let input = input.trim().to_lowercase();

                if input == "y" || input == "yes" {
                    out.success("Resuming...");
                    out.newline();
                    Some(state)
                } else {
                    out.info("Starting fresh (previous progress will be discarded)");
                    let _ = EraseState::delete(&control_db, &bsd_name);
                    out.newline();
                    None
                }
            } else {
                // Force mode or JSON - auto-resume
                Some(state)
            }
        }
        Ok(Some(_)) => {
            // State exists but doesn't match (different level, etc.)
            if !out.is_json() {
                out.warn("Found state from different erase settings, starting fresh");
            }
            let _ = EraseState::delete(&control_db, &bsd_name);
            None
        }
        _ => None,
    };

    // Run the erase with detailed progress
    if !out.is_json() {
        if resume_state.is_some() {
            out.info(&format!("Resuming secure erase of '{}'...", target.name));
        } else {
            out.info(&format!("Starting secure erase of '{}'...", target.name));
        }
        out.newline();

        // Show preparation steps
        out.println("  [1/3] Unmounting volume...");
        io::stdout().flush().ok();
    }

    let mut last_percent: u8 = 0;
    let mut last_stage: usize = usize::MAX; // Use MAX to detect first stage
    let mut stage_start_time = Instant::now();

    // Prepare target and scheme for wipe
    let scheme = level.scheme();

    // Create persistent state if not resuming
    let persistent_state = resume_state.unwrap_or_else(|| {
        let device = DeviceIdentifier::from_volume_info(
            &bsd_name,
            Some(&volume.volume_uuid),
            None, // serial if available
            target.size_bytes,
            Some(&target.name),
            target.mount_point.as_deref(),
        );
        EraseState::new(device, level, verify, options.block_size)
    });

    // Unmount and open device manually since we're using run_wipe_with_resume
    if let Some(ref mount_point) = target.mount_point
        && let Err(e) = platform::unmount_volume(mount_point)
    {
        if !out.is_json() {
            out.error(&format!("Failed to unmount: {e}"));
        }
        return Ok(());
    }

    if !out.is_json() {
        print!("\r  [2/3] Opening device for direct I/O...    \n");
        io::stdout().flush().ok();
    }

    let mut access = match platform::open_device(&target.device_path) {
        Ok(a) => a,
        Err(e) => {
            if !out.is_json() {
                out.error(&format!("Failed to open device: {e}"));
            }
            return Ok(());
        }
    };

    if !out.is_json() {
        print!("  [3/3] Erasing...\n\n");
        io::stdout().flush().ok();
    }

    // Use the internal wipe function with resume support
    let result = run_wipe_with_resume_internal(
        &mut *access,
        &target,
        &scheme,
        &options,
        Some(persistent_state),
        |progress: EraseProgress| {
            if out.is_json() {
                // In JSON mode, we could emit progress events, but for now skip
                return;
            }

            // Update progress display
            let percent = progress.percent();
            let stage_changed = progress.stage != last_stage;

            if stage_changed {
                // Show completion of previous stage (if not first)
                if last_stage != usize::MAX {
                    let elapsed = stage_start_time.elapsed();
                    print!(
                        "\r                                                                                    \r"
                    );
                    out.success(&format!(
                        "Pass {}/{} complete ({:.1}s)",
                        last_stage + 1,
                        progress.total_passes,
                        elapsed.as_secs_f64()
                    ));
                }

                last_stage = progress.stage;
                stage_start_time = Instant::now();
                last_percent = 0;

                // Show what we're starting
                let phase_name = if progress.verifying {
                    "Verifying"
                } else {
                    "Writing"
                };
                out.info(&format!(
                    "Pass {}/{}: {} ({})",
                    progress.stage + 1,
                    progress.total_passes,
                    progress
                        .description
                        .split(": ")
                        .last()
                        .unwrap_or(&progress.description),
                    phase_name.to_lowercase()
                ));
            }

            // Only update display every 1%
            if percent > last_percent || stage_changed {
                last_percent = percent;

                let phase = if progress.verifying {
                    "Verifying"
                } else {
                    "Writing"
                };

                // Calculate total ETA across remaining passes
                let remaining_passes = progress.total_passes.saturating_sub(progress.stage + 1);
                let current_pass_eta = progress.eta_seconds;
                // Estimate remaining passes based on current speed
                let estimated_per_pass = if percent > 0 {
                    progress.bytes_total / progress.speed_bps.max(1)
                } else {
                    current_pass_eta
                };
                let total_eta = current_pass_eta + (remaining_passes as u64 * estimated_per_pass);

                let total_eta_display = format_eta(total_eta);

                print!(
                    "\r  {} {}%  [{:<50}]  {}  Pass ETA: {} | Total ETA: {}    ",
                    phase,
                    percent,
                    "█".repeat((percent as usize) / 2),
                    progress.speed_display(),
                    progress.eta_display(),
                    total_eta_display,
                );
                io::stdout().flush().ok();
            }
        },
    );

    // Clear the progress line
    if !out.is_json() {
        println!();
    }

    match result {
        Ok(erase_result) => {
            let duration_ms = start.elapsed().as_millis() as u64;
            let data = serde_json::json!({
                "success": true,
                "target": {
                    "name": target.name,
                    "device_path": target.device_path,
                    "size_bytes": target.size_bytes,
                },
                "result": {
                    "duration_ms": erase_result.duration.as_millis() as u64,
                    "bytes_written": erase_result.bytes_written,
                    "bad_blocks": erase_result.bad_blocks,
                    "average_speed_bps": erase_result.average_speed_bps,
                    "verified": erase_result.verified,
                },
                "warnings": erase_result.warnings,
            });
            telemetry::record_erase(&level_name);
            cmd_success!(out, "erase", duration_ms, data, {
                out.newline();
                out.success(&format!("Secure erase of '{}' complete!", target.name));
                out.newline();
                out.info("Summary:");
                out.println(&format!(
                    "  Duration:      {:.1}s",
                    erase_result.duration.as_secs_f64()
                ));
                out.println(&format!(
                    "  Data written:  {}",
                    format_bytes(erase_result.bytes_written)
                ));
                out.println(&format!(
                    "  Avg speed:     {}/s",
                    format_bytes(erase_result.average_speed_bps)
                ));
                out.println(&format!(
                    "  Verified:      {}",
                    if erase_result.verified { "Yes" } else { "No" }
                ));
                if erase_result.bad_blocks > 0 {
                    out.warn(&format!(
                        "  Bad blocks:    {} (skipped)",
                        erase_result.bad_blocks
                    ));
                }
                if !erase_result.warnings.is_empty() {
                    out.newline();
                    out.warn("Warnings:");
                    for warning in &erase_result.warnings {
                        out.println(&format!("  • {warning}"));
                    }
                }
                out.newline();
                out.info("The device is now safe to dispose of, recycle, or repurpose.");
            });
        }
        Err(e) => {
            let duration_ms = start.elapsed().as_millis() as u64;
            let msg = format!("Erase failed: {e}");
            cmd_error!(out, "erase", duration_ms, "ERASE_FAILED", msg, {
                out.newline();
                out.error(&msg);
                match &e {
                    EraseError::UnmountFailed(_) => {
                        out.info("Try closing any applications using the drive and retry.");
                    }
                    EraseError::OpenFailed(_) => {
                        out.info("Make sure you have permission to access the device.");
                        out.info("On macOS, you may need to run with sudo.");
                    }
                    EraseError::TooManyBadBlocks { .. } => {
                        out.warn("The device has too many bad blocks and may be failing.");
                        out.info("Consider physically destroying the device instead.");
                    }
                    _ => {}
                }
                out.newline();
                out.info("To restart the erase (the volume may be unmounted now):");
                out.println(&format!(
                    "  sudo zero erase {} --level {} --force",
                    target.device_path,
                    level.name()
                ));
            });
        }
    }

    Ok(())
}

/// Format seconds as human-readable ETA
fn format_eta(secs: u64) -> String {
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

/// Internal wipe function that uses `run_wipe_with_resume`
fn run_wipe_with_resume_internal<F>(
    _access: &mut dyn disk::erase::platform::StorageAccess,
    target: &EraseTarget,
    scheme: &disk::erase::Scheme,
    options: &EraseOptions,
    resume_state: Option<EraseState>,
    progress_callback: F,
) -> Result<disk::erase::wipe::EraseResult, EraseError>
where
    F: FnMut(EraseProgress),
{
    // We need to call the internal wipe logic
    // For now, delegate to run_wipe_with_resume which handles its own device opening
    // This is a bit redundant but keeps the API clean
    disk::erase::wipe::run_wipe_with_resume(
        target,
        scheme,
        options,
        resume_state,
        progress_callback,
    )
}

/// Handle erase for raw device paths with auto-resume (called from volume-not-found path)
/// Skips all confirmations since we've already verified the device
fn cmd_erase_device_resume(
    out: &Outputter,
    args: &EraseArgs,
    device_path: &str,
    level: EraseLevel,
    verify: Verify,
    control_db: &ControlDb,
) -> anyhow::Result<()> {
    // Create modified args with force=true to skip confirmations
    let force_args = EraseArgs {
        path: args.path.clone(),
        level: args.level,
        verify: args.verify,
        force: true,
        dry_run: args.dry_run,
    };
    cmd_erase_device(out, &force_args, device_path, level, verify, control_db)
}

/// Handle erase for raw device paths (e.g., /dev/rdisk24s1)
/// Used when restarting after interruption when volume is no longer mounted
fn cmd_erase_device(
    out: &Outputter,
    args: &EraseArgs,
    device_path: &str,
    level: EraseLevel,
    verify: Verify,
    control_db: &ControlDb,
) -> anyhow::Result<()> {
    let start = Instant::now();

    // Get device size using diskutil
    let size_bytes = match get_device_size(device_path) {
        Ok(size) => size,
        Err(e) => {
            let msg = format!("Failed to get device size: {e}");
            cmd_error!(
                out,
                "erase",
                start.elapsed().as_millis() as u64,
                "DEVICE_ERROR",
                msg,
                {
                    out.error(&msg);
                    out.info("Make sure the device path is correct (e.g., /dev/rdisk24s1)");
                }
            );
            return Ok(());
        }
    };

    // Extract BSD name for state lookup
    let bsd_name = device_path
        .strip_prefix("/dev/r")
        .or_else(|| device_path.strip_prefix("/dev/"))
        .unwrap_or(device_path)
        .to_string();

    // Check for existing resume state - auto-resume without confirmation
    let resume_state = match EraseState::load(control_db, &bsd_name) {
        Ok(Some(state)) if state.can_resume_for(&bsd_name, size_bytes, level.name()) => {
            if !out.is_json() {
                out.info("Resuming interrupted erase...");
                out.println(&format!("  Progress: {}", state.summary()));
                out.newline();
            }
            Some(state)
        }
        Ok(Some(_)) => {
            // State exists but doesn't match
            if !out.is_json() {
                out.warn("Found state from different erase settings, starting fresh");
            }
            let _ = EraseState::delete(control_db, &bsd_name);
            None
        }
        _ => None,
    };

    // Create target info for raw device
    let target = EraseTarget {
        device_path: device_path.to_string(),
        name: bsd_name.clone(),
        size_bytes,
        is_flash: true, // Assume flash for safety warning
        mount_point: None,
        warnings: vec![
            "Direct device access - some safety checks bypassed".to_string(),
            "This appears to be a flash-based device. Due to wear leveling, some data may theoretically remain.".to_string(),
        ],
    };

    let scheme = level.scheme();

    // Show what we're about to do
    if !out.is_json() {
        if resume_state.is_some() {
            out.header("Secure Erase (Resuming)");
        } else {
            out.header("Secure Erase (Direct Device)");
        }
        out.newline();

        out.warn("⚠️  WARNING: THIS OPERATION IS IRREVERSIBLE ⚠️");
        out.newline();

        out.info("Target device:");
        out.println(&format!("  Device:      {}", target.device_path));
        out.println(&format!("  Size:        {}", target.size_display()));
        out.newline();

        out.info("Erase settings:");
        out.println(&format!(
            "  Level:       {} ({})",
            level.name(),
            level.description()
        ));
        out.println(&format!("  Method:      {}", scheme.name));
        out.println(&format!("  Passes:      {}", scheme.pass_count()));
        out.println(&format!("  Verify:      {verify:?}"));

        let estimated_speed = 100; // Assume USB speed
        let time_estimate = scheme.estimate_time_display(target.size_bytes, estimated_speed);
        out.println(&format!("  Est. Time:   ~{time_estimate}"));
        out.newline();

        // Show warnings
        out.warn("Warnings:");
        for warning in &target.warnings {
            out.println(&format!("  • {warning}"));
        }
        out.newline();
    }

    // Skip confirmation prompts if resuming (we've already verified the device)
    let skip_confirmation = resume_state.is_some();

    // Dry run
    if args.dry_run {
        let data = serde_json::json!({
            "dry_run": true,
            "target": {
                "device_path": target.device_path,
                "size_bytes": target.size_bytes,
            },
            "settings": {
                "level": level.name(),
                "passes": scheme.pass_count(),
                "verify": format!("{:?}", verify),
            },
        });
        cmd_success!(out, "erase", start.elapsed().as_millis() as u64, data, {
            out.info("DRY RUN - no changes will be made");
            out.success("Would erase the above device with the specified settings");
        });
        return Ok(());
    }

    // Confirmation (unless --force or resuming)
    if !args.force && !skip_confirmation && !out.is_json() {
        out.warn(&format!(
            "ALL DATA ON '{}' WILL BE PERMANENTLY DESTROYED!",
            target.device_path
        ));
        out.newline();

        out.println("To confirm, type 'YES' (all caps):");
        print!("> ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim();

        if input != "YES" {
            out.error("Confirmation failed");
            out.info("Erase cancelled");
            return Ok(());
        }

        out.newline();
    }

    // Build options
    let options = EraseOptions::new(level)
        .with_verify(verify)
        .with_force(args.force);

    // Run the erase
    if !out.is_json() {
        if resume_state.is_some() {
            out.info(&format!(
                "Resuming secure erase of '{}'...",
                target.device_path
            ));
        } else {
            out.info(&format!(
                "Starting secure erase of '{}'...",
                target.device_path
            ));
        }
        out.newline();
        out.println("  [1/2] Opening device for direct I/O...");
        io::stdout().flush().ok();
    }

    let mut last_percent: u8 = 0;
    let mut last_stage: usize = usize::MAX;
    let mut shown_start_msg = false;
    let mut stage_start_time = Instant::now();

    // Create persistent state if not resuming
    let persistent_state = resume_state.unwrap_or_else(|| {
        let device =
            DeviceIdentifier::from_volume_info(&bsd_name, None, None, size_bytes, None, None);
        EraseState::new(device, level, verify, options.block_size)
    });

    let result = run_wipe_with_resume_internal(
        &mut *platform::open_device(&target.device_path)
            .map_err(|e| anyhow::anyhow!("Failed to open device: {e}"))?,
        &target,
        &scheme,
        &options,
        Some(persistent_state),
        |progress: EraseProgress| {
            if out.is_json() {
                return;
            }

            if !shown_start_msg {
                print!("\r  [2/2] Erasing...                              \n\n");
                io::stdout().flush().ok();
                shown_start_msg = true;
            }

            let percent = progress.percent();
            let stage_changed = progress.stage != last_stage;

            if stage_changed {
                if last_stage != usize::MAX {
                    let elapsed = stage_start_time.elapsed();
                    print!(
                        "\r                                                                                    \r"
                    );
                    out.success(&format!(
                        "Pass {}/{} complete ({:.1}s)",
                        last_stage + 1,
                        progress.total_passes,
                        elapsed.as_secs_f64()
                    ));
                }

                last_stage = progress.stage;
                stage_start_time = Instant::now();
                last_percent = 0;

                let phase_name = if progress.verifying {
                    "Verifying"
                } else {
                    "Writing"
                };
                out.info(&format!(
                    "Pass {}/{}: {} ({})",
                    progress.stage + 1,
                    progress.total_passes,
                    progress
                        .description
                        .split(": ")
                        .last()
                        .unwrap_or(&progress.description),
                    phase_name.to_lowercase()
                ));
            }

            if percent > last_percent || stage_changed {
                last_percent = percent;

                let phase = if progress.verifying {
                    "Verifying"
                } else {
                    "Writing"
                };

                let remaining_passes = progress.total_passes.saturating_sub(progress.stage + 1);
                let current_pass_eta = progress.eta_seconds;
                let estimated_per_pass = if percent > 0 {
                    progress.bytes_total / progress.speed_bps.max(1)
                } else {
                    current_pass_eta
                };
                let total_eta = current_pass_eta + (remaining_passes as u64 * estimated_per_pass);
                let total_eta_display = format_eta(total_eta);

                print!(
                    "\r  {} {}%  [{:<50}]  {}  Pass ETA: {} | Total ETA: {}    ",
                    phase,
                    percent,
                    "█".repeat((percent as usize) / 2),
                    progress.speed_display(),
                    progress.eta_display(),
                    total_eta_display,
                );
                io::stdout().flush().ok();
            }
        },
    );

    if !out.is_json() {
        println!();
    }

    match result {
        Ok(erase_result) => {
            let duration_ms = start.elapsed().as_millis() as u64;
            let data = serde_json::json!({
                "success": true,
                "target": {
                    "device_path": target.device_path,
                    "size_bytes": target.size_bytes,
                },
                "result": {
                    "duration_ms": erase_result.duration.as_millis() as u64,
                    "bytes_written": erase_result.bytes_written,
                    "bad_blocks": erase_result.bad_blocks,
                    "average_speed_bps": erase_result.average_speed_bps,
                    "verified": erase_result.verified,
                },
                "warnings": erase_result.warnings,
            });
            telemetry::record_erase(level.name());
            cmd_success!(out, "erase", duration_ms, data, {
                out.newline();
                out.success(&format!(
                    "Secure erase of '{}' complete!",
                    target.device_path
                ));
                out.newline();
                out.info("Summary:");
                out.println(&format!(
                    "  Duration:      {:.1}s",
                    erase_result.duration.as_secs_f64()
                ));
                out.println(&format!(
                    "  Data written:  {}",
                    format_bytes(erase_result.bytes_written)
                ));
                out.println(&format!(
                    "  Avg speed:     {}/s",
                    format_bytes(erase_result.average_speed_bps)
                ));
                out.println(&format!(
                    "  Verified:      {}",
                    if erase_result.verified { "Yes" } else { "No" }
                ));
                out.newline();
                out.info("The device is now safe to dispose of, recycle, or repurpose.");
            });
        }
        Err(e) => {
            let duration_ms = start.elapsed().as_millis() as u64;
            let msg = format!("Erase failed: {e}");
            cmd_error!(out, "erase", duration_ms, "ERASE_FAILED", msg, {
                out.newline();
                out.error(&msg);
                out.newline();
                out.info("To restart the erase:");
                out.println(&format!(
                    "  sudo zero erase {} --level {} --force",
                    target.device_path,
                    level.name()
                ));
            });
        }
    }

    Ok(())
}

/// Get device size using diskutil info
/// Information about a device obtained from diskutil
struct DeviceInfo {
    size_bytes: u64,
    volume_name: Option<String>,
}

/// Get device info (size and current volume name) using diskutil
fn get_device_info(device_path: &str) -> Result<DeviceInfo, String> {
    use std::process::Command;

    // Extract disk identifier (e.g., "disk24s1" from "/dev/rdisk24s1")
    let disk_id = device_path
        .strip_prefix("/dev/r")
        .or_else(|| device_path.strip_prefix("/dev/"))
        .ok_or_else(|| format!("Invalid device path: {device_path}"))?;

    let output = Command::new("diskutil")
        .args(["info", "-plist", disk_id])
        .output()
        .map_err(|e| format!("Failed to run diskutil: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "diskutil failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let plist_str = String::from_utf8_lossy(&output.stdout);

    // Extract TotalSize
    let size_bytes = extract_plist_integer(&plist_str, "TotalSize")
        .ok_or_else(|| "Could not find TotalSize in diskutil output".to_string())?;

    // Extract VolumeName (may not exist for unmounted/raw devices)
    let volume_name = extract_plist_string(&plist_str, "VolumeName");

    Ok(DeviceInfo {
        size_bytes,
        volume_name,
    })
}

/// Get device size using diskutil info
fn get_device_size(device_path: &str) -> Result<u64, String> {
    get_device_info(device_path).map(|info| info.size_bytes)
}

/// Extract an integer value from a plist XML string
fn extract_plist_integer(plist: &str, key: &str) -> Option<u64> {
    let key_tag = format!("<key>{key}</key>");
    if let Some(key_pos) = plist.find(&key_tag) {
        let after_key = &plist[key_pos..];
        if let Some(int_start) = after_key.find("<integer>") {
            let after_int_start = &after_key[int_start + 9..];
            if let Some(int_end) = after_int_start.find("</integer>") {
                let value_str = &after_int_start[..int_end];
                return value_str.parse::<u64>().ok();
            }
        }
    }
    None
}

/// Extract a string value from a plist XML string
fn extract_plist_string(plist: &str, key: &str) -> Option<String> {
    let key_tag = format!("<key>{key}</key>");
    if let Some(key_pos) = plist.find(&key_tag) {
        let after_key = &plist[key_pos..];
        if let Some(str_start) = after_key.find("<string>") {
            let after_str_start = &after_key[str_start + 8..];
            if let Some(str_end) = after_str_start.find("</string>") {
                let value = &after_str_start[..str_end];
                if !value.is_empty() {
                    return Some(value.to_string());
                }
            }
        }
    }
    None
}
