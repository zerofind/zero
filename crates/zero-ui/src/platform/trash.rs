//! Move files to macOS Trash using NSFileManager.
//!
//! Uses `NSFileManager.trashItemAtURL:resultingItemURL:error:` which:
//! - Does NOT require Finder automation permissions (unlike AppleScript)
//! - Supports "Put Back" functionality
//! - Works in sandboxed and non-sandboxed apps

use std::path::Path;

use objc2_foundation::{NSFileManager, NSString, NSURL};

/// Move a file or directory to the macOS Trash.
pub fn move_to_trash(path: &Path) -> Result<(), String> {
    let abs_path = path
        .canonicalize()
        .map_err(|e| format!("cannot resolve path: {e}"))?;
    let path_str = abs_path.to_string_lossy();

    let ns_path = NSString::from_str(&path_str);
    let url = NSURL::fileURLWithPath(&ns_path);
    let fm = NSFileManager::defaultManager();

    fm.trashItemAtURL_resultingItemURL_error(&url, None)
        .map_err(|e| e.localizedDescription().to_string())
}
