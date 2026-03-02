//! USB hardware information from IOKit/ioreg
//!
//! This module provides detailed USB device information including:
//! - Connection speed (USB 2.0, 3.0, 3.1, 3.2)
//! - Vendor and product identification
//! - Serial number (useful for identifying specific drives)
//!
//! This information is extracted from macOS IOKit registry using ioreg.

use std::process::Command;

use super::{DiskError, VolumeInfo};

/// USB connection speed
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsbSpeed {
    /// USB 1.0 Low Speed - 1.5 Mbps
    LowSpeed,
    /// USB 1.1 Full Speed - 12 Mbps
    FullSpeed,
    /// USB 2.0 High Speed - 480 Mbps
    HighSpeed,
    /// USB 3.0/3.1 Gen 1/3.2 Gen 1 SuperSpeed - 5 Gbps
    SuperSpeed,
    /// USB 3.1 Gen 2/3.2 Gen 2 SuperSpeed+ - 10 Gbps
    SuperSpeedPlus,
    /// USB 3.2 Gen 2x2 - 20 Gbps
    SuperSpeed20Gbps,
    /// USB4 - 40 Gbps
    Usb4,
    /// Unknown speed
    Unknown(u32),
}

impl UsbSpeed {
    /// Create from IOKit device speed value
    pub fn from_device_speed(speed: u32) -> Self {
        match speed {
            0 => UsbSpeed::LowSpeed,
            1 => UsbSpeed::FullSpeed,
            2 => UsbSpeed::HighSpeed,
            3 => UsbSpeed::SuperSpeed,
            4 => UsbSpeed::SuperSpeedPlus,
            5 => UsbSpeed::SuperSpeed20Gbps,
            6 => UsbSpeed::Usb4,
            n => UsbSpeed::Unknown(n),
        }
    }

    /// Get the USB speed name
    pub fn name(&self) -> &'static str {
        match self {
            UsbSpeed::LowSpeed => "USB 1.0 Low Speed",
            UsbSpeed::FullSpeed => "USB 1.1 Full Speed",
            UsbSpeed::HighSpeed => "USB 2.0 High Speed",
            UsbSpeed::SuperSpeed => "USB 3.0 SuperSpeed",
            UsbSpeed::SuperSpeedPlus => "USB 3.1 Gen 2 SuperSpeed+",
            UsbSpeed::SuperSpeed20Gbps => "USB 3.2 Gen 2x2",
            UsbSpeed::Usb4 => "USB4",
            UsbSpeed::Unknown(_) => "Unknown",
        }
    }

    /// Get the theoretical maximum throughput
    pub fn max_throughput(&self) -> &'static str {
        match self {
            UsbSpeed::LowSpeed => "1.5 Mbps",
            UsbSpeed::FullSpeed => "12 Mbps",
            UsbSpeed::HighSpeed => "480 Mbps (~60 MB/s)",
            UsbSpeed::SuperSpeed => "5 Gbps (~625 MB/s)",
            UsbSpeed::SuperSpeedPlus => "10 Gbps (~1.25 GB/s)",
            UsbSpeed::SuperSpeed20Gbps => "20 Gbps (~2.5 GB/s)",
            UsbSpeed::Usb4 => "40 Gbps (~5 GB/s)",
            UsbSpeed::Unknown(_) => "Unknown",
        }
    }

    /// Get realistic throughput estimate for flash drives
    /// (accounting for protocol overhead and typical flash performance)
    pub fn realistic_throughput(&self) -> &'static str {
        match self {
            UsbSpeed::LowSpeed => "~0.1 MB/s",
            UsbSpeed::FullSpeed => "~1 MB/s",
            UsbSpeed::HighSpeed => "20-40 MB/s",
            UsbSpeed::SuperSpeed => "100-400 MB/s",
            UsbSpeed::SuperSpeedPlus => "400-900 MB/s",
            UsbSpeed::SuperSpeed20Gbps => "800-1500 MB/s",
            UsbSpeed::Usb4 => "1500-3000 MB/s",
            UsbSpeed::Unknown(_) => "Unknown",
        }
    }

    /// Get theoretical max bytes per second
    pub fn max_bytes_per_second(&self) -> u64 {
        match self {
            UsbSpeed::LowSpeed => 187_500,               // 1.5 Mbps
            UsbSpeed::FullSpeed => 1_500_000,            // 12 Mbps
            UsbSpeed::HighSpeed => 60_000_000,           // 480 Mbps
            UsbSpeed::SuperSpeed => 625_000_000,         // 5 Gbps
            UsbSpeed::SuperSpeedPlus => 1_250_000_000,   // 10 Gbps
            UsbSpeed::SuperSpeed20Gbps => 2_500_000_000, // 20 Gbps
            UsbSpeed::Usb4 => 5_000_000_000,             // 40 Gbps
            UsbSpeed::Unknown(_) => 0,
        }
    }
}

/// USB device information
#[derive(Debug, Clone)]
pub struct UsbDeviceInfo {
    /// Product name (e.g., "SanDisk 3.2Gen1")
    pub product_name: String,

    /// Vendor name (e.g., "SanDisk", "Western Digital")
    pub vendor_name: String,

    /// USB Vendor ID (e.g., 1921 for SanDisk)
    pub vendor_id: u32,

    /// USB Product ID
    pub product_id: u32,

    /// Serial number (unique identifier for this specific device)
    pub serial_number: String,

    /// Connection speed
    pub speed: UsbSpeed,

    /// USB protocol version (e.g., "3.0", "2.0")
    pub usb_version: String,

    /// Location ID (unique port identifier)
    pub location_id: u32,

    /// Power allocation in mA
    pub power_ma: Option<u32>,
}

impl UsbDeviceInfo {
    /// Get USB device info for a mounted volume
    pub fn for_volume(volume: &VolumeInfo) -> Result<Self, DiskError> {
        if volume.physical_drive.protocol != "USB" {
            return Err(DiskError::NotUsbDevice);
        }

        // Extract device name from physical_drive info
        let device_name = &volume.physical_drive.device_name;

        // Get all USB devices from ioreg and find the matching one
        let devices = get_usb_devices()?;

        // Try to find by product name match
        devices
            .into_iter()
            .find(|d| {
                d.product_name.contains(device_name)
                    || device_name.contains(&d.product_name)
                    || d.product_name
                        .split_whitespace()
                        .any(|part| device_name.contains(part) && part.len() > 3)
            })
            .ok_or_else(|| {
                DiskError::HardwareInfoError(format!(
                    "Could not find USB device info for '{}'",
                    device_name
                ))
            })
    }

    /// Get all connected USB storage devices
    pub fn all_storage_devices() -> Result<Vec<Self>, DiskError> {
        get_usb_devices()
    }

    /// Get a display-friendly vendor name
    /// (resolves common vendor IDs to proper names)
    pub fn display_vendor(&self) -> &str {
        // Map known vendor IDs to proper names
        // The vendor_name from ioreg is often just "USB" or truncated
        match self.vendor_id {
            0x0781 => "SanDisk",
            0x0951 => "Kingston",
            0x058f => "Alcor Micro (Generic)",
            0x0930 => "Toshiba",
            0x1058 => "Western Digital",
            0x0bc2 => "Seagate",
            0x04e8 => "Samsung",
            0x13fe => "Phison (Generic)",
            0x8564 => "Transcend",
            0x05e3 => "Genesys Logic (Hub/Reader)",
            0x0bda => "Realtek",
            0x1b1c => "Corsair",
            0x154b => "PNY",
            0x0411 => "Buffalo",
            0x1f75 => "Innostor",
            0x125f => "ADATA",
            0x18a5 => "Verbatim",
            0x0cf2 => "ENE Technology",
            _ => {
                // Fall back to reported name, cleaning up common issues
                let name = self.vendor_name.trim();
                if name.is_empty() || name == "USB" {
                    "Unknown Vendor"
                } else {
                    &self.vendor_name
                }
            }
        }
    }
}

/// Disk hardware abstraction trait for cross-platform support
pub trait DiskHardware {
    /// Get USB device info if this is a USB device
    fn usb_info(&self) -> Option<&UsbDeviceInfo>;

    /// Check if this is an SSD
    fn is_ssd(&self) -> bool;

    /// Check if this is an external device
    fn is_external(&self) -> bool;

    /// Get the connection protocol (USB, SATA, NVMe, etc.)
    fn protocol(&self) -> &str;
}

// ---- Internal parsing ----

/// Get all USB storage devices from ioreg
fn get_usb_devices() -> Result<Vec<UsbDeviceInfo>, DiskError> {
    let output = Command::new("ioreg")
        .args(["-c", "IOUSBHostDevice", "-r", "-l"])
        .output()
        .map_err(|e| DiskError::HardwareInfoError(format!("Failed to run ioreg: {}", e)))?;

    if !output.status.success() {
        return Err(DiskError::HardwareInfoError(
            "ioreg returned non-zero exit code".to_string(),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_ioreg_output(&stdout)
}

/// Parse ioreg output to extract USB device info
fn parse_ioreg_output(output: &str) -> Result<Vec<UsbDeviceInfo>, DiskError> {
    let mut devices = Vec::new();
    let mut current_device: Option<UsbDeviceBuilder> = None;

    for line in output.lines() {
        let trimmed = line.trim();

        // Start of a new device block
        if trimmed.contains("IOUSBHostDevice") && trimmed.contains("<class") {
            // Save previous device if complete
            if let Some(builder) = current_device.take()
                && let Some(device) = builder.build()
            {
                devices.push(device);
            }
            current_device = Some(UsbDeviceBuilder::new());
        }

        // Parse properties within device block
        if let Some(ref mut builder) = current_device
            && let Some((key, value)) = parse_ioreg_property(trimmed)
        {
            match key {
                "USB Product Name" | "kUSBProductString" => {
                    if builder.product_name.is_none() {
                        builder.product_name = Some(value.trim().to_string());
                    }
                }
                "USB Vendor Name" | "kUSBVendorString" => {
                    if builder.vendor_name.is_none() {
                        builder.vendor_name = Some(value.trim().to_string());
                    }
                }
                "idVendor" => {
                    if let Ok(id) = value.parse::<u32>() {
                        builder.vendor_id = Some(id);
                    }
                }
                "idProduct" => {
                    if let Ok(id) = value.parse::<u32>() {
                        builder.product_id = Some(id);
                    }
                }
                "USB Serial Number" | "kUSBSerialNumberString" => {
                    if builder.serial_number.is_none() {
                        builder.serial_number = Some(value.to_string());
                    }
                }
                "Device Speed" => {
                    if let Ok(speed) = value.parse::<u32>() {
                        builder.speed = Some(speed);
                    }
                }
                "bcdUSB" => {
                    if let Ok(version) = value.parse::<u32>() {
                        builder.usb_version = Some(bcd_to_version(version));
                    }
                }
                "locationID" => {
                    if let Ok(loc) = value.parse::<u32>() {
                        builder.location_id = Some(loc);
                    }
                }
                "UsbPowerSinkAllocation" => {
                    if let Ok(power) = value.parse::<u32>() {
                        builder.power_ma = Some(power);
                    }
                }
                _ => {}
            }
        }
    }

    // Don't forget the last device
    if let Some(builder) = current_device
        && let Some(device) = builder.build()
    {
        devices.push(device);
    }

    // Filter to only storage devices (have product name suggesting storage)
    let storage_devices: Vec<_> = devices
        .into_iter()
        .filter(|d| {
            let name_lower = d.product_name.to_lowercase();
            // Include devices that look like storage
            name_lower.contains("disk")
                || name_lower.contains("storage")
                || name_lower.contains("flash")
                || name_lower.contains("sandisk")
                || name_lower.contains("kingston")
                || name_lower.contains("cruzer")
                || name_lower.contains("gen1")
                || name_lower.contains("gen2")
                || name_lower.contains("ssd")
                || name_lower.contains("hdd")
                // Exclude obvious non-storage devices
                || (!name_lower.contains("hub")
                    && !name_lower.contains("keyboard")
                    && !name_lower.contains("mouse")
                    && !name_lower.contains("camera")
                    && !name_lower.contains("audio")
                    && !name_lower.contains("bluetooth")
                    && d.serial_number.len() > 5) // Storage devices usually have longer serials
        })
        .collect();

    Ok(storage_devices)
}

/// Parse a single ioreg property line
fn parse_ioreg_property(line: &str) -> Option<(&str, &str)> {
    // Format: "key" = value or "key" = "value"
    let line = line.trim_start_matches(['|', ' ', '+', '-']);

    if !line.starts_with('"') {
        return None;
    }

    let parts: Vec<&str> = line.splitn(2, " = ").collect();
    if parts.len() != 2 {
        return None;
    }

    let key = parts[0].trim_matches('"');
    let value = parts[1].trim_matches('"').trim();

    Some((key, value))
}

/// Convert BCD USB version to string (e.g., 0x0200 -> "2.0", 0x0310 -> "3.1")
fn bcd_to_version(bcd: u32) -> String {
    let major = (bcd >> 8) & 0xFF;
    let minor = (bcd >> 4) & 0x0F;
    let patch = bcd & 0x0F;

    if patch > 0 {
        format!("{}.{}.{}", major, minor, patch)
    } else {
        format!("{}.{}", major, minor)
    }
}

/// Builder for UsbDeviceInfo during parsing
#[derive(Default)]
struct UsbDeviceBuilder {
    product_name: Option<String>,
    vendor_name: Option<String>,
    vendor_id: Option<u32>,
    product_id: Option<u32>,
    serial_number: Option<String>,
    speed: Option<u32>,
    usb_version: Option<String>,
    location_id: Option<u32>,
    power_ma: Option<u32>,
}

impl UsbDeviceBuilder {
    fn new() -> Self {
        Self::default()
    }

    fn build(self) -> Option<UsbDeviceInfo> {
        // Require at least product name and vendor ID
        let product_name = self.product_name?;
        let vendor_id = self.vendor_id?;

        Some(UsbDeviceInfo {
            product_name,
            vendor_name: self.vendor_name.unwrap_or_else(|| "Unknown".to_string()),
            vendor_id,
            product_id: self.product_id.unwrap_or(0),
            serial_number: self.serial_number.unwrap_or_else(|| "Unknown".to_string()),
            speed: UsbSpeed::from_device_speed(self.speed.unwrap_or(0)),
            usb_version: self.usb_version.unwrap_or_else(|| "Unknown".to_string()),
            location_id: self.location_id.unwrap_or(0),
            power_ma: self.power_ma,
        })
    }
}

#[cfg(test)]
#[path = "hardware_test.rs"]
mod hardware_test;
