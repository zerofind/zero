//! Configuration-driven profiles system
//!
//! This module provides profile-based configuration for file types and cleanup categories.
//! Profiles are stored as TOML files and loaded from:
//! 1. Embedded defaults (compiled into binary)
//! 2. User overrides (~/.zero/profiles/)

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
    AggregateMode, CleanupCategory, CleanupGroup, CleanupProfile, FileTypeConfig, FileTypesProfile,
    MatchPatterns, Os,
};
