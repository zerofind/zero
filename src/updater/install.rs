//! Binary installation (atomic swap)

use std::fs;
use std::path::Path;

use super::UpdateError;

/// Install a new binary by atomically swapping it with the current one.
///
/// Steps:
/// 1. Rename current binary to `{path}.bak`
/// 2. Copy new binary to `{path}`
/// 3. Set executable permissions
/// 4. Remove `.bak`
pub fn install_binary(new_binary: &Path) -> Result<(), UpdateError> {
    let current = std::env::current_exe().map_err(|e| UpdateError::Install(e.to_string()))?;

    let backup = current.with_extension("bak");

    // Rename current → backup
    fs::rename(&current, &backup).map_err(|e| {
        UpdateError::Install(format!(
            "failed to back up current binary {}: {}",
            current.display(),
            e
        ))
    })?;

    // Copy new → current (rename may fail across filesystems)
    let result = fs::copy(new_binary, &current);

    match result {
        Ok(_) => {
            // Set executable permissions
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let perms = fs::Permissions::from_mode(0o755);
                fs::set_permissions(&current, perms).map_err(|e| {
                    UpdateError::Install(format!("failed to set permissions: {}", e))
                })?;
            }

            // Clean up backup and temp file
            let _ = fs::remove_file(&backup);
            let _ = fs::remove_file(new_binary);

            Ok(())
        }
        Err(e) => {
            // Rollback: restore backup
            let _ = fs::rename(&backup, &current);
            Err(UpdateError::Install(format!(
                "failed to install new binary: {}",
                e
            )))
        }
    }
}
