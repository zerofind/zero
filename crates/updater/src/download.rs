//! Download, verify, and extract update artifacts

use std::fs;
use std::io::Read;
use std::path::PathBuf;

use sha2::{Digest, Sha512};

use super::{DL_BASE, UpdateError, platform_target};

/// Download, verify, and extract an update for the given version.
///
/// Returns the path to the extracted binary.
pub fn download_update(version: &str) -> Result<PathBuf, UpdateError> {
    let (os, arch) = platform_target();
    let artifact = format!("zero-{version}-{os}-{arch}");
    let tgz_url = format!("{DL_BASE}/v{version}/{artifact}.tgz");
    let sha_url = format!("{DL_BASE}/v{version}/{artifact}.tgz.sha512");

    let tmp_dir = std::env::temp_dir().join("zero-update");
    fs::create_dir_all(&tmp_dir).map_err(|e| UpdateError::Extract(e.to_string()))?;

    let tgz_path = tmp_dir.join(format!("{artifact}.tgz"));
    let bin_path = tmp_dir.join("zero");

    // Download checksum first (small, fails fast if version doesn't exist)
    let expected_sha = download_string(&sha_url)?;
    let expected_sha = expected_sha
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_string();

    if expected_sha.is_empty() {
        return Err(UpdateError::Parse("empty checksum".into()));
    }

    // Download tarball
    download_file(&tgz_url, &tgz_path)?;

    // Verify SHA512
    let actual_sha = sha512_file(&tgz_path)?;
    if actual_sha != expected_sha {
        // Clean up
        let _ = fs::remove_file(&tgz_path);
        return Err(UpdateError::ChecksumMismatch {
            expected: expected_sha,
            actual: actual_sha,
        });
    }

    // Extract binary from tarball
    extract_binary(&tgz_path, &bin_path)?;

    // Clean up tarball
    let _ = fs::remove_file(&tgz_path);

    Ok(bin_path)
}

/// Download a URL to a string
fn download_string(url: &str) -> Result<String, UpdateError> {
    ureq::get(url)
        .call()
        .map_err(|e| UpdateError::Network(format!("{url}: {e}")))?
        .body_mut()
        .read_to_string()
        .map_err(|e| UpdateError::Network(e.to_string()))
}

/// Download a URL to a file
fn download_file(url: &str, dest: &PathBuf) -> Result<(), UpdateError> {
    let mut response = ureq::get(url)
        .call()
        .map_err(|e| UpdateError::Network(format!("{url}: {e}")))?;

    let mut body = response.body_mut().as_reader();
    let mut file = fs::File::create(dest).map_err(|e| UpdateError::Extract(e.to_string()))?;

    std::io::copy(&mut body, &mut file).map_err(|e| UpdateError::Network(e.to_string()))?;

    Ok(())
}

/// Compute SHA512 hash of a file
fn sha512_file(path: &PathBuf) -> Result<String, UpdateError> {
    let mut file = fs::File::open(path).map_err(|e| UpdateError::Extract(e.to_string()))?;
    let mut hasher = Sha512::new();
    let mut buf = [0u8; 8192];

    loop {
        let n = file
            .read(&mut buf)
            .map_err(|e| UpdateError::Extract(e.to_string()))?;
        if n == 0 {
            break;
        }
        hasher.update(buf.get(..n).unwrap_or(&[]));
    }

    Ok(hex::encode(hasher.finalize()))
}

/// Extract the `zero` binary from a .tgz archive
fn extract_binary(tgz_path: &PathBuf, dest: &PathBuf) -> Result<(), UpdateError> {
    let file = fs::File::open(tgz_path).map_err(|e| UpdateError::Extract(e.to_string()))?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(gz);

    for entry in archive
        .entries()
        .map_err(|e| UpdateError::Extract(e.to_string()))?
    {
        let mut entry = entry.map_err(|e| UpdateError::Extract(e.to_string()))?;
        let path = entry
            .path()
            .map_err(|e| UpdateError::Extract(e.to_string()))?;

        // Look for the "zero" binary (may be at root or in a subdirectory)
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        if file_name == "zero" {
            entry
                .unpack(dest)
                .map_err(|e| UpdateError::Extract(e.to_string()))?;
            return Ok(());
        }
    }

    Err(UpdateError::Extract(
        "binary 'zero' not found in archive".into(),
    ))
}
