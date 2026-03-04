use std::path::Path;

/// Extensions that open in the data table viewer.
const DATA_TABLE_EXTENSIONS: &[&str] = &[
    "csv",
    "tsv",
    // Binary columnar formats (future)
    // "parquet", "arrow", "ipc", "feather",
];

/// Check whether a file path should open in the data table viewer.
pub fn is_data_table(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| DATA_TABLE_EXTENSIONS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

/// Detect the delimiter from the file extension.
pub fn delimiter_for_path(path: &Path) -> u8 {
    match path.extension().and_then(|e| e.to_str()) {
        Some("tsv") => b'\t',
        _ => b',',
    }
}
