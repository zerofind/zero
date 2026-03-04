//! Version checking and rate limiting

use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use super::{CURRENT_VERSION, DL_BASE, UpdateError};

/// Result of a version check
#[derive(Debug, Clone)]
pub enum UpdateStatus {
    UpToDate,
    Available { version: String },
}

/// Check the latest version from the CDN
pub fn check_latest() -> Result<UpdateStatus, UpdateError> {
    let url = format!("{}/latest.txt", DL_BASE);

    let body = ureq::get(&url)
        .call()
        .map_err(|e| UpdateError::Network(e.to_string()))?
        .body_mut()
        .read_to_string()
        .map_err(|e| UpdateError::Network(e.to_string()))?;

    let latest = body.trim();

    if latest.is_empty() {
        return Err(UpdateError::Parse("empty version string".into()));
    }

    // Simple string comparison — versions are semver-shaped (e.g. "0.4.5")
    if latest == CURRENT_VERSION {
        Ok(UpdateStatus::UpToDate)
    } else {
        // Compare as semver tuples to avoid lexicographic pitfalls
        match (parse_semver(latest), parse_semver(CURRENT_VERSION)) {
            (Some(remote), Some(local)) if remote > local => Ok(UpdateStatus::Available {
                version: latest.to_string(),
            }),
            (Some(_), Some(_)) => Ok(UpdateStatus::UpToDate),
            _ => {
                // Fallback: if parsing fails, just compare strings
                if latest != CURRENT_VERSION {
                    Ok(UpdateStatus::Available {
                        version: latest.to_string(),
                    })
                } else {
                    Ok(UpdateStatus::UpToDate)
                }
            }
        }
    }
}

/// Parse a version string like "0.4.5" into (major, minor, patch)
fn parse_semver(v: &str) -> Option<(u32, u32, u32)> {
    let parts: Vec<&str> = v.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    Some((
        parts[0].parse().ok()?,
        parts[1].parse().ok()?,
        parts[2].parse().ok()?,
    ))
}

/// Check if enough time has passed since the last update check (24h)
pub fn should_check() -> bool {
    let Some(path) = last_check_path() else {
        return true;
    };

    let Ok(contents) = fs::read_to_string(&path) else {
        return true;
    };

    let Ok(last_ts) = contents.trim().parse::<u64>() else {
        return true;
    };

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // 24 hours = 86400 seconds
    now.saturating_sub(last_ts) > 86400
}

/// Record the current time as the last update check
pub fn record_check() {
    let Some(path) = last_check_path() else {
        return;
    };

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let _ = fs::write(&path, now.to_string());
}

/// Read the auto_update setting from ~/.zero/settings.json
///
/// Defaults to true if file/field is missing.
pub fn read_auto_update_setting() -> bool {
    let Some(path) = crate::dirs::settings_path() else {
        return true;
    };

    let Ok(contents) = fs::read_to_string(&path) else {
        return true;
    };

    let Ok(value) = serde_json::from_str::<serde_json::Value>(&contents) else {
        return true;
    };

    value
        .get("auto_update")
        .and_then(|v| v.as_bool())
        .unwrap_or(true)
}

/// Path to the last update check timestamp file
fn last_check_path() -> Option<std::path::PathBuf> {
    crate::dirs::data_dir().map(|d| d.join(".last_update_check"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_semver() {
        assert_eq!(parse_semver("0.4.5"), Some((0, 4, 5)));
        assert_eq!(parse_semver("1.0.0"), Some((1, 0, 0)));
        assert_eq!(parse_semver("bad"), None);
        assert_eq!(parse_semver("1.2"), None);
    }

    #[test]
    fn test_parse_semver_comparison() {
        let v1 = parse_semver("0.5.0").unwrap();
        let v2 = parse_semver("0.4.4").unwrap();
        assert!(v1 > v2);
    }
}
