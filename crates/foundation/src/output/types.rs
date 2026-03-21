//! Response types for all commands
//!
//! These types are used for both JSON serialization and structured human output.

use serde::Serialize;
use std::path::PathBuf;

/// Universal response envelope for all commands
#[derive(Debug, Serialize)]
pub struct Response<T: Serialize> {
    /// Whether the command succeeded
    pub success: bool,
    /// Command name
    pub command: String,
    /// Duration in milliseconds
    pub duration_ms: u64,
    /// Command-specific data (null on error)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    /// Error details (null on success)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorInfo>,
}

impl<T: Serialize> Response<T> {
    /// Create a successful response
    pub fn success(command: impl Into<String>, duration_ms: u64, data: T) -> Self {
        Self {
            success: true,
            command: command.into(),
            duration_ms,
            data: Some(data),
            error: None,
        }
    }

    /// Create an error response
    pub fn error(command: impl Into<String>, duration_ms: u64, error: ErrorInfo) -> Self {
        Self {
            success: false,
            command: command.into(),
            duration_ms,
            data: None,
            error: Some(error),
        }
    }
}

/// Error information
#[derive(Debug, Serialize)]
pub struct ErrorInfo {
    /// Error code for programmatic handling
    pub code: String,
    /// Human-readable error message
    pub message: String,
}

impl ErrorInfo {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }

    /// Create from an anyhow error
    pub fn from_error(err: &anyhow::Error) -> Self {
        Self {
            code: "ERROR".to_string(),
            message: err.to_string(),
        }
    }
}

// ============================================================================
// Command-specific response types
// ============================================================================

/// Response data for `scan` command
#[derive(Debug, Serialize)]
pub struct ScanData {
    pub path: PathBuf,
    pub total_files: usize,
    pub total_dirs: usize,
    pub total_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files_per_second: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes_per_second: Option<f64>,
}

/// Response data for `diff` command
#[derive(Debug, Serialize)]
pub struct DiffData {
    pub source: PathBuf,
    pub dest: PathBuf,
    pub summary: DiffOutputSummary,
    pub changes: Vec<DiffChange>,
}

/// Summary of diff results for JSON output
/// Note: Named `DiffOutputSummary` to avoid collision with `sync::DiffSummary`
#[derive(Debug, Serialize)]
pub struct DiffOutputSummary {
    pub identical: usize,
    pub modified: usize,
    pub source_only: usize,
    pub dest_only: usize,
}

#[derive(Debug, Serialize)]
pub struct DiffChange {
    pub status: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dest_size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_modified: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dest_modified: Option<u64>,
}

/// Response data for `transfer` command
#[derive(Debug, Serialize)]
pub struct TransferData {
    pub source: PathBuf,
    pub dest: PathBuf,
    pub files_copied: usize,
    pub bytes_copied: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub throughput_mbps: Option<f64>,
    pub verified: bool,
    pub errors: Vec<TransferError>,
    /// Number of directories whose permissions were synced (if `preserve_permissions` enabled)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dirs_permissions_synced: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct TransferError {
    pub path: String,
    pub message: String,
}

/// Response data for `sync` command
#[derive(Debug, Serialize)]
pub struct SyncData {
    pub source: PathBuf,
    pub dest: PathBuf,
    pub files_copied: usize,
    pub files_deleted: usize,
    pub files_unchanged: usize,
    pub bytes_copied: u64,
    pub bytes_deleted: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub throughput_mbps: Option<f64>,
    /// Number of directories whose permissions were synced (if `preserve_permissions` enabled)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dirs_permissions_synced: Option<usize>,
}

/// Response data for `dupes` command
#[derive(Debug, Serialize)]
pub struct DupesData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    pub files_scanned: usize,
    pub bytes_scanned: u64,
    pub duplicate_groups: Vec<DuplicateGroupData>,
    pub total_duplicate_count: usize,
    pub total_wasted_bytes: u64,
    /// Whether duplicates were deleted
    pub deleted: bool,
    /// Number of files deleted (if delete was requested)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files_deleted: Option<usize>,
    /// Bytes reclaimed (if delete was requested)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes_reclaimed: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct DuplicateGroupData {
    pub hash: String,
    pub size: u64,
    pub count: usize,
    pub wasted_bytes: u64,
    pub files: Vec<DuplicateFileData>,
}

#[derive(Debug, Serialize)]
pub struct DuplicateFileData {
    pub path: PathBuf,
    pub keep: bool,
}

/// Response data for `verify` command
#[derive(Debug, Serialize)]
pub struct VerifyData {
    pub source: PathBuf,
    pub dest: PathBuf,
    /// Whether verification passed (all files match)
    pub verified: bool,
    pub files_checked: usize,
    pub bytes_hashed: u64,
    pub mismatches: Vec<VerifyMismatch>,
    pub missing_in_dest: Vec<String>,
    pub extra_in_dest: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct VerifyMismatch {
    pub path: String,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dest_hash: Option<String>,
}

/// Response data for `status` command
#[derive(Debug, Serialize)]
pub struct StatusData {
    pub jobs: Vec<JobInfo>,
}

#[derive(Debug, Serialize)]
pub struct JobInfo {
    pub id: u64,
    pub command: String,
    pub status: String,
    pub progress_percent: Option<f64>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

// ============================================================================
// Progress events (for streaming updates)
// ============================================================================

/// Progress event emitted during long-running operations
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum ProgressEvent<T: Serialize> {
    /// Progress update
    #[serde(rename = "progress")]
    Progress(ProgressInfo),
    /// Final result
    #[serde(rename = "result")]
    Result(Response<T>),
}

/// Response data for `drives` command
#[derive(Debug, Serialize)]
pub struct DrivesData {
    pub drives: Vec<DriveInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DriveInfo {
    /// Drive name
    pub name: String,
    /// Mount path (e.g., /Volumes/Ghost)
    pub path: String,
    /// Total capacity in bytes
    pub total_bytes: u64,
    /// Used space in bytes
    pub used_bytes: u64,
    /// Free space in bytes
    pub free_bytes: u64,
    /// Used percentage (0-100)
    pub used_percent: f64,
    /// File system type (e.g., "APFS", "HFS+")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_system: Option<String>,
    /// BSD device name (e.g., "disk22s1")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bsd_name: Option<String>,
    /// Connection protocol (e.g., "USB", "Apple Fabric", "SATA")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol: Option<String>,
    /// Whether this is an internal disk
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_internal: Option<bool>,
    /// Physical device name (e.g., "`SanDisk` 3.2Gen1")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_name: Option<String>,
    /// USB-specific hardware info (only for USB devices)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usb: Option<UsbHardwareInfo>,
}

/// USB hardware information
#[derive(Debug, Clone, Serialize)]
pub struct UsbHardwareInfo {
    /// Product name (e.g., "`SanDisk` 3.2Gen1")
    pub product_name: String,
    /// Vendor name (e.g., "`SanDisk`")
    pub vendor_name: String,
    /// USB Vendor ID (e.g., 1921)
    pub vendor_id: u32,
    /// USB Product ID
    pub product_id: u32,
    /// Serial number (unique device identifier)
    pub serial_number: String,
    /// USB speed name (e.g., "USB 3.0 `SuperSpeed`")
    pub speed: String,
    /// Maximum theoretical throughput (e.g., "5 Gbps (~625 MB/s)")
    pub max_throughput: String,
    /// Realistic throughput estimate (e.g., "100-400 MB/s")
    pub realistic_throughput: String,
    /// USB protocol version (e.g., "3.0")
    pub usb_version: String,
    /// Power allocation in mA (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub power_ma: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct ProgressInfo {
    /// Progress percentage (0-100)
    pub percent: f64,
    /// Current phase of operation
    pub phase: String,
    /// Human-readable message
    pub message: String,
    /// Files processed so far
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files_processed: Option<usize>,
    /// Bytes processed so far
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes_processed: Option<u64>,
}

// ============================================================================
// Automation command response types
// ============================================================================

/// Response data for `automation list` command
#[derive(Debug, Serialize)]
pub struct AutomationListData {
    pub automations: Vec<AutomationInfo>,
    pub total: usize,
}

/// Summary info for an automation (used in list)
#[derive(Debug, Serialize)]
pub struct AutomationInfo {
    pub id: i64,
    pub name: String,
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dest_device_serial: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dest_volume_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dest_path: Option<String>,
    pub triggers: AutomationTriggers,
    pub paths_count: usize,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Response data for `automation show` command
#[derive(Debug, Serialize)]
pub struct AutomationShowData {
    pub id: i64,
    pub name: String,
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dest_device_serial: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dest_volume_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dest_path: Option<String>,
    pub triggers: AutomationTriggers,
    pub paths: Vec<AutomationPath>,
    pub settings: AutomationSettingsData,
    pub created_at: i64,
    pub updated_at: i64,
    /// Last run info (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_run: Option<AutomationRunInfo>,
}

/// Trigger configuration
#[derive(Debug, Serialize)]
pub struct AutomationTriggers {
    pub on_mount: bool,
    pub on_change: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_schedule: Option<String>,
}

/// Path mapping in an automation
#[derive(Debug, Serialize)]
pub struct AutomationPath {
    pub source: String,
    pub dest: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub exclude: Vec<String>,
}

/// Automation settings
#[derive(Debug, Serialize)]
pub struct AutomationSettingsData {
    pub verify: bool,
    pub delete_orphans: bool,
    pub notify: bool,
    pub debounce_ms: u64,
}

/// Response data for `automation create` command
#[derive(Debug, Serialize)]
pub struct AutomationCreateData {
    pub id: i64,
    pub name: String,
}

/// Response data for `automation delete` command
#[derive(Debug, Serialize)]
pub struct AutomationDeleteData {
    pub id: i64,
    pub name: String,
    pub deleted: bool,
}

/// Response data for `automation run` command
#[derive(Debug, Serialize)]
pub struct AutomationRunData {
    pub automation_id: i64,
    pub automation_name: String,
    pub run_id: i64,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<AutomationRunResult>,
}

/// Run result summary
#[derive(Debug, Serialize)]
pub struct AutomationRunResult {
    pub files_added: u64,
    pub files_modified: u64,
    pub files_deleted: u64,
    pub files_unchanged: u64,
    pub bytes_transferred: u64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
}

/// Response data for `automation history` command
#[derive(Debug, Serialize)]
pub struct AutomationHistoryData {
    pub automation_id: i64,
    pub automation_name: String,
    pub runs: Vec<AutomationRunInfo>,
    pub total: usize,
}

/// Info about a single run
#[derive(Debug, Serialize)]
pub struct AutomationRunInfo {
    pub id: i64,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger: Option<String>,
    pub started_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files_total: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes_transferred: Option<u64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
}

impl ProgressInfo {
    pub fn new(percent: f64, phase: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            percent,
            phase: phase.into(),
            message: message.into(),
            files_processed: None,
            bytes_processed: None,
        }
    }

    pub fn with_files(mut self, files: usize) -> Self {
        self.files_processed = Some(files);
        self
    }

    pub fn with_bytes(mut self, bytes: u64) -> Self {
        self.bytes_processed = Some(bytes);
        self
    }
}
