use std::path::PathBuf;

/// Whether the clipboard represents a copy or a cut (move) operation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ClipboardOperation {
    Copy,
    Cut,
}

/// Holds file paths that have been copied or cut.
#[derive(Debug, Clone)]
pub struct FileClipboard {
    pub paths: Vec<PathBuf>,
    pub operation: ClipboardOperation,
}

impl FileClipboard {
    pub fn new(paths: Vec<PathBuf>, operation: ClipboardOperation) -> Self {
        Self { paths, operation }
    }
}
