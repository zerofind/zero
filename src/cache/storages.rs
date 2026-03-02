//! Storage types for the split database architecture
//!
//! Storages represent indexed locations (USB drives, internal disks, network paths).
//! Each storage has its own separate etch store for file hashes.
//!
//! Identification priority:
//! 1. `device_serial` - USB serial number (most stable, survives renames)
//! 2. `device_uuid` - Disk UUID for internal drives
//! 3. `path_hash` - SHA256 of path for network mounts (fallback)
//!
//! The actual persistence is handled by ControlState (etch-backed).

use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Type of mount/storage
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MountType {
    /// USB external drive
    Usb,
    /// Internal disk (SSD, HDD)
    Internal,
    /// Network mount (NFS, SMB, AFP)
    Network,
    /// Regular folder (not a mount point)
    Folder,
}

impl MountType {
    pub fn as_str(&self) -> &'static str {
        match self {
            MountType::Usb => "usb",
            MountType::Internal => "internal",
            MountType::Network => "network",
            MountType::Folder => "folder",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "usb" => Some(MountType::Usb),
            "internal" => Some(MountType::Internal),
            "network" => Some(MountType::Network),
            "folder" => Some(MountType::Folder),
            _ => None,
        }
    }
}

/// Identifier for a storage (in priority order)
#[derive(Debug, Clone)]
pub enum StorageIdentifier {
    /// USB device serial number (most stable)
    DeviceSerial(String),
    /// Disk UUID (for internal drives)
    DeviceUuid(String),
    /// SHA256 hash of path (fallback for network/folders)
    PathHash(String),
}

impl StorageIdentifier {
    /// Create identifier from a path (fallback method)
    pub fn from_path(path: &Path) -> Self {
        let path_str = path.to_string_lossy();
        let mut hasher = Sha256::new();
        hasher.update(path_str.as_bytes());
        let hash = hasher.finalize();
        let hash_hex = hex::encode(&hash[..16]); // Use first 16 bytes (32 hex chars)
        StorageIdentifier::PathHash(hash_hex)
    }

    /// Get the database filename for this identifier
    pub fn db_filename(&self) -> String {
        match self {
            StorageIdentifier::DeviceSerial(s) => format!("serial_{}", sanitize_filename(s)),
            StorageIdentifier::DeviceUuid(u) => format!("uuid_{}", sanitize_filename(u)),
            StorageIdentifier::PathHash(h) => format!("path_{}", h),
        }
    }

    /// Get the raw identifier value
    pub fn value(&self) -> &str {
        match self {
            StorageIdentifier::DeviceSerial(s) => s,
            StorageIdentifier::DeviceUuid(u) => u,
            StorageIdentifier::PathHash(h) => h,
        }
    }
}

/// Sanitize a string for use in a filename
fn sanitize_filename(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Device information for creating a storage
#[derive(Debug, Clone, Default)]
pub struct DeviceInfo {
    /// USB serial number
    pub serial: Option<String>,
    /// Disk UUID
    pub uuid: Option<String>,
    /// Mount type
    pub mount_type: Option<MountType>,
    /// User-friendly display name
    pub display_name: Option<String>,
}

impl DeviceInfo {
    /// Create DeviceInfo for a USB device
    pub fn usb(serial: impl Into<String>, label: Option<String>) -> Self {
        Self {
            serial: Some(serial.into()),
            mount_type: Some(MountType::Usb),
            display_name: label,
            ..Default::default()
        }
    }

    /// Create DeviceInfo for an internal drive
    pub fn internal(uuid: impl Into<String>, label: Option<String>) -> Self {
        Self {
            uuid: Some(uuid.into()),
            mount_type: Some(MountType::Internal),
            display_name: label,
            ..Default::default()
        }
    }

    /// Create DeviceInfo for a network mount
    pub fn network(label: Option<String>) -> Self {
        Self {
            mount_type: Some(MountType::Network),
            display_name: label,
            ..Default::default()
        }
    }

    /// Create DeviceInfo for a regular folder
    pub fn folder(label: Option<String>) -> Self {
        Self {
            mount_type: Some(MountType::Folder),
            display_name: label,
            ..Default::default()
        }
    }

    /// Get the best identifier from this device info, with path as fallback
    pub fn to_identifier(&self, path: &Path) -> StorageIdentifier {
        if let Some(serial) = &self.serial {
            StorageIdentifier::DeviceSerial(serial.clone())
        } else if let Some(uuid) = &self.uuid {
            StorageIdentifier::DeviceUuid(uuid.clone())
        } else {
            StorageIdentifier::from_path(path)
        }
    }
}

/// A storage entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Storage {
    /// Unique identifier
    pub id: i64,
    /// USB serial number (if available)
    pub device_serial: Option<String>,
    /// Disk UUID (if available)
    pub device_uuid: Option<String>,
    /// Path hash (fallback identifier)
    pub path_hash: Option<String>,
    /// User-friendly display name
    pub display_name: Option<String>,
    /// Where this storage was last mounted
    pub last_mount_path: Option<String>,
    /// Type of mount
    pub mount_type: Option<MountType>,
    /// Filename of the storage database (e.g., "serial_ABC123")
    pub db_filename: String,
    /// Number of indexed files (cached)
    pub file_count: i64,
    /// Total bytes of indexed files (cached)
    pub total_bytes: i64,
    /// When this storage was last seen
    pub last_seen_at: Option<i64>,
    /// When this storage was created
    pub created_at: i64,
}

impl Storage {
    /// Get the best identifier for this storage
    pub fn identifier(&self) -> Option<StorageIdentifier> {
        self.device_serial
            .as_ref()
            .map(|s| StorageIdentifier::DeviceSerial(s.clone()))
            .or_else(|| {
                self.device_uuid
                    .as_ref()
                    .map(|u| StorageIdentifier::DeviceUuid(u.clone()))
            })
            .or_else(|| {
                self.path_hash
                    .as_ref()
                    .map(|h| StorageIdentifier::PathHash(h.clone()))
            })
    }
}
