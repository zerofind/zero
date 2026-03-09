//! Flat compressed snapshot persistence for code indexes.
//!
//! Format: `[u8; 4]` magic `b"CDX1"` + postcard-encoded `ProjectSummary`,
//! zstd-compressed. Single `.cidx` file per project.

use std::fs;
use std::io::Cursor;
use std::path::Path;

use super::element::ProjectSummary;

const MAGIC: [u8; 4] = *b"CDX1";
const ZSTD_LEVEL: i32 = 3;

/// Save a ProjectSummary as a compressed .cidx snapshot.
///
/// Writes to a temp file then atomically renames for crash safety.
pub fn save_summary(summary: &ProjectSummary, path: &Path) -> Result<(), CodePersistError> {
    let payload =
        postcard::to_allocvec(summary).map_err(|e| CodePersistError::Serialize(e.to_string()))?;

    let compressed = zstd::bulk::compress(&payload, ZSTD_LEVEL).map_err(CodePersistError::Io)?;

    let tmp = path.with_extension("cidx.tmp");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&tmp, [MAGIC.as_slice(), &compressed].concat())?;
    fs::rename(&tmp, path)?;

    Ok(())
}

/// Load a ProjectSummary from a compressed .cidx snapshot.
pub fn load_summary(path: &Path) -> Result<ProjectSummary, CodePersistError> {
    let data = fs::read(path)?;

    if data.len() < MAGIC.len() || data[..4] != MAGIC {
        return Err(CodePersistError::Serialize(
            "invalid .cidx snapshot magic".into(),
        ));
    }

    let decompressed =
        zstd::stream::decode_all(Cursor::new(&data[4..])).map_err(CodePersistError::Io)?;

    let summary: ProjectSummary = postcard::from_bytes(&decompressed)
        .map_err(|e| CodePersistError::Serialize(e.to_string()))?;

    Ok(summary)
}

#[derive(Debug, thiserror::Error)]
pub enum CodePersistError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialize(String),
}

#[cfg(test)]
#[path = "persistence_test.rs"]
mod persistence_test;
