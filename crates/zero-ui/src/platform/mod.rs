#[cfg(target_os = "macos")]
pub mod open;
#[cfg(target_os = "macos")]
pub mod quicklook;
pub mod traffic_lights;
#[cfg(target_os = "macos")]
pub mod trash;

#[cfg(not(target_os = "macos"))]
pub mod trash {
    use std::path::Path;

    pub fn move_to_trash(path: &Path) -> Result<(), String> {
        if path.is_dir() {
            std::fs::remove_dir_all(path)
        } else {
            std::fs::remove_file(path)
        }
        .map_err(|e| format!("failed to remove: {e}"))
    }
}
