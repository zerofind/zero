//! Platform-specific storage access for disk wiping
//!
//! This module provides low-level disk access that bypasses OS caches
//! to ensure data is actually written to the physical media.

#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "linux")]
pub use linux::*;

// Common traits and types used by platform implementations

use std::io::Result;
use std::path::Path;

/// Low-level storage access for direct I/O operations
pub trait StorageAccess: Send {
    /// Seek to a specific position
    fn seek(&mut self, position: u64) -> Result<u64>;

    /// Read data into buffer
    fn read(&mut self, buffer: &mut [u8]) -> Result<usize>;

    /// Write data to device
    fn write(&mut self, data: &[u8]) -> Result<()>;

    /// Flush any buffered data to device
    fn flush(&mut self) -> Result<()>;
}

/// Open a device for direct I/O access
///
/// This bypasses the OS cache to ensure writes go directly to the device.
///
/// # Arguments
///
/// * `path` - Path to the device (e.g., "/dev/rdisk4" on macOS)
///
/// # Platform behavior
///
/// - **macOS**: Uses `F_NOCACHE` to disable caching, uses raw device (`/dev/rdisk*`)
/// - **Linux**: Uses `O_DIRECT` for direct I/O
pub fn open_device<P: AsRef<Path>>(path: P) -> Result<Box<dyn StorageAccess>> {
    #[cfg(target_os = "macos")]
    {
        macos::open_device_direct(path)
    }

    #[cfg(target_os = "linux")]
    {
        linux::open_device_direct(path)
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = path;
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Direct disk access not supported on this platform",
        ))
    }
}

/// Unmount a volume before erasing
///
/// # Arguments
///
/// * `mount_point` - Path where the volume is mounted
///
/// # Returns
///
/// Ok(()) if unmount succeeded or volume was not mounted
pub fn unmount_volume<P: AsRef<Path>>(mount_point: P) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        macos::unmount(mount_point.as_ref())
    }

    #[cfg(target_os = "linux")]
    {
        linux::unmount(mount_point.as_ref())
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = mount_point;
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Unmount not supported on this platform",
        ))
    }
}
