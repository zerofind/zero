//! Disk information module - hardware details and volume information
//!
//! This module provides detailed information about storage devices,
//! including USB hardware specs (speed, vendor, serial) and volume
//! capacity/usage. Useful for:
//! - Understanding transfer speed limitations
//! - Identifying drives for auto-sync rules
//! - Support/debugging information
//! - Secure disk erasure

mod hardware;
mod volume;

pub mod erase;

pub use hardware::{DiskHardware, UsbDeviceInfo, UsbSpeed};
pub use volume::{PhysicalDrive, VolumeInfo};

use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DiskError {
    #[error("Volume not found: {0}")]
    VolumeNotFound(String),

    #[error("Failed to get volume info: {0}")]
    VolumeInfoError(String),

    #[error("Failed to get hardware info: {0}")]
    HardwareInfoError(String),

    #[error("Not a USB device")]
    NotUsbDevice,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Complete disk information combining volume and hardware details
#[derive(Debug, Clone)]
pub struct DiskInfo {
    /// Volume information (name, capacity, mount point, etc.)
    pub volume: VolumeInfo,

    /// USB hardware details (if this is a USB device)
    pub usb: Option<UsbDeviceInfo>,
}

impl DiskInfo {
    /// Get disk information for a given path (mount point or any path on the volume)
    pub fn for_path(path: &Path) -> Result<Self, DiskError> {
        let volume = VolumeInfo::for_path(path)?;

        // Only try to get USB info if this is a USB device
        let usb = if volume.physical_drive.protocol == "USB" {
            UsbDeviceInfo::for_volume(&volume).ok()
        } else {
            None
        };

        Ok(Self { volume, usb })
    }

    /// Get a formatted summary string suitable for display
    pub fn summary(&self) -> String {
        let mut lines = Vec::new();

        // Volume name and device
        lines.push(format!(
            "{} ({})",
            self.volume.name, self.volume.physical_drive.device_name
        ));

        // USB details if available
        if let Some(ref usb) = self.usb {
            lines.push(format!("├─ Vendor: {}", usb.display_vendor()));
            lines.push(format!("├─ Product: {}", usb.product_name));
            lines.push(format!(
                "├─ Protocol: {} ({})",
                usb.speed.name(),
                usb.speed.max_throughput()
            ));
            lines.push(format!("├─ Serial: {}", usb.serial_number));
        } else {
            lines.push(format!(
                "├─ Protocol: {}",
                self.volume.physical_drive.protocol
            ));
            if let Some(ref medium) = self.volume.physical_drive.medium_type {
                lines.push(format!("├─ Type: {}", medium.to_uppercase()));
            }
        }

        // Capacity info
        let total_gb = self.volume.size_bytes as f64 / 1_000_000_000.0;
        let free_gb = self.volume.free_bytes as f64 / 1_000_000_000.0;
        let used_pct = (self.volume.size_bytes - self.volume.free_bytes) as f64
            / self.volume.size_bytes as f64
            * 100.0;

        lines.push(format!(
            "├─ Capacity: {total_gb:.1} GB ({free_gb:.1} GB free, {used_pct:.0}% used)"
        ));
        lines.push(format!("├─ File System: {}", self.volume.file_system));
        lines.push(format!("├─ Mount Point: {}", self.volume.mount_point));
        lines.push(format!("└─ BSD Device: /dev/{}", self.volume.bsd_name));

        lines.join("\n")
    }
}
