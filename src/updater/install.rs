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
/// 5. Update app bundle Info.plist version if inside a .app bundle
pub fn install_binary(new_binary: &Path, version: &str) -> Result<(), UpdateError> {
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

            // Update Info.plist if binary lives inside a .app bundle
            update_bundle_version(&current, version);

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

/// If the binary is inside a macOS .app bundle, update the Info.plist version.
/// Looks for `Contents/MacOS/<binary>` pattern and writes to sibling `Info.plist`.
/// Failures are non-fatal — the binary is already updated.
fn update_bundle_version(binary_path: &Path, version: &str) {
    // Expected: .../Zero.app/Contents/MacOS/zero
    let Some(macos_dir) = binary_path.parent() else {
        return;
    };
    if macos_dir.file_name().and_then(|n| n.to_str()) != Some("MacOS") {
        return;
    }
    let Some(contents_dir) = macos_dir.parent() else {
        return;
    };
    if contents_dir.file_name().and_then(|n| n.to_str()) != Some("Contents") {
        return;
    }

    let plist_path = contents_dir.join("Info.plist");
    if !plist_path.exists() {
        return;
    }

    let Ok(plist) = fs::read_to_string(&plist_path) else {
        return;
    };

    // Replace version strings in the plist
    let updated = replace_plist_value(&plist, "CFBundleVersion", version);
    let updated = replace_plist_value(&updated, "CFBundleShortVersionString", version);

    let _ = fs::write(&plist_path, updated);
}

/// Replace the string value following a given key in a plist XML.
fn replace_plist_value(plist: &str, key: &str, value: &str) -> String {
    let needle = format!("<key>{key}</key>");
    let Some(key_pos) = plist.find(&needle) else {
        return plist.to_string();
    };
    let after_key = key_pos + needle.len();
    let Some(str_start) = plist[after_key..].find("<string>") else {
        return plist.to_string();
    };
    let str_start = after_key + str_start + "<string>".len();
    let Some(str_end) = plist[str_start..].find("</string>") else {
        return plist.to_string();
    };
    let str_end = str_start + str_end;

    format!("{}{}{}", &plist[..str_start], value, &plist[str_end..])
}
