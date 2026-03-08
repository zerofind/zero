//! QuickLook integration — open/close QLPreviewPanel for a file path.
//!
//! Uses `qlmanage` command as a simple fallback until objc2 integration is ready.

use std::path::Path;

/// Show the system QuickLook panel for the given file path.
pub fn preview_file(path: &Path) {
    let _ = std::process::Command::new("qlmanage")
        .args(["-p", &path.to_string_lossy()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}
