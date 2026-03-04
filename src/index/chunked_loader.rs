//! Chunked index loader for progressive search availability.
//!
//! Instead of loading a root's entire index at once (which blocks search
//! for 10-15s on large roots), this module deserializes the full snapshot
//! from etch, extracts the raw `FileNode` slab, and replays it in chunks
//! of configurable size.  After each chunk is inserted into a fresh
//! `SearchIndex`, that partial index is immediately searchable.

use std::path::Path;

use super::etch::open_index_store;
use super::node::FileNode;
use super::search::IndexError;

/// Iterates over deserialized nodes in fixed-size chunks.
pub struct ChunkedIndexLoader {
    nodes: Vec<FileNode>,
    position: usize,
    chunk_size: usize,
}

impl ChunkedIndexLoader {
    /// Total number of nodes to replay.
    pub fn total(&self) -> usize {
        self.nodes.len()
    }

    /// Number of nodes already yielded.
    pub fn loaded(&self) -> usize {
        self.position
    }

    /// Take the next chunk of nodes.  Returns `None` when all nodes
    /// have been yielded.
    pub fn next_chunk(&mut self) -> Option<Vec<FileNode>> {
        if self.position >= self.nodes.len() {
            return None;
        }
        let end = (self.position + self.chunk_size).min(self.nodes.len());
        let chunk = self.nodes[self.position..end].to_vec();
        self.position = end;
        Some(chunk)
    }
}

/// Deserialize a root's index from its etch directory and prepare it
/// for chunked replay.
///
/// The heavy deserialization (postcard + zstd) happens once up-front.
/// The caller then calls `next_chunk()` repeatedly to feed batches
/// into a `SearchIndex` on the main thread.
pub fn load_index_chunked(
    etch_dir: &Path,
    chunk_size: usize,
) -> Result<ChunkedIndexLoader, IndexError> {
    let store = open_index_store(etch_dir)?;
    let full_index = store.read().clone();
    let nodes = full_index.into_nodes();

    Ok(ChunkedIndexLoader {
        nodes,
        position: 0,
        chunk_size,
    })
}
