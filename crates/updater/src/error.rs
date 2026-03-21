//! Update error types

use thiserror::Error;

#[derive(Debug, Error)]
pub enum UpdateError {
    #[error("network error: {0}")]
    Network(String),

    #[error("checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },

    #[error("extraction error: {0}")]
    Extract(String),

    #[error("install error: {0}")]
    Install(String),

    #[error("parse error: {0}")]
    Parse(String),
}
