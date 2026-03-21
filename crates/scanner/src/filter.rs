//! Ignore filter for file scanning
//!
//! Filters out system files and directories that should be skipped during scanning.

use std::path::Path;

/// Default patterns to ignore (macOS specific)
pub const DEFAULT_IGNORE_PATTERNS: &[&str] = &[
    ".DS_Store",
    ".Spotlight-V100",
    ".Trashes",
    ".fseventsd",
    ".TemporaryItems",
    ".VolumeIcon.icns",
    ".AppleDB",
    ".AppleDesktop",
    ".AppleDouble",
    ".apdisk",
    ".DocumentRevisions-V100",
    "._*", // AppleDouble resource fork files
    ".zero",
];

/// Filter for determining which files/directories to ignore during scanning
#[derive(Debug, Clone)]
pub struct ScanFilter {
    patterns: Vec<String>,
}

impl Default for ScanFilter {
    fn default() -> Self {
        Self::new()
    }
}

impl ScanFilter {
    /// Create a new filter with default macOS ignore patterns
    pub fn new() -> Self {
        Self {
            patterns: DEFAULT_IGNORE_PATTERNS
                .iter()
                .map(std::string::ToString::to_string)
                .collect(),
        }
    }

    /// Create an empty filter (ignore nothing)
    pub fn empty() -> Self {
        Self {
            patterns: Vec::new(),
        }
    }

    /// Add a custom pattern to ignore
    pub fn add_pattern(&mut self, pattern: impl Into<String>) {
        self.patterns.push(pattern.into());
    }

    /// Create filter with additional custom patterns
    pub fn with_patterns(mut self, patterns: impl IntoIterator<Item = impl Into<String>>) -> Self {
        for p in patterns {
            self.patterns.push(p.into());
        }
        self
    }

    /// Check if a file name should be ignored
    pub fn should_ignore(&self, file_name: &str) -> bool {
        for pattern in &self.patterns {
            if Self::matches_pattern(file_name, pattern) {
                return true;
            }
        }
        false
    }

    /// Check if a path should be ignored (checks file name component)
    ///
    /// Returns `false` for non-UTF-8 filenames (they cannot match text patterns).
    /// Callers should check `path.to_str()` separately if they need to reject
    /// non-UTF-8 paths entirely.
    pub fn should_ignore_path(&self, path: &Path) -> bool {
        let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
            // Non-UTF-8 filename — can't match any text pattern.
            // Log so the user knows a path was encountered that Zero can't fully handle.
            tracing::warn!(path = %path.display(), "non-UTF-8 filename, ignore filters skipped");
            return false;
        };
        self.should_ignore(file_name)
    }

    /// Simple pattern matching supporting:
    /// - Exact match: "foo" matches "foo"
    /// - Prefix wildcard: "*foo" matches "barfoo"
    /// - Suffix wildcard: "foo*" matches "foobar"
    /// - Contains wildcard: "*foo*" matches "barfoobaz"
    /// - Prefix pattern: "._*" matches "._anything"
    fn matches_pattern(name: &str, pattern: &str) -> bool {
        // Check for contains pattern: *foo* (wildcards on both ends)
        if pattern.starts_with('*') && pattern.ends_with('*') && pattern.len() > 2 {
            let middle = &pattern[1..pattern.len() - 1];
            // Handle nested wildcards like *.sqlite* -> check if contains ".sqlite"
            if !middle.contains('*') {
                return name.contains(middle);
            }
        }

        if let Some(suffix) = pattern.strip_prefix('*') {
            // Suffix match: *foo matches anything ending with foo
            name.ends_with(suffix)
        } else if let Some(prefix) = pattern.strip_suffix('*') {
            // Prefix match: foo* matches anything starting with foo
            name.starts_with(prefix)
        } else {
            // Exact match
            name == pattern
        }
    }
}

#[cfg(test)]
#[path = "filter_test.rs"]
mod filter_test;
