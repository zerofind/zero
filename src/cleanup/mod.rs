//! Cleanup module - Smart storage cleanup using the search index
//!
//! This module provides profile-based cleanup queries that translate
//! cleanup categories from TOML configuration into index queries.
//!
//! ## Architecture
//!
//! Categories are defined in TOML profile files:
//! - `profiles/cleanup/shared.toml` - Cross-platform categories
//! - `profiles/cleanup/macos.toml` - macOS-specific categories
//!
//! Categories are grouped for UI display (Apple Storage style):
//! - Developer: Xcode, node_modules, Rust target, etc.
//! - System: Caches, logs, .DS_Store, Spotlight indexes
//! - Documents: Downloads, disk images
//! - Media: Photos, Music caches
//! - Mail: Mail attachments
//! - Messages: iMessage attachments
//! - iOS: iOS backups, device support
//! - Trash: Trash contents
//!
//! ## Usage
//!
//! ```ignore
//! use zero::cleanup::{execute_full_cleanup_scan, execute_group_cleanup};
//! use zero::profiles::CleanupGroup;
//!
//! // Scan all categories grouped by UI group
//! let summary = execute_full_cleanup_scan(&index_manager)?;
//! for group in summary.groups {
//!     println!("{}: {} bytes", group.group_name, group.total_bytes);
//! }
//!
//! // Scan a specific group
//! let dev_summary = execute_group_cleanup(&index_manager, CleanupGroup::Developer)?;
//! ```

pub mod dev_scan;
mod profile_query;

#[cfg(test)]
mod dev_scan_test;
#[cfg(test)]
mod profile_query_test;

// Dev garbage scanner
pub use dev_scan::{
    DevGarbageItem, DevScanError, DevScanProgress, DevScanResult, scan_dev_garbage,
};

// Profile-based cleanup (new system)
pub use profile_query::{
    GroupSummary, ProfileCleanupItem, ProfileCleanupQuery, ProfileCleanupResult,
    ProfileCleanupSummary, execute_category_cleanup, execute_full_cleanup_scan,
    execute_group_cleanup,
};
