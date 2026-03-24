use std::fmt::Write as _;
use std::path::PathBuf;

use rand::Rng as _;

/// Generate a random 32-byte hex API key.
pub fn generate_api_key() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill(&mut bytes);
    bytes.iter().fold(String::with_capacity(64), |mut acc, b| {
        let _ = write!(acc, "{b:02x}");
        acc
    })
}

/// Path to the persisted API key file.
fn key_path() -> Option<PathBuf> {
    foundation::dirs::data_dir().map(|d| d.join("mcp_api_key"))
}

/// Load existing API key or create a new one.
/// File is created with 0o600 permissions on Unix.
pub fn load_or_create_api_key() -> String {
    if let Some(path) = key_path() {
        if let Ok(key) = std::fs::read_to_string(&path) {
            let key = key.trim().to_string();
            if key.len() >= 32 {
                return key;
            }
        }

        let key = generate_api_key();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&path, &key);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }

        key
    } else {
        generate_api_key()
    }
}
