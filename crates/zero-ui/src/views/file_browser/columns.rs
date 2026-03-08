use gpui::Pixels;
use gpui::px;
use serde::{Deserialize, Serialize};

/// Identifies a column in the file browser table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileColumn {
    Name,
    DateModified,
    Size,
    Kind,
    Permissions,
    Owner,
    #[serde(alias = "flags")]
    Attributes,
}

impl FileColumn {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Name => "Name",
            Self::DateModified => "Date Modified",
            Self::Size => "Size",
            Self::Kind => "Kind",
            Self::Permissions => "Permissions",
            Self::Owner => "Owner",
            Self::Attributes => "Attributes",
        }
    }

    pub fn default_width(&self) -> Pixels {
        match self {
            Self::Name => px(260.0),
            Self::DateModified => px(140.0),
            Self::Size => px(80.0),
            Self::Kind => px(100.0),
            Self::Permissions => px(120.0),
            Self::Owner => px(110.0),
            Self::Attributes => px(80.0),
        }
    }

    /// True if the column content should be right-aligned.
    pub fn right_aligned(&self) -> bool {
        matches!(self, Self::Size)
    }

    pub fn sortable(&self) -> bool {
        matches!(
            self,
            Self::Name | Self::DateModified | Self::Size | Self::Kind
        )
    }

    /// All available column variants (Name excluded — always shown).
    #[allow(dead_code)]
    pub const TOGGLEABLE: &'static [FileColumn] = &[
        Self::DateModified,
        Self::Size,
        Self::Kind,
        Self::Permissions,
        Self::Owner,
        Self::Attributes,
    ];
}

/// Default visible columns when no user preference is set.
pub fn default_columns() -> Vec<FileColumn> {
    vec![
        FileColumn::Name,
        FileColumn::Permissions,
        FileColumn::DateModified,
        FileColumn::Size,
        FileColumn::Kind,
    ]
}

/// Convert persisted string names back to FileColumn values.
pub fn columns_from_names(names: &[String]) -> Vec<FileColumn> {
    let mut cols: Vec<FileColumn> = names
        .iter()
        .filter_map(|n| serde_json::from_value(serde_json::Value::String(n.clone())).ok())
        .collect();
    if cols.is_empty() {
        cols = default_columns();
    }
    // Name is always first and required
    if cols.first() != Some(&FileColumn::Name) {
        cols.retain(|c| *c != FileColumn::Name);
        cols.insert(0, FileColumn::Name);
    }
    cols
}

/// Serialize column list to string names for persistence.
pub fn columns_to_names(cols: &[FileColumn]) -> Vec<String> {
    cols.iter()
        .filter_map(|c| {
            serde_json::to_value(c)
                .ok()
                .and_then(|v| v.as_str().map(String::from))
        })
        .collect()
}
