mod detail;
mod view;

pub use view::CleanupView;

use std::path::PathBuf;

/// Category scan result for display.
pub(super) struct CategoryResult {
    pub name: String,
    pub group: String,
    pub file_count: usize,
    pub total_bytes: u64,
    pub files: Vec<CategoryFileItem>,
}

/// A single file within a category result.
pub(super) struct CategoryFileItem {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
}
