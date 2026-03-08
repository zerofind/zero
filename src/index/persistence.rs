//! Flat compressed snapshot persistence for SearchIndex.
//!
//! Format: `[u8; 4]` magic `b"ZDX1"` + postcard-encoded `(Vec<String>, Vec<FileNode>)`,
//! zstd-compressed. Single `.zidx` file per root.

use std::fs;
use std::io::Cursor;
use std::path::Path;

use super::node::FileNode;
use super::search::{IndexError, SearchIndex};

const MAGIC: [u8; 4] = *b"ZDX1";
const ZSTD_LEVEL: i32 = 3;

/// Save a SearchIndex as a compressed snapshot.
///
/// Writes to a temp file then atomically renames for crash safety.
pub fn save_index(index: &SearchIndex, path: &Path) -> Result<(), IndexError> {
    let roots = index.roots().to_vec();
    let nodes: Vec<FileNode> = index.iter().collect();

    let payload = postcard::to_allocvec(&(&roots, &nodes))
        .map_err(|e| IndexError::Serialize(e.to_string()))?;

    let compressed = zstd::bulk::compress(&payload, ZSTD_LEVEL).map_err(IndexError::Io)?;

    // Atomic write: tmp file + rename
    let tmp = path.with_extension("zidx.tmp");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&tmp, [MAGIC.as_slice(), &compressed].concat())?;
    fs::rename(&tmp, path)?;

    Ok(())
}

/// Load a SearchIndex from a compressed snapshot.
pub fn load_index(path: &Path) -> Result<SearchIndex, IndexError> {
    let data = fs::read(path)?;

    if data.len() < MAGIC.len() || data[..4] != MAGIC {
        return Err(IndexError::Serialize("invalid snapshot magic".into()));
    }

    let decompressed = zstd::stream::decode_all(Cursor::new(&data[4..])).map_err(IndexError::Io)?;

    let (roots, nodes): (Vec<String>, Vec<FileNode>) =
        postcard::from_bytes(&decompressed).map_err(|e| IndexError::Serialize(e.to_string()))?;

    let mut index = SearchIndex::with_capacity(nodes.len());
    index.set_roots(roots);
    for node in nodes {
        index.insert(node);
    }
    index.finalize();

    Ok(index)
}
