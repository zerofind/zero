/// Format a number with comma separators (e.g. 12450 → "12,450").
pub fn format_number(n: u64) -> String {
    if n < 1_000 {
        return n.to_string();
    }
    let s = n.to_string();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            result.push(',');
        }
        result.push(c);
    }
    result
}

/// Format bytes for human-readable display (e.g. 1536 → "2 KB").
pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.0} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

/// Format bytes for file sizes, showing "--" for zero (directories).
pub fn format_size(bytes: u64) -> String {
    if bytes == 0 {
        "--".to_string()
    } else {
        format_bytes(bytes)
    }
}

/// Format a unix timestamp as a relative time string.
pub fn format_date(mtime: u64) -> String {
    if mtime == 0 {
        return "--".to_string();
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let diff = now.saturating_sub(mtime);

    if diff < 60 {
        "Just now".to_string()
    } else if diff < 3600 {
        format!("{} min ago", diff / 60)
    } else if diff < 86400 {
        format!("{} hr ago", diff / 3600)
    } else if diff < 86400 * 30 {
        let days = diff / 86400;
        if days == 1 {
            "Yesterday".to_string()
        } else {
            format!("{days} days ago")
        }
    } else if diff < 86400 * 365 {
        format!("{} months ago", diff / (86400 * 30))
    } else {
        format!("{} years ago", diff / (86400 * 365))
    }
}
