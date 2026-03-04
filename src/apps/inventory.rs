use std::path::PathBuf;
use std::time::SystemTime;

/// Where the package came from.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PackageSource {
    /// /Applications or /System/Applications .app bundle
    MacApp,
    /// Homebrew formula (CLI tools)
    Homebrew,
    /// Homebrew cask (GUI apps installed via brew)
    HomebrewCask,
    // Future: Npm, Pip, Cargo, SystemExtension, …
}

/// Unified metadata for any installed package.
#[derive(Debug, Clone)]
pub struct PackageInfo {
    /// Display name (e.g. "Firefox", "ripgrep")
    pub name: String,
    /// Unique identifier when available (e.g. "org.mozilla.firefox", "ripgrep")
    pub bundle_id: Option<String>,
    /// Version string from Info.plist / brew / etc.
    pub version: Option<String>,
    /// Install location on disk
    pub path: PathBuf,
    /// Where this package came from
    pub source: PackageSource,
    /// When first installed (from receipts / brew log)
    pub installed_at: Option<SystemTime>,
    /// Last modification time of the bundle or binary
    pub updated_at: Option<SystemTime>,
    /// Total size in bytes
    pub size: Option<u64>,
}
