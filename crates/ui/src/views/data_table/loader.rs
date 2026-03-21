use std::path::Path;

use super::types::delimiter_for_path;

/// Maximum number of rows to load into memory.
const MAX_ROWS: usize = 100_000;

/// Maximum file size we'll attempt to parse (500 MB).
const MAX_FILE_SIZE: u64 = 500 * 1024 * 1024;

/// A parsed data frame ready for display.
pub struct DataFrame {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub total_rows: usize,
    pub truncated: bool,
}

/// Load a delimited file (CSV/TSV) into a DataFrame.
pub fn load_delimited(path: &Path) -> Result<DataFrame, String> {
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if meta.len() > MAX_FILE_SIZE {
        return Err(format!(
            "File too large ({:.0} MB). Maximum is 500 MB.",
            meta.len() as f64 / (1024.0 * 1024.0),
        ));
    }

    let delimiter = delimiter_for_path(path);

    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .flexible(true)
        .has_headers(true)
        .from_path(path)
        .map_err(|e| e.to_string())?;

    let headers: Vec<String> = reader
        .headers()
        .map_err(|e| e.to_string())?
        .iter()
        .map(|h| h.to_string())
        .collect();

    if headers.is_empty() {
        return Err("File has no columns.".to_string());
    }

    let mut rows = Vec::new();
    let mut total_rows = 0usize;
    let col_count = headers.len();

    for result in reader.records() {
        let record = result.map_err(|e| e.to_string())?;
        total_rows += 1;

        if rows.len() < MAX_ROWS {
            let mut row: Vec<String> = record.iter().map(|f| f.to_string()).collect();
            // Pad short rows
            row.resize(col_count, String::new());
            rows.push(row);
        }
    }

    let truncated = total_rows > MAX_ROWS;

    Ok(DataFrame {
        headers,
        rows,
        total_rows,
        truncated,
    })
}
