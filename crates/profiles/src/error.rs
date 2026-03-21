//! Profile error types
//!
//! Defines errors that can occur when loading and parsing profile files.

use std::path::PathBuf;

/// Errors that can occur when working with profiles
#[derive(Debug, thiserror::Error)]
pub enum ProfileError {
    /// Failed to parse TOML content
    #[error("Failed to parse profile '{path}': {source}")]
    Parse {
        path: String,
        #[source]
        source: toml::de::Error,
    },

    /// Failed to read profile file
    #[error("Failed to read profile file '{path}': {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Invalid glob pattern in profile
    #[error("Invalid glob pattern '{pattern}' in category '{category}': {reason}")]
    InvalidPattern {
        category: String,
        pattern: String,
        reason: String,
    },

    /// Invalid duration format (e.g., "30d", "24h")
    #[error(
        "Invalid duration format '{value}' in category '{category}': expected format like '30d', '24h', or '60m'"
    )]
    InvalidDuration { category: String, value: String },

    /// Invalid size format (e.g., "100MB", "1GB")
    #[error(
        "Invalid size format '{value}' in category '{category}': expected format like '100MB', '1GB', or '500KB'"
    )]
    InvalidSize { category: String, value: String },

    /// Missing required field
    #[error("Missing required field '{field}' in category '{category}'")]
    MissingField { category: String, field: String },

    /// Duplicate category ID across files
    #[error("Duplicate category ID '{id}' found in both '{first_file}' and '{second_file}'")]
    DuplicateCategory {
        id: String,
        first_file: String,
        second_file: String,
    },

    /// Profile not found
    #[error("Profile not found: {0}")]
    NotFound(String),
}

impl ProfileError {
    /// Create a parse error
    pub fn parse(path: impl Into<String>, source: toml::de::Error) -> Self {
        ProfileError::Parse {
            path: path.into(),
            source,
        }
    }

    /// Create an IO error
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        ProfileError::Io {
            path: path.into(),
            source,
        }
    }

    /// Create an invalid pattern error
    pub fn invalid_pattern(
        category: impl Into<String>,
        pattern: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        ProfileError::InvalidPattern {
            category: category.into(),
            pattern: pattern.into(),
            reason: reason.into(),
        }
    }

    /// Create an invalid duration error
    pub fn invalid_duration(category: impl Into<String>, value: impl Into<String>) -> Self {
        ProfileError::InvalidDuration {
            category: category.into(),
            value: value.into(),
        }
    }

    /// Create an invalid size error
    pub fn invalid_size(category: impl Into<String>, value: impl Into<String>) -> Self {
        ProfileError::InvalidSize {
            category: category.into(),
            value: value.into(),
        }
    }

    /// Create a missing field error
    pub fn missing_field(category: impl Into<String>, field: impl Into<String>) -> Self {
        ProfileError::MissingField {
            category: category.into(),
            field: field.into(),
        }
    }
}

/// Result type for profile operations
pub type ProfileResult<T> = Result<T, ProfileError>;
