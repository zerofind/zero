//! Anonymous install ID generation
//!
//! Produces a deterministic, non-reversible identifier from machine attributes.
//! Same machine always generates the same ID. Cannot be reversed to recover
//! the home path or any personal information.

use sha2::{Digest, Sha256};

/// Generate an anonymous install ID from machine attributes.
///
/// `SHA256(home_path + os + arch)` truncated to first 16 bytes (32 hex chars).
/// Deterministic: same machine always produces the same ID.
pub fn generate_install_id() -> String {
    let home = dirs::home_dir()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;

    let input = format!("{home}{os}{arch}");
    let hash = Sha256::digest(input.as_bytes());
    hex::encode(&hash[..16])
}
