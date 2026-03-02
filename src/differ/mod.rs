//! Differ module - compare two directory trees
//!
//! Compares scan results from two directories and produces a diff showing:
//! - Files only in source (to be added)
//! - Files only in destination (orphans, may be deleted)
//! - Files in both but different (modified)
//! - Files in both and identical (unchanged)

mod checksum;
mod compare;
mod result;

pub use compare::{
    ComputedHash, DiffOptions, DiffProgress, diff, diff_with_atomic_progress, diff_with_options,
    diff_with_progress, diff_with_progress_and_hash_streaming, diff_with_progress_and_hashes,
};
pub use result::{DiffEntry, DiffResult, DiffStatus};
