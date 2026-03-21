use std::io::{BufRead, BufReader};
use std::path::Path;

/// Extensions that open in the data table viewer.
const DATA_TABLE_EXTENSIONS: &[&str] = &[
    "csv",
    "tsv",
    // Binary columnar formats (future)
    // "parquet", "arrow", "ipc", "feather",
];

/// Candidate delimiters to test when sniffing CSV content.
const CANDIDATES: &[u8] = b",;\t|";

/// Number of lines to sample when sniffing the delimiter.
const SNIFF_LINES: usize = 20;

/// Check whether a file path should open in the data table viewer.
pub fn is_data_table(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| DATA_TABLE_EXTENSIONS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

/// Detect the delimiter for a delimited file.
///
/// `.tsv` files always use tab. For everything else, sniff the first few lines
/// and pick the candidate delimiter that produces the most consistent column
/// count (with more than one column). Falls back to comma.
pub fn delimiter_for_path(path: &Path) -> u8 {
    if path.extension().and_then(|e| e.to_str()) == Some("tsv") {
        return b'\t';
    }

    sniff_delimiter(path).unwrap_or(b',')
}

/// Read the first [`SNIFF_LINES`] lines and score each candidate delimiter.
///
/// For each candidate we count how many fields each line produces. The best
/// delimiter is the one where (a) lines consistently have the same field count
/// and (b) that count is > 1. If no candidate qualifies, returns `None`.
fn sniff_delimiter(path: &Path) -> Option<u8> {
    let file = std::fs::File::open(path).ok()?;
    let reader = BufReader::new(file);

    let lines: Vec<String> = reader
        .lines()
        .take(SNIFF_LINES)
        .filter_map(|l| l.ok())
        .filter(|l| !l.is_empty())
        .collect();

    if lines.is_empty() {
        return None;
    }

    let mut best: Option<(u8, usize)> = None; // (delimiter, consistent_col_count)

    for &delim in CANDIDATES {
        let counts: Vec<usize> = lines
            .iter()
            .map(|line| count_fields(line.as_bytes(), delim))
            .collect();

        // All lines must agree on the column count.
        let first = counts[0];
        if first <= 1 {
            continue;
        }
        if counts.iter().all(|&c| c == first) {
            // Prefer the delimiter that yields more columns (more structure).
            if best.is_none_or(|(_, prev)| first > prev) {
                best = Some((delim, first));
            }
        }
    }

    best.map(|(d, _)| d)
}

/// Count fields in a line for a given delimiter, respecting double-quote escaping.
fn count_fields(line: &[u8], delim: u8) -> usize {
    let mut fields = 1usize;
    let mut in_quotes = false;
    let mut i = 0;
    while i < line.len() {
        let b = line[i];
        if b == b'"' {
            if in_quotes && i + 1 < line.len() && line[i + 1] == b'"' {
                i += 1; // escaped quote
            } else {
                in_quotes = !in_quotes;
            }
        } else if b == delim && !in_quotes {
            fields += 1;
        }
        i += 1;
    }
    fields
}
