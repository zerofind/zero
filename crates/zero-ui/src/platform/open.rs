//! NSWorkspace wrappers — open files and reveal in Finder.
//!
//! Stubbed out for now; will integrate with objc2 when API is stable.

use std::path::Path;

/// Open a file or folder with the default application.
pub fn open_path(path: &Path) {
    // Use `open` command as a simple fallback
    let _ = std::process::Command::new("open").arg(path).spawn();
}

/// Reveal a file in Finder (select it in its parent folder).
pub fn reveal_in_finder(path: &Path) {
    let _ = std::process::Command::new("open")
        .args(["-R", &path.to_string_lossy()])
        .spawn();
}

/// Eject a mounted volume. Returns `Ok(())` on success.
pub fn eject_drive(mount_point: &Path) -> Result<(), String> {
    let output = std::process::Command::new("diskutil")
        .arg("eject")
        .arg(mount_point)
        .output()
        .map_err(|e| format!("Failed to run diskutil: {e}"))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("Eject failed: {}", stderr.trim()))
    }
}
