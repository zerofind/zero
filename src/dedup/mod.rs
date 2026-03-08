//! Dedup module - find and remove duplicate files
//!
//! Finds duplicate files by:
//! 1. Grouping files by size (fast filter - different sizes can't be duplicates)
//! 2. Hashing only files with matching sizes
//! 3. Grouping by hash to find true duplicates
//!
//! ## Usage
//!
//! ```ignore
//! use zero::dedup::{find_duplicates, DedupOptions};
//!
//! let options = DedupOptions::default();
//! let result = find_duplicates("/path/to/folder", options)?;
//!
//! for group in result.groups {
//!     println!("Duplicate group: {} files, {} bytes each", group.files.len(), group.size);
//!     for file in &group.files {
//!         println!("  {}", file.display());
//!     }
//! }
//! ```

mod finder;
mod stream;
mod types;

pub use finder::{delete_duplicates, find_duplicates, find_duplicates_with_progress};
pub use stream::{DedupStreamEvent, find_duplicates_streaming};
pub use types::{
    DedupError, DedupOptions, DedupProgress, DedupResult, DeleteResult, DuplicateGroup,
};
