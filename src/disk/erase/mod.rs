//! Secure disk erasure module
//!
//! Provides secure wiping of storage devices with multiple security levels.
//! Uses cryptographic random data and optional verification passes.
//!
//! # Safety Warning
//!
//! These operations are **destructive and irreversible**. Always verify
//! the target device before erasing.
//!
//! # Example
//!
//! ```no_run
//! use std::path::Path;
//! use zero::disk::{VolumeInfo, erase::{EraseLevel, EraseOptions, erase_volume}};
//!
//! let volume = VolumeInfo::for_path(Path::new("/Volumes/MyUSB")).unwrap();
//! let options = EraseOptions::new(EraseLevel::Normal);
//!
//! erase_volume(&volume, options, |progress| {
//!     println!("{}% complete", progress.percent());
//! }).unwrap();
//! ```

mod buffer;
pub mod platform;
mod schemes;
mod stage;
pub mod state;
pub mod wipe;

pub use schemes::{EraseLevel, Scheme};
pub use wipe::{EraseProgress, EraseResult, Verify, run_wipe_cancellable};

use crate::disk::{DiskError, VolumeInfo};
use std::path::Path;

/// Options for disk erasure
#[derive(Debug, Clone)]
pub struct EraseOptions {
    /// Security level (Quick, Normal, Strong)
    pub level: EraseLevel,

    /// Whether to verify writes by reading back
    pub verify: Verify,

    /// Block size for I/O operations (default: 1MB)
    pub block_size: usize,

    /// Number of retries for failed blocks
    pub retries: u32,

    /// Skip confirmation (for programmatic use)
    pub force: bool,
}

impl EraseOptions {
    /// Create new erase options with the given security level
    pub fn new(level: EraseLevel) -> Self {
        Self {
            level,
            verify: Verify::Last,    // Verify after final pass by default
            block_size: 1024 * 1024, // 1MB blocks
            retries: 8,
            force: false,
        }
    }

    /// Set verification mode
    pub fn with_verify(mut self, verify: Verify) -> Self {
        self.verify = verify;
        self
    }

    /// Set block size (must be power of 2, minimum 4096)
    pub fn with_block_size(mut self, block_size: usize) -> Self {
        // Ensure minimum block size and power of 2
        let block_size = block_size.max(4096);
        self.block_size = if block_size.is_power_of_two() {
            block_size
        } else {
            block_size.next_power_of_two()
        };
        self
    }

    /// Set retry count for failed blocks
    pub fn with_retries(mut self, retries: u32) -> Self {
        self.retries = retries;
        self
    }

    /// Skip confirmation prompts
    pub fn with_force(mut self, force: bool) -> Self {
        self.force = force;
        self
    }
}

impl Default for EraseOptions {
    fn default() -> Self {
        Self::new(EraseLevel::Normal)
    }
}

/// Errors specific to erase operations
#[derive(Debug, thiserror::Error)]
pub enum EraseError {
    #[error("Device not found: {0}")]
    DeviceNotFound(String),

    #[error("Device is not writable: {0}")]
    NotWritable(String),

    #[error("Device is internal/system disk - refusing to erase: {0}")]
    SystemDisk(String),

    #[error("Failed to unmount volume: {0}")]
    UnmountFailed(String),

    #[error("Failed to open device for writing: {0}")]
    OpenFailed(String),

    #[error("Write failed at position {position}: {message}")]
    WriteFailed { position: u64, message: String },

    #[error(
        "Verification failed at position {position}: expected {expected:02X}, got {actual:02X}"
    )]
    VerificationFailed {
        position: u64,
        expected: u8,
        actual: u8,
    },

    #[error("Operation cancelled by user")]
    Cancelled,

    #[error("Too many bad blocks ({count}) - device may be failing")]
    TooManyBadBlocks { count: u32 },

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Disk error: {0}")]
    Disk(#[from] DiskError),
}

/// Information about the device to be erased (for confirmation)
#[derive(Debug, Clone)]
pub struct EraseTarget {
    /// Device path (e.g., "/dev/rdisk4")
    pub device_path: String,

    /// Human-readable name
    pub name: String,

    /// Size in bytes
    pub size_bytes: u64,

    /// Whether this is an SSD/flash device
    pub is_flash: bool,

    /// Mount point (if mounted)
    pub mount_point: Option<String>,

    /// Warning messages for the user
    pub warnings: Vec<String>,
}

impl EraseTarget {
    /// Create erase target from volume info
    pub fn from_volume(volume: &VolumeInfo) -> Self {
        let mut warnings = Vec::new();

        // Check if it's flash/SSD
        let is_flash = volume.physical_drive.medium_type.as_deref() == Some("ssd")
            || volume.physical_drive.protocol == "USB"; // Most USB drives are flash

        if is_flash {
            warnings.push(
                "This appears to be a flash-based device (SSD/USB). Due to wear leveling, \
                 some data may theoretically remain in unmapped blocks. For absolute certainty, \
                 physical destruction is the only guarantee."
                    .to_string(),
            );
        }

        // Use raw device for better performance on macOS
        let device_path = if cfg!(target_os = "macos") {
            format!("/dev/r{}", volume.bsd_name) // raw device
        } else {
            format!("/dev/{}", volume.bsd_name)
        };

        Self {
            device_path,
            name: volume.name.clone(),
            size_bytes: volume.size_bytes,
            is_flash,
            mount_point: Some(volume.mount_point.clone()),
            warnings,
        }
    }

    /// Format size for display
    pub fn size_display(&self) -> String {
        const GB: u64 = 1_000_000_000;
        const MB: u64 = 1_000_000;

        if self.size_bytes >= GB {
            format!("{:.1} GB", self.size_bytes as f64 / GB as f64)
        } else {
            format!("{:.1} MB", self.size_bytes as f64 / MB as f64)
        }
    }
}

/// Erase an entire volume/disk
///
/// # Safety
///
/// This will **permanently destroy all data** on the device.
/// The operation cannot be undone.
///
/// # Arguments
///
/// * `volume` - The volume to erase (from `VolumeInfo::for_path()`)
/// * `options` - Erase options (level, verification, etc.)
/// * `progress` - Callback for progress updates
///
/// # Example
///
/// ```no_run
/// use std::path::Path;
/// use zero::disk::{VolumeInfo, erase::{EraseLevel, EraseOptions, erase_volume}};
///
/// let volume = VolumeInfo::for_path(Path::new("/Volumes/MyUSB")).unwrap();
/// let options = EraseOptions::new(EraseLevel::Strong);
///
/// erase_volume(&volume, options, |progress| {
///     println!("[{}/{}] Pass {}/{}: {}%",
///         progress.stage + 1,
///         progress.total_stages,
///         progress.pass + 1,
///         progress.total_passes,
///         progress.percent()
///     );
/// }).unwrap();
/// ```
pub fn erase_volume<F>(
    volume: &VolumeInfo,
    options: EraseOptions,
    progress: F,
) -> Result<EraseResult, EraseError>
where
    F: FnMut(EraseProgress),
{
    // Safety check: refuse to erase internal/system disks
    if volume.physical_drive.is_internal {
        return Err(EraseError::SystemDisk(volume.name.clone()));
    }

    // Check if writable
    if !volume.writable {
        return Err(EraseError::NotWritable(volume.name.clone()));
    }

    let target = EraseTarget::from_volume(volume);
    let scheme = options.level.scheme();

    wipe::run_wipe(&target, &scheme, &options, progress)
}

/// Prepare an erase operation and return target info for confirmation
///
/// Use this to show the user what will be erased before proceeding.
pub fn prepare_erase(volume: &VolumeInfo) -> Result<EraseTarget, EraseError> {
    // Safety check: refuse to erase internal/system disks
    if volume.physical_drive.is_internal {
        return Err(EraseError::SystemDisk(volume.name.clone()));
    }

    // Check if writable
    if !volume.writable {
        return Err(EraseError::NotWritable(volume.name.clone()));
    }

    Ok(EraseTarget::from_volume(volume))
}

/// Erase a device by path (advanced use)
///
/// This bypasses some safety checks. Use `erase_volume` for normal use.
///
/// # Safety
///
/// The caller must ensure the path points to a valid, non-system device.
pub fn erase_device<P, F>(
    device_path: P,
    size_bytes: u64,
    options: EraseOptions,
    progress: F,
) -> Result<EraseResult, EraseError>
where
    P: AsRef<Path>,
    F: FnMut(EraseProgress),
{
    let target = EraseTarget {
        device_path: device_path.as_ref().to_string_lossy().to_string(),
        name: device_path
            .as_ref()
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string()),
        size_bytes,
        is_flash: false, // Unknown
        mount_point: None,
        warnings: vec!["Direct device access - safety checks bypassed".to_string()],
    };

    let scheme = options.level.scheme();
    wipe::run_wipe(&target, &scheme, &options, progress)
}

#[cfg(test)]
#[path = "mod_test.rs"]
mod mod_test;
