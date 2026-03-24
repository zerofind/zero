//! Volume information from `system_profiler` `SPStorageDataType`
//!
//! This module provides volume details like capacity, free space,
//! file system, and physical drive information.

use serde::Deserialize;
use std::path::Path;
use std::process::Command;

use super::DiskError;

/// Volume information for a mounted disk
#[derive(Debug, Clone)]
pub struct VolumeInfo {
    /// Volume name (e.g., "Ghost", "Macintosh HD")
    pub name: String,

    /// BSD device name (e.g., "disk22s1")
    pub bsd_name: String,

    /// File system type (e.g., "APFS", "Case-sensitive Journaled HFS+")
    pub file_system: String,

    /// Total size in bytes
    pub size_bytes: u64,

    /// Free space in bytes
    pub free_bytes: u64,

    /// Mount point path (e.g., "/Volumes/Ghost")
    pub mount_point: String,

    /// Volume UUID
    pub volume_uuid: String,

    /// Whether the volume is writable
    pub writable: bool,

    /// Whether ownership is ignored on this volume
    pub ignore_ownership: bool,

    /// Physical drive information
    pub physical_drive: PhysicalDrive,
}

/// Physical drive information
#[derive(Debug, Clone)]
pub struct PhysicalDrive {
    /// Device name (e.g., "`SanDisk` 3.2Gen1", "APPLE SSD AP0512Z")
    pub device_name: String,

    /// Media name (e.g., "USB `SanDisk` 3.2Gen1 Media")
    pub media_name: String,

    /// Protocol (e.g., "USB", "Apple Fabric", "Disk Image")
    pub protocol: String,

    /// Whether this is an internal disk
    pub is_internal: bool,

    /// Medium type (e.g., "ssd", "hdd") - may be None for USB
    pub medium_type: Option<String>,

    /// Partition map type
    pub partition_map_type: Option<String>,

    /// SMART status (if available)
    pub smart_status: Option<String>,
}

impl VolumeInfo {
    /// Get volume information for a path (mount point or any path on the volume)
    pub fn for_path(path: &Path) -> Result<Self, DiskError> {
        // Get the mount point for this path
        let mount_point = get_mount_point(path)?;

        // Get all volumes from system_profiler
        let volumes = get_all_volumes()?;

        // Find the volume matching this mount point
        volumes
            .into_iter()
            .find(|v| v.mount_point == mount_point)
            .ok_or(DiskError::VolumeNotFound(mount_point))
    }

    /// Get volume information by volume name
    pub fn by_name(name: &str) -> Result<Self, DiskError> {
        let volumes = get_all_volumes()?;

        volumes
            .into_iter()
            .find(|v| v.name == name)
            .ok_or_else(|| DiskError::VolumeNotFound(name.to_string()))
    }

    /// Get all mounted volumes
    pub fn all() -> Result<Vec<Self>, DiskError> {
        get_all_volumes()
    }

    /// Get used space in bytes
    pub fn used_bytes(&self) -> u64 {
        self.size_bytes.saturating_sub(self.free_bytes)
    }

    /// Get usage percentage (0.0 - 100.0)
    pub fn usage_percent(&self) -> f64 {
        if self.size_bytes == 0 {
            return 0.0;
        }
        self.used_bytes() as f64 / self.size_bytes as f64 * 100.0
    }

    /// Check if this is an external/removable volume
    pub fn is_external(&self) -> bool {
        !self.physical_drive.is_internal
    }

    /// Check if this is a USB device
    pub fn is_usb(&self) -> bool {
        self.physical_drive.protocol == "USB"
    }
}

// JSON structures for parsing system_profiler output
#[derive(Debug, Deserialize)]
struct SystemProfilerOutput {
    #[serde(rename = "SPStorageDataType")]
    storage_data: Vec<StorageEntry>,
}

#[derive(Debug, Deserialize)]
struct StorageEntry {
    #[serde(rename = "_name")]
    name: String,

    bsd_name: String,

    file_system: String,

    #[serde(rename = "size_in_bytes")]
    size_bytes: u64,

    #[serde(rename = "free_space_in_bytes")]
    free_bytes: u64,

    mount_point: String,

    volume_uuid: String,

    #[serde(default)]
    writable: YesNo,

    #[serde(default)]
    ignore_ownership: YesNo,

    physical_drive: PhysicalDriveEntry,
}

#[derive(Debug, Deserialize)]
struct PhysicalDriveEntry {
    device_name: String,

    #[serde(default)]
    media_name: Option<String>,

    protocol: String,

    #[serde(default)]
    is_internal_disk: YesNo,

    #[serde(default)]
    medium_type: Option<String>,

    #[serde(default)]
    partition_map_type: Option<String>,

    #[serde(default)]
    smart_status: Option<String>,
}

/// Helper for yes/no string fields
#[derive(Debug, Deserialize, Default)]
#[serde(from = "Option<String>")]
struct YesNo(bool);

impl From<Option<String>> for YesNo {
    fn from(s: Option<String>) -> Self {
        YesNo(s.is_some_and(|s| s == "yes"))
    }
}

/// Get the mount point for a given path
fn get_mount_point(path: &Path) -> Result<String, DiskError> {
    // Use df to get the mount point
    let output = Command::new("df")
        .arg(path)
        .output()
        .map_err(|e| DiskError::VolumeInfoError(format!("Failed to run df: {e}")))?;

    if !output.status.success() {
        return Err(DiskError::VolumeNotFound(path.display().to_string()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse df output - second line contains the info
    // Format: Filesystem 512-blocks Used Available Capacity iused ifree %iused Mounted on
    let lines: Vec<&str> = stdout.lines().collect();
    if lines.len() < 2 {
        return Err(DiskError::VolumeInfoError(
            "Unexpected df output format".to_string(),
        ));
    }

    // The mount point is the last field (may contain spaces)
    // We need to find it after "Mounted on" in header
    // SAFETY: bounds checked above (lines.len() >= 2)
    #[allow(clippy::indexing_slicing)]
    let data_line = lines[1];

    // Find mount point - it's everything after the percentage columns
    // Look for pattern like "100%" or "50%" followed by mount path
    if let Some(idx) = data_line.rfind('%') {
        let after_pct = &data_line[idx + 1..];
        let mount = after_pct.trim();
        if !mount.is_empty() {
            return Ok(mount.to_string());
        }
    }

    // Fallback: try to find /Volumes/ or / at end
    if let Some(idx) = data_line.find("/Volumes/") {
        return Ok(data_line[idx..].trim().to_string());
    }
    if data_line.ends_with(" /") {
        return Ok("/".to_string());
    }

    Err(DiskError::VolumeInfoError(format!(
        "Could not parse mount point from df output: {data_line}"
    )))
}

/// Get all volumes from `system_profiler`
fn get_all_volumes() -> Result<Vec<VolumeInfo>, DiskError> {
    let output = Command::new("system_profiler")
        .args(["SPStorageDataType", "-json"])
        .output()
        .map_err(|e| DiskError::VolumeInfoError(format!("Failed to run system_profiler: {e}")))?;

    if !output.status.success() {
        return Err(DiskError::VolumeInfoError(
            "system_profiler returned non-zero exit code".to_string(),
        ));
    }

    let parsed: SystemProfilerOutput = serde_json::from_slice(&output.stdout)?;

    let volumes = parsed
        .storage_data
        .into_iter()
        .map(|entry| VolumeInfo {
            name: entry.name,
            bsd_name: entry.bsd_name,
            file_system: entry.file_system,
            size_bytes: entry.size_bytes,
            free_bytes: entry.free_bytes,
            mount_point: entry.mount_point,
            volume_uuid: entry.volume_uuid,
            writable: entry.writable.0,
            ignore_ownership: entry.ignore_ownership.0,
            physical_drive: PhysicalDrive {
                device_name: entry.physical_drive.device_name,
                media_name: entry
                    .physical_drive
                    .media_name
                    .unwrap_or_else(|| "Unknown".to_string()),
                protocol: entry.physical_drive.protocol,
                is_internal: entry.physical_drive.is_internal_disk.0,
                medium_type: entry.physical_drive.medium_type,
                partition_map_type: entry.physical_drive.partition_map_type,
                smart_status: entry.physical_drive.smart_status,
            },
        })
        .collect();

    Ok(volumes)
}

#[cfg(test)]
#[path = "volume_test.rs"]
mod volume_test;
