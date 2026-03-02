//! Event types for file system and USB changes
//!
//! All events are designed to be easily serializable to JSON for FFI.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Unified event type for all watch events
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WatchEvent {
    /// File system change event
    File(FileEvent),
    /// USB mount/unmount event
    Usb(UsbEvent),
}

/// A file system change event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEvent {
    /// The kind of change
    pub kind: FileChangeKind,

    /// Paths affected by this event
    /// Usually one path, but rename events may have two (from, to)
    pub paths: Vec<PathBuf>,

    /// The root watch path that triggered this event
    pub watch_root: PathBuf,

    /// Timestamp of the event (milliseconds since epoch)
    pub timestamp_ms: u64,
}

/// The kind of file system change
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileChangeKind {
    /// File or directory was created
    Created,

    /// File or directory was modified
    Modified,

    /// File or directory was deleted
    Deleted,

    /// File or directory was renamed/moved
    Renamed,

    /// File metadata changed (permissions, etc.)
    Metadata,

    /// Unknown or other change type
    Other,
}

/// A USB mount/unmount event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsbEvent {
    /// The kind of USB event
    pub kind: UsbEventKind,

    /// Mount point path (e.g., "/Volumes/MyDrive")
    pub mount_point: PathBuf,

    /// Volume name/label
    pub volume_name: Option<String>,

    /// Device serial number (if available)
    pub device_serial: Option<String>,

    /// BSD device name (e.g., "disk2s1")
    pub bsd_name: Option<String>,

    /// Volume UUID
    pub volume_uuid: Option<String>,

    /// File system type (e.g., "apfs", "hfs", "exfat")
    pub file_system: Option<String>,

    /// Whether the volume is writable
    pub is_writable: Option<bool>,

    /// Total capacity in bytes
    pub capacity_bytes: Option<u64>,

    /// Timestamp of the event (milliseconds since epoch)
    pub timestamp_ms: u64,
}

/// The kind of USB event
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsbEventKind {
    /// A volume was mounted
    Mounted,

    /// A volume was unmounted
    Unmounted,

    /// A volume is about to be unmounted (chance to finish operations)
    Unmounting,
}

impl WatchEvent {
    /// Serialize the event to JSON
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string(self)
    }

    /// Serialize the event to pretty JSON
    pub fn to_json_pretty(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }

    /// Get the timestamp of the event
    pub fn timestamp_ms(&self) -> u64 {
        match self {
            WatchEvent::File(e) => e.timestamp_ms,
            WatchEvent::Usb(e) => e.timestamp_ms,
        }
    }
}

impl FileEvent {
    /// Create a new file event
    pub fn new(kind: FileChangeKind, paths: Vec<PathBuf>, watch_root: PathBuf) -> Self {
        Self {
            kind,
            paths,
            watch_root,
            timestamp_ms: current_timestamp_ms(),
        }
    }

    /// Serialize to JSON
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string(self)
    }
}

impl UsbEvent {
    /// Create a new USB mount event
    pub fn mounted(mount_point: PathBuf) -> Self {
        Self {
            kind: UsbEventKind::Mounted,
            mount_point,
            volume_name: None,
            device_serial: None,
            bsd_name: None,
            volume_uuid: None,
            file_system: None,
            is_writable: None,
            capacity_bytes: None,
            timestamp_ms: current_timestamp_ms(),
        }
    }

    /// Create a new USB unmount event
    pub fn unmounted(mount_point: PathBuf) -> Self {
        Self {
            kind: UsbEventKind::Unmounted,
            mount_point,
            volume_name: None,
            device_serial: None,
            bsd_name: None,
            volume_uuid: None,
            file_system: None,
            is_writable: None,
            capacity_bytes: None,
            timestamp_ms: current_timestamp_ms(),
        }
    }

    /// Builder: set volume name
    pub fn with_volume_name(mut self, name: impl Into<String>) -> Self {
        self.volume_name = Some(name.into());
        self
    }

    /// Builder: set device serial
    pub fn with_serial(mut self, serial: impl Into<String>) -> Self {
        self.device_serial = Some(serial.into());
        self
    }

    /// Builder: set BSD name
    pub fn with_bsd_name(mut self, name: impl Into<String>) -> Self {
        self.bsd_name = Some(name.into());
        self
    }

    /// Builder: set volume UUID
    pub fn with_uuid(mut self, uuid: impl Into<String>) -> Self {
        self.volume_uuid = Some(uuid.into());
        self
    }

    /// Builder: set file system type
    pub fn with_file_system(mut self, fs: impl Into<String>) -> Self {
        self.file_system = Some(fs.into());
        self
    }

    /// Builder: set writable flag
    pub fn with_writable(mut self, writable: bool) -> Self {
        self.is_writable = Some(writable);
        self
    }

    /// Builder: set capacity
    pub fn with_capacity(mut self, bytes: u64) -> Self {
        self.capacity_bytes = Some(bytes);
        self
    }

    /// Serialize to JSON
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string(self)
    }
}

/// Get current timestamp in milliseconds since Unix epoch
fn current_timestamp_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_event_json() {
        let event = FileEvent::new(
            FileChangeKind::Modified,
            vec![PathBuf::from("/Users/test/file.txt")],
            PathBuf::from("/Users/test"),
        );

        let json = event.to_json().unwrap();
        assert!(json.contains("\"kind\":\"modified\""));
        assert!(json.contains("file.txt"));
    }

    #[test]
    fn test_usb_event_json() {
        let event = UsbEvent::mounted(PathBuf::from("/Volumes/MyDrive"))
            .with_volume_name("MyDrive")
            .with_serial("ABC123")
            .with_file_system("apfs");

        let json = event.to_json().unwrap();
        assert!(json.contains("\"kind\":\"mounted\""));
        assert!(json.contains("MyDrive"));
        assert!(json.contains("ABC123"));
    }

    #[test]
    fn test_watch_event_tagged() {
        let file_event = FileEvent::new(
            FileChangeKind::Created,
            vec![PathBuf::from("/test")],
            PathBuf::from("/"),
        );
        let watch_event = WatchEvent::File(file_event);

        let json = watch_event.to_json().unwrap();
        assert!(json.contains("\"type\":\"file\""));
    }
}
