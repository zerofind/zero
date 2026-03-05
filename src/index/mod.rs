//! Fast file search index - Cardinal-style implementation
//!
//! This module provides blazing fast file search using:
//! - In-memory slab storage for file nodes
//! - BTreeMap name index for O(log n) lookups
//! - Substring matching on filenames
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────┐     ┌──────────────────────────────┐
//! │ Name Index  │────▶│ Slab (file nodes by index)   │
//! │ BTreeMap    │     │ [0] FileNode { path }        │
//! │ "foo" → [0] │     │ [1] FileNode { path }        │
//! │ "bar" → [1] │     │ ...                          │
//! └─────────────┘     └──────────────────────────────┘
//! ```
//!
//! ## Multi-Root Architecture (IndexManager)
//!
//! For managing multiple indexed locations with instant add/remove:
//!
//! ```text
//! ~/.zero/indexes/
//!   a1b2c3.zidx             # compressed snapshot for /Users/foo
//!   d4e5f6.zidx             # compressed snapshot for /Volumes/External
//! ```
//!
//! Root registry is stored in ControlDb.
//!
//! ## Usage
//!
//! ### Single Index
//!
//! ```no_run
//! use std::path::Path;
//! use zero::index::SearchIndex;
//!
//! // Build index from directory
//! let mut index = SearchIndex::new();
//! index.build_from_path(Path::new("/Users/me/Documents")).unwrap();
//!
//! // Search
//! let results = index.search("report", 100);
//! for result in results {
//!     println!("{}", result.node.path);
//! }
//! ```
//!
//! ### Multi-Root Index Manager (recommended)
//!
//! ```no_run
//! use zero::index::IndexManager;
//!
//! let mut manager = IndexManager::load().unwrap();
//!
//! // Add a root (builds index)
//! manager.add_root("/Users/me/Documents").unwrap();
//!
//! // Search across all indexes in parallel
//! let results = manager.search("report", 100);
//!
//! // Remove a root (instant!)
//! manager.remove_root("/Users/me/Documents");
//! ```

pub(crate) mod arena;
mod manager;
mod node;
pub mod persistence;
mod search;
pub mod type_index;
mod watcher;

pub use manager::{
    IndexManager, RootStats, SharedIndexManager, default_indexes_dir, hash_path,
    load_index_snapshot,
};
pub use node::{FileNode, NodeType};
pub use search::{IndexError, SearchIndex, SearchOptions, SearchQuery, SearchResult, SortBy};
pub use type_index::{FileTypeCategory, TypeIndex, TypeIndexStats};
pub use watcher::{IndexWatcher, WatcherConfig, WatcherError, WatcherStats};

#[cfg(test)]
mod arena_test;
#[cfg(test)]
mod persistence_test;
#[cfg(test)]
mod tests;
