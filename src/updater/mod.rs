//! Auto-update support for Zero
//!
//! Checks for new versions from the R2 CDN, downloads verified tarballs,
//! and swaps the binary in place.

pub mod check;
pub mod download;
pub mod error;
pub mod install;

pub use check::{UpdateStatus, check_latest, read_auto_update_setting, record_check, should_check};
pub use download::download_update;
pub use error::UpdateError;
pub use install::install_binary;

/// Base URL for release artifacts on R2 CDN
pub const DL_BASE: &str = "https://pub-2f61b70db2ea4fcda60d3c48f5b750f4.r2.dev";

/// Current version of this binary
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Returns (os, arch) strings matching the artifact naming in release.yml
pub fn platform_target() -> (&'static str, &'static str) {
    let os = if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        "unknown"
    };

    let arch = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else {
        "unknown"
    };

    (os, arch)
}
