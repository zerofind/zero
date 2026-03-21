//! Version checking and rate limiting

use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use super::{CURRENT_VERSION, GITHUB_REPO, UpdateError};

/// Result of a version check
#[derive(Debug, Clone)]
pub enum UpdateStatus {
    UpToDate,
    Available { version: String },
}

/// Check the latest version from GitHub Releases
pub fn check_latest() -> Result<UpdateStatus, UpdateError> {
    let url = format!("https://api.github.com/repos/{GITHUB_REPO}/releases/latest");

    let body = ureq::get(&url)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "zero-updater")
        .call()
        .map_err(|e| UpdateError::Network(e.to_string()))?
        .body_mut()
        .read_to_string()
        .map_err(|e| UpdateError::Network(e.to_string()))?;

    let json: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| UpdateError::Parse(e.to_string()))?;

    let tag = json["tag_name"]
        .as_str()
        .ok_or_else(|| UpdateError::Parse("missing tag_name in response".into()))?;

    // Strip leading 'v' from tag (e.g. "v0.7.0" → "0.7.0")
    let latest = tag.strip_prefix('v').unwrap_or(tag);

    if latest.is_empty() {
        return Err(UpdateError::Parse("empty version string".into()));
    }

    // Compare as semver tuples to avoid lexicographic pitfalls
    match (parse_semver(latest), parse_semver(CURRENT_VERSION)) {
        (Some(remote), Some(local)) if remote > local => Ok(UpdateStatus::Available {
            version: latest.to_string(),
        }),
        (Some(_), Some(_)) => Ok(UpdateStatus::UpToDate),
        _ => {
            if latest == CURRENT_VERSION {
                Ok(UpdateStatus::UpToDate)
            } else {
                Ok(UpdateStatus::Available {
                    version: latest.to_string(),
                })
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

/// Read the `auto_update` setting from ~/.zero/settings.json
///
/// Defaults to true if file/field is missing.
pub fn read_auto_update_setting() -> bool {
    let Some(path) = foundation::dirs::settings_path() else {
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
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true)
}

/// Path to the last update check timestamp file
fn last_check_path() -> Option<std::path::PathBuf> {
    foundation::dirs::data_dir().map(|d| d.join(".last_update_check"))
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
