pub mod clipboard;
pub mod pane;

pub use clipboard::{ClipboardOperation, FileClipboard};
pub use pane::{PaneId, SplitPane};

/// Which view is currently displayed in the content area.
#[derive(Debug, Clone, PartialEq)]
pub enum ActiveView {
    FileBrowser,
    Cleanup,
    Dedup,
    Todo,
    Settings,
    Editor(std::path::PathBuf),
    DataTable(std::path::PathBuf),
    SecureErase,
    Automations,
}

/// File browser display mode.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ViewMode {
    List,
    Grid,
}

/// Column to sort the file browser by.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SortField {
    Name,
    DateModified,
    Size,
    Kind,
}

/// Sort direction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SortDirection {
    Ascending,
    Descending,
}

impl SortDirection {
    pub fn toggle(self) -> Self {
        match self {
            Self::Ascending => Self::Descending,
            Self::Descending => Self::Ascending,
        }
    }
}
