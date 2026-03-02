//! Hasher module - fast file hashing with multiple algorithm support
//!
//! Provides streaming hash computation for files with support for:
//! - blake3 (default) - cryptographic, very fast, recommended for integrity
//! - xxh3 - non-cryptographic, extremely fast, good for dedup/comparison

mod algorithm;
mod hash;

pub use algorithm::HashAlgorithm;
pub use hash::{
    HashError, HashResult, hash_file, hash_file_with_atomic_progress, hash_file_with_buffer,
    hash_file_with_progress,
};
