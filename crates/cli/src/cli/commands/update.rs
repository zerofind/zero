//! Update command handler

use foundation::output::Outputter;
use updater::{self, CURRENT_VERSION, UpdateStatus, check_latest, download_update, install_binary};

/// Run the update command
#[allow(clippy::unnecessary_wraps)] // CLI command handlers return Result for consistency
pub fn cmd_update(out: &Outputter, check_only: bool) -> anyhow::Result<()> {
    out.info(&format!("Current version: {CURRENT_VERSION}"));

    let (os, arch) = updater::platform_target();
    out.info(&format!("Platform: {os}-{arch}"));

    // Always check (ignore 24h cooldown — explicit user action)
    out.info("Checking for updates...");

    let status = match check_latest() {
        Ok(s) => s,
        Err(e) => {
            out.error(&format!("Failed to check for updates: {e}"));
            return Ok(());
        }
    };

    // Record the check regardless
    updater::record_check();

    match status {
        UpdateStatus::UpToDate => {
            out.success("Already up to date");
            return Ok(());
        }
        UpdateStatus::Available { ref version } => {
            out.info(&format!("New version available: {version}"));

            if check_only {
                return Ok(());
            }

            out.info(&format!("Downloading v{version}..."));

            let new_binary = match download_update(version) {
                Ok(p) => p,
                Err(e) => {
                    out.error(&format!("Download failed: {e}"));
                    return Ok(());
                }
            };

            out.info("Installing...");

            match install_binary(&new_binary, version) {
                Ok(()) => {
                    out.success(&format!("Updated zero {CURRENT_VERSION} → {version}"));
                }
                Err(e) => {
                    out.error(&format!("Install failed: {e}"));
                }
            }
        }
    }

    Ok(())
}
