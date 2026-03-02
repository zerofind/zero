//! Profile schema definitions for TOML parsing
//!
//! Defines the structure of profile configuration files:
//! - `file_types.toml` - File type classifications by extension
//! - `cleanup/*.toml` - Cleanup category definitions

use serde::Deserialize;
use std::collections::HashMap;

// =============================================================================
// File Types Schema
// =============================================================================

/// Root structure for file_types.toml
#[derive(Debug, Clone, Deserialize)]
#[serde(transparent)]
pub struct FileTypesProfile {
    /// Map of category name -> file type config
    pub types: HashMap<String, FileTypeConfig>,
}

/// Configuration for a single file type category
#[derive(Debug, Clone, Deserialize)]
pub struct FileTypeConfig {
    /// File extensions belonging to this category (without dots)
    pub extensions: Vec<String>,
}

// =============================================================================
// Cleanup Profile Schema
// =============================================================================

/// Root structure for cleanup TOML files
#[derive(Debug, Clone, Deserialize)]
#[serde(transparent)]
pub struct CleanupProfile {
    /// Map of category ID -> cleanup category config
    pub categories: HashMap<String, CleanupCategory>,
}

/// Configuration for a single cleanup category
#[derive(Debug, Clone, Deserialize)]
pub struct CleanupCategory {
    /// Display name shown in UI
    pub name: String,

    /// Glob patterns to match files/directories
    #[serde(default)]
    pub patterns: MatchPatterns,

    /// UI grouping (developer, system, media, etc.)
    pub group: CleanupGroup,

    /// Optional help text for users
    #[serde(default)]
    pub description: Option<String>,

    /// Optional warning shown before deletion
    #[serde(default)]
    pub warning: Option<String>,

    /// Minimum file age filter (e.g., "30d", "24h")
    #[serde(default)]
    pub min_age: Option<String>,

    /// Minimum file size filter (e.g., "100MB", "1GB")
    #[serde(default)]
    pub min_size: Option<String>,

    /// Paths to exclude from matching
    #[serde(default)]
    pub exclude: Vec<String>,
}

/// UI grouping for cleanup categories (Apple Storage style)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CleanupGroup {
    Developer,
    System,
    Documents,
    Media,
    Mail,
    Messages,
    Ios,
    Trash,
}

impl CleanupGroup {
    /// Get display name for the group
    pub fn display_name(&self) -> &'static str {
        match self {
            CleanupGroup::Developer => "Developer",
            CleanupGroup::System => "System",
            CleanupGroup::Documents => "Documents",
            CleanupGroup::Media => "Media",
            CleanupGroup::Mail => "Mail",
            CleanupGroup::Messages => "Messages",
            CleanupGroup::Ios => "iOS Files",
            CleanupGroup::Trash => "Trash",
        }
    }

    /// Get SF Symbol icon name for the group
    pub fn icon(&self) -> &'static str {
        match self {
            CleanupGroup::Developer => "hammer",
            CleanupGroup::System => "gearshape",
            CleanupGroup::Documents => "doc",
            CleanupGroup::Media => "photo.on.rectangle",
            CleanupGroup::Mail => "envelope",
            CleanupGroup::Messages => "message",
            CleanupGroup::Ios => "iphone",
            CleanupGroup::Trash => "trash",
        }
    }

    /// All groups in display order
    pub fn all() -> &'static [CleanupGroup] {
        &[
            CleanupGroup::Developer,
            CleanupGroup::System,
            CleanupGroup::Documents,
            CleanupGroup::Media,
            CleanupGroup::Mail,
            CleanupGroup::Messages,
            CleanupGroup::Ios,
            CleanupGroup::Trash,
        ]
    }
}

/// Pattern matching configuration
///
/// Supports both universal patterns and per-OS patterns:
/// ```toml
/// # Universal (same on all OSes)
/// patterns = ["**/node_modules"]
///
/// # Per-OS (different paths)
/// patterns.macos = ["~/Library/Caches"]
/// patterns.linux = ["~/.cache"]
/// ```
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(untagged)]
pub enum MatchPatterns {
    /// Same patterns for all operating systems
    Universal(Vec<String>),

    /// Different patterns per OS
    PerOs {
        #[serde(default)]
        macos: Option<Vec<String>>,
        #[serde(default)]
        linux: Option<Vec<String>>,
        #[serde(default)]
        windows: Option<Vec<String>>,
    },

    /// No patterns specified
    #[default]
    None,
}

impl MatchPatterns {
    /// Get patterns for the current OS
    #[cfg(target_os = "macos")]
    pub fn for_current_os(&self) -> Vec<&str> {
        match self {
            MatchPatterns::Universal(patterns) => patterns.iter().map(|s| s.as_str()).collect(),
            MatchPatterns::PerOs { macos, .. } => macos
                .as_ref()
                .map(|p| p.iter().map(|s| s.as_str()).collect())
                .unwrap_or_default(),
            MatchPatterns::None => Vec::new(),
        }
    }

    /// Get patterns for the current OS
    #[cfg(target_os = "linux")]
    pub fn for_current_os(&self) -> Vec<&str> {
        match self {
            MatchPatterns::Universal(patterns) => patterns.iter().map(|s| s.as_str()).collect(),
            MatchPatterns::PerOs { linux, .. } => linux
                .as_ref()
                .map(|p| p.iter().map(|s| s.as_str()).collect())
                .unwrap_or_default(),
            MatchPatterns::None => Vec::new(),
        }
    }

    /// Get patterns for the current OS
    #[cfg(target_os = "windows")]
    pub fn for_current_os(&self) -> Vec<&str> {
        match self {
            MatchPatterns::Universal(patterns) => patterns.iter().map(|s| s.as_str()).collect(),
            MatchPatterns::PerOs { windows, .. } => windows
                .as_ref()
                .map(|p| p.iter().map(|s| s.as_str()).collect())
                .unwrap_or_default(),
            MatchPatterns::None => Vec::new(),
        }
    }

    /// Get patterns for a specific OS (for testing or cross-platform tooling)
    pub fn for_os(&self, os: Os) -> Vec<&str> {
        match self {
            MatchPatterns::Universal(patterns) => patterns.iter().map(|s| s.as_str()).collect(),
            MatchPatterns::PerOs {
                macos,
                linux,
                windows,
            } => {
                let patterns = match os {
                    Os::MacOs => macos,
                    Os::Linux => linux,
                    Os::Windows => windows,
                };
                patterns
                    .as_ref()
                    .map(|p| p.iter().map(|s| s.as_str()).collect())
                    .unwrap_or_default()
            }
            MatchPatterns::None => Vec::new(),
        }
    }

    /// Check if any patterns are defined
    pub fn is_empty(&self) -> bool {
        match self {
            MatchPatterns::Universal(patterns) => patterns.is_empty(),
            MatchPatterns::PerOs {
                macos,
                linux,
                windows,
            } => {
                macos.as_ref().is_none_or(|p| p.is_empty())
                    && linux.as_ref().is_none_or(|p| p.is_empty())
                    && windows.as_ref().is_none_or(|p| p.is_empty())
            }
            MatchPatterns::None => true,
        }
    }
}

/// Operating system identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    MacOs,
    Linux,
    Windows,
}

impl Os {
    /// Get the current operating system
    #[cfg(target_os = "macos")]
    pub fn current() -> Self {
        Os::MacOs
    }

    #[cfg(target_os = "linux")]
    pub fn current() -> Self {
        Os::Linux
    }

    #[cfg(target_os = "windows")]
    pub fn current() -> Self {
        Os::Windows
    }

    /// Get the profile filename for this OS
    pub fn profile_filename(&self) -> &'static str {
        match self {
            Os::MacOs => "macos.toml",
            Os::Linux => "linux.toml",
            Os::Windows => "windows.toml",
        }
    }
}
