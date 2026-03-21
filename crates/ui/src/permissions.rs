/// Check whether this process has Full Disk Access (FDA) on macOS.
///
/// Uses the same probe paths as the FullDiskAccess Swift package:
/// - macOS 12+: ~/Library/Containers/com.apple.stocks
/// - macOS 10.14–11: ~/Library/Safari
///
/// Attempting to read these TCC-protected directories also registers
/// the app in System Settings > Full Disk Access (unchecked).
///
/// On non-macOS platforms, always returns `true`.
pub fn has_full_disk_access() -> bool {
    #[cfg(target_os = "macos")]
    {
        let Some(home) = dirs::home_dir() else {
            return true;
        };

        // macOS 12+ (Monterey, Ventura, Sonoma, Sequoia, Tahoe)
        let probe = home.join("Library/Containers/com.apple.stocks");
        match std::fs::read_dir(&probe) {
            Ok(_) => return true,
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => return false,
            _ => {}
        }

        // Fallback for macOS 10.14–11
        let probe = home.join("Library/Safari");
        match std::fs::read_dir(&probe) {
            Ok(_) => true,
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => false,
            // Directory doesn't exist or other error — can't determine, assume granted
            _ => true,
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

/// Open macOS System Settings directly to the Full Disk Access pane.
#[cfg(target_os = "macos")]
pub fn open_full_disk_access_settings() {
    let _ = std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles")
        .spawn();
}
