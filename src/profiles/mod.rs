//! Configuration-driven profiles system
//!
//! This module provides profile-based configuration for file types and cleanup categories.
//! Profiles are stored as TOML files and loaded from:
//! 1. Embedded defaults (compiled into binary)
//! 2. User overrides (~/.config/zero/profiles/)
//!
//! ## Structure
//!
//! ```text
//! profiles/
//!   file_types.toml              # Global file type definitions
//!   cleanup/
//!     shared.toml                # Cross-platform cleanup categories
//!     macos.toml                 # macOS-only categories
//!     linux.toml                 # Linux-only categories
//!     windows.toml               # Windows-only categories
//! ```
//!
//! ## Usage
//!
//! ```rust,ignore
//! use zero::profiles::{load_file_types, load_cleanup, CleanupGroup};
//!
//! // Load file types (cached after first load)
//! let file_types = load_file_types()?;
//! let image_exts = &file_types.types["images"].extensions;
//!
//! // Load cleanup categories for current OS
//! let cleanup = load_cleanup()?;
//!
//! // Get all developer-related cleanup categories
//! for category in cleanup.categories_by_group(CleanupGroup::Developer) {
//!     println!("{}: {:?}", category.name, category.patterns);
//! }
//! ```

mod error;
mod loader;
mod schema;

#[cfg(test)]
mod loader_test;

// Re-export public types
pub use error::{ProfileError, ProfileResult};
pub use loader::{
    MergedCleanupProfile, ResolvedCleanupCategory, ensure_user_profiles_dir, has_user_overrides,
    load_cleanup, load_cleanup_for_os, load_file_types, user_profiles_dir,
};
pub use schema::{
    CleanupCategory, CleanupGroup, CleanupProfile, FileTypeConfig, FileTypesProfile, MatchPatterns,
    Os,
};
