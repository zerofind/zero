//! USB mount/unmount watcher using macOS `DiskArbitration` framework
//!
//! This module monitors for volume mount and unmount events on macOS.
//! It uses the `DiskArbitration` framework via core-foundation bindings.
//!
//! Note: `DiskArbitration`'s disappear callback can be unreliable for detecting
//! unmounts, so we also use a polling fallback to detect when volumes disappear.

use crate::UsbWatchConfig;
use crate::events::{UsbEvent, UsbEventKind};
use anyhow::Result;
use core_foundation::base::{CFType, TCFType, kCFAllocatorDefault};
use core_foundation::dictionary::CFDictionary;
use core_foundation::runloop::{
    CFRunLoop, CFRunLoopRef, kCFRunLoopCommonModes, kCFRunLoopDefaultMode,
};
use core_foundation::string::CFString;
use crossfire::mpsc as cf_mpsc;
use std::collections::HashMap;
use std::ffi::c_void;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

// DiskArbitration framework bindings
#[link(name = "DiskArbitration", kind = "framework")]
unsafe extern "C" {
    fn DASessionCreate(allocator: core_foundation::base::CFAllocatorRef) -> DASessionRef;
    fn DASessionScheduleWithRunLoop(
        session: DASessionRef,
        runLoop: CFRunLoopRef,
        runLoopMode: core_foundation::string::CFStringRef,
    );
    fn DASessionUnscheduleFromRunLoop(
        session: DASessionRef,
        runLoop: CFRunLoopRef,
        runLoopMode: core_foundation::string::CFStringRef,
    );
    fn DARegisterDiskAppearedCallback(
        session: DASessionRef,
        match_dict: core_foundation::dictionary::CFDictionaryRef,
        callback: DADiskAppearedCallback,
        context: *mut c_void,
    );
    fn DARegisterDiskDisappearedCallback(
        session: DASessionRef,
        match_dict: core_foundation::dictionary::CFDictionaryRef,
        callback: DADiskDisappearedCallback,
        context: *mut c_void,
    );

    fn DADiskCopyDescription(disk: DADiskRef) -> core_foundation::dictionary::CFDictionaryRef;

}

// Opaque types
#[repr(C)]
struct __DASession(c_void);
type DASessionRef = *mut __DASession;

#[repr(C)]
struct __DADisk(c_void);
type DADiskRef = *mut __DADisk;

// Callback types
type DADiskAppearedCallback = extern "C" fn(disk: DADiskRef, context: *mut c_void);
type DADiskDisappearedCallback = extern "C" fn(disk: DADiskRef, context: *mut c_void);

// DiskArbitration keys
const DA_DISK_DESCRIPTION_VOLUME_PATH_KEY: &str = "DAVolumePathKey";
const DA_DISK_DESCRIPTION_VOLUME_NAME_KEY: &str = "DAVolumeNameKey";
const DA_DISK_DESCRIPTION_VOLUME_UUID_KEY: &str = "DAVolumeUUIDKey";
const DA_DISK_DESCRIPTION_MEDIA_BSD_NAME_KEY: &str = "DAMediaBSDNameKey";
const DA_DISK_DESCRIPTION_MEDIA_SIZE_KEY: &str = "DAMediaSizeKey";
const DA_DISK_DESCRIPTION_MEDIA_WRITABLE_KEY: &str = "DAMediaWritableKey";
const DA_DISK_DESCRIPTION_VOLUME_KIND_KEY: &str = "DAVolumeKindKey";
const DA_DISK_DESCRIPTION_DEVICE_VENDOR_KEY: &str = "DADeviceVendorKey";
const DA_DISK_DESCRIPTION_DEVICE_MODEL_KEY: &str = "DADeviceModelKey";

/// USB mount/unmount watcher
pub struct UsbWatcher {
    /// Channel to receive events
    event_rx: crossfire::Rx<cf_mpsc::List<UsbEvent>>,

    /// Handle to stop the watcher thread
    stop_tx: crossfire::MTx<cf_mpsc::List<()>>,

    /// The watcher thread handle
    thread_handle: Option<thread::JoinHandle<()>>,

    /// Configuration
    config: UsbWatchConfig,
}

/// Information about a known volume
#[derive(Debug, Clone)]
struct VolumeInfo {
    mount_point: PathBuf,
    volume_name: Option<String>,
    device_serial: Option<String>,
    file_system: Option<String>,
}

/// Shared state for the watcher callbacks
struct WatcherState {
    event_tx: crossfire::MTx<cf_mpsc::List<UsbEvent>>,
    /// Track volumes by BSD name (e.g., "disk4s1") -> volume info
    known_volumes: HashMap<String, VolumeInfo>,
    /// Last time we polled for unmounts
    last_poll: Instant,
}

impl UsbWatcher {
    /// Create a new USB watcher with default configuration
    pub fn new() -> Result<Self> {
        Self::with_config(UsbWatchConfig::default())
    }

    /// Create a new USB watcher with custom configuration
    pub fn with_config(config: UsbWatchConfig) -> Result<Self> {
        let (event_tx, event_rx) = cf_mpsc::unbounded_blocking();
        let (stop_tx, stop_rx) = cf_mpsc::unbounded_blocking();

        // Initialize known volumes by scanning /Volumes
        let known_volumes = scan_current_volumes_map();

        let state = Arc::new(Mutex::new(WatcherState {
            event_tx,
            known_volumes,
            last_poll: Instant::now(),
        }));

        let state_clone = Arc::clone(&state);
        let thread_handle = thread::spawn(move || {
            run_disk_arbitration_loop(state_clone, stop_rx);
        });

        Ok(Self {
            event_rx,
            stop_tx,
            thread_handle: Some(thread_handle),
            config,
        })
    }

    /// Try to receive the next event (non-blocking)
    pub fn try_next_event(&mut self) -> Option<UsbEvent> {
        match self.event_rx.try_recv() {
            Ok(event) => self.filter_event(event),
            Err(crossfire::TryRecvError::Empty) => None,
            Err(crossfire::TryRecvError::Disconnected) => {
                tracing::error!("USB watcher channel disconnected (try_recv)");
                None
            }
        }
    }

    /// Receive the next event (blocking)
    pub fn next_event(&mut self) -> Option<UsbEvent> {
        if let Ok(event) = self.event_rx.recv() {
            self.filter_event(event)
        } else {
            tracing::error!("USB watcher channel disconnected (recv)");
            None
        }
    }

    /// Receive the next event with timeout
    pub fn next_event_timeout(&mut self, timeout: Duration) -> Option<UsbEvent> {
        match self.event_rx.recv_timeout(timeout) {
            Ok(event) => self.filter_event(event),
            Err(crossfire::RecvTimeoutError::Timeout) => None,
            Err(crossfire::RecvTimeoutError::Disconnected) => {
                tracing::error!("USB watcher channel disconnected (recv_timeout)");
                None
            }
        }
    }

    /// Apply filters from config
    fn filter_event(&self, event: UsbEvent) -> Option<UsbEvent> {
        // Filter by volume path if configured
        if !self.config.volume_filter.is_empty() {
            let path_str = event.mount_point.to_string_lossy();
            if !self
                .config
                .volume_filter
                .iter()
                .any(|f| path_str.contains(f))
            {
                return None;
            }
        }

        // Filter by serial if configured
        if !self.config.serial_filter.is_empty() {
            if let Some(ref serial) = event.device_serial {
                if !self.config.serial_filter.contains(serial) {
                    return None;
                }
            } else {
                // No serial available, skip if we're filtering by serial
                return None;
            }
        }

        Some(event)
    }

    /// Stop the watcher
    pub fn stop(&mut self) {
        let _ = self.stop_tx.send(());
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }

    /// Get the list of currently mounted volumes
    pub fn current_volumes() -> Vec<PathBuf> {
        scan_current_volumes_set().into_iter().collect()
    }
}

impl Drop for UsbWatcher {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Scan /Volumes for currently mounted volumes, returning a set of paths
fn scan_current_volumes_set() -> std::collections::HashSet<PathBuf> {
    let mut volumes = std::collections::HashSet::new();

    if let Ok(entries) = std::fs::read_dir("/Volumes") {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                volumes.insert(path);
            }
        }
    }

    // Also include the root volume
    volumes.insert(PathBuf::from("/"));

    volumes
}

/// Scan /Volumes for currently mounted volumes, returning a map with BSD-like keys
fn scan_current_volumes_map() -> HashMap<String, VolumeInfo> {
    let mut volumes = HashMap::new();

    if let Ok(entries) = std::fs::read_dir("/Volumes") {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // Use the volume name as a pseudo-BSD name for initial scan
                let key = path.file_name().map_or_else(
                    || path.to_string_lossy().to_string(),
                    |n| n.to_string_lossy().to_string(),
                );

                volumes.insert(
                    key,
                    VolumeInfo {
                        mount_point: path.clone(),
                        volume_name: path.file_name().map(|n| n.to_string_lossy().to_string()),
                        device_serial: None,
                        file_system: None,
                    },
                );
            }
        }
    }

    // Also include the root volume
    volumes.insert(
        "root".to_string(),
        VolumeInfo {
            mount_point: PathBuf::from("/"),
            volume_name: Some("Macintosh HD".to_string()),
            device_serial: None,
            file_system: Some("apfs".to_string()),
        },
    );

    volumes
}

/// Run the `DiskArbitration` event loop
fn run_disk_arbitration_loop(
    state: Arc<Mutex<WatcherState>>,
    stop_rx: crossfire::Rx<cf_mpsc::List<()>>,
) {
    // SAFETY: All DiskArbitration calls require an unsafe block because they are
    // FFI into Apple's C framework. The session is created, used, and cleaned up
    // within this function. The Arc<Mutex<WatcherState>> is passed to callbacks
    // via Arc::into_raw and reconstructed via Arc::from_raw on cleanup.
    unsafe {
        let session = DASessionCreate(kCFAllocatorDefault);
        if session.is_null() {
            tracing::error!("Failed to create DiskArbitration session (null DASessionRef)");
            return;
        }

        let run_loop = CFRunLoop::get_current();
        let run_loop_ref = run_loop.as_concrete_TypeRef();

        DASessionScheduleWithRunLoop(session, run_loop_ref, kCFRunLoopCommonModes);

        // SAFETY: Arc::into_raw increments the refcount. The matching
        // Arc::from_raw at the end of this function balances it. The raw
        // pointer is passed to DA callbacks which only borrow it (no from_raw).
        let state_ptr = Arc::into_raw(Arc::clone(&state)) as *mut c_void;

        DARegisterDiskAppearedCallback(
            session,
            std::ptr::null(),
            disk_appeared_callback,
            state_ptr,
        );

        DARegisterDiskDisappearedCallback(
            session,
            std::ptr::null(),
            disk_disappeared_callback,
            state_ptr,
        );

        loop {
            core_foundation::runloop::CFRunLoopRunInMode(
                kCFRunLoopDefaultMode,
                0.1, // 100ms
                u8::from(false),
            );

            if stop_rx.try_recv().is_ok() {
                break;
            }

            poll_for_unmounts(&state);
        }

        // Cleanup: unschedule before dropping the Arc the callbacks reference
        DASessionUnscheduleFromRunLoop(session, run_loop_ref, kCFRunLoopCommonModes);

        // SAFETY: Balances the Arc::into_raw above. Callbacks are unregistered
        // (session unscheduled) so no further access to state_ptr can occur.
        let _ = Arc::from_raw(state_ptr as *const Mutex<WatcherState>);
    }
}

/// Poll for volumes that have been unmounted (fallback detection)
fn poll_for_unmounts(state: &Arc<Mutex<WatcherState>>) {
    let mut guard = match state.lock() {
        Ok(g) => g,
        Err(_) => return,
    };

    // Only poll every 500ms
    if guard.last_poll.elapsed() < Duration::from_millis(500) {
        return;
    }
    guard.last_poll = Instant::now();

    // Get current volumes
    let current_volumes = scan_current_volumes_set();

    // Find volumes that are in our known list but no longer exist
    let mut unmounted = Vec::new();
    for (key, info) in &guard.known_volumes {
        if !current_volumes.contains(&info.mount_point) {
            unmounted.push((key.clone(), info.clone()));
        }
    }

    // Send unmount events for disappeared volumes
    for (key, info) in unmounted {
        guard.known_volumes.remove(&key);

        let mut event = UsbEvent::unmounted(info.mount_point);
        if let Some(name) = info.volume_name {
            event = event.with_volume_name(name);
        }
        if let Some(serial) = info.device_serial {
            event = event.with_serial(serial);
        }
        if let Some(fs) = info.file_system {
            event = event.with_file_system(fs);
        }

        let _ = guard.event_tx.send(event);
    }
}

/// Callback when a disk appears (is mounted)
extern "C" fn disk_appeared_callback(disk: DADiskRef, context: *mut c_void) {
    if context.is_null() || disk.is_null() {
        return;
    }

    // SAFETY: context is an Arc::into_raw'd pointer to Mutex<WatcherState>.
    // We borrow it (no Arc::from_raw) — ownership stays with the event loop.
    unsafe {
        let state = &*(context as *const Mutex<WatcherState>);

        if let Some((bsd_name, event)) = extract_disk_info_with_bsd(disk, UsbEventKind::Mounted) {
            let mut guard = match state.lock() {
                Ok(g) => g,
                Err(e) => {
                    tracing::error!(error = %e, "Failed to lock watcher state");
                    return;
                }
            };

            // Use BSD name as key, or fall back to mount point
            let key = bsd_name.unwrap_or_else(|| event.mount_point.to_string_lossy().to_string());

            // Check if this is a new volume
            if guard.known_volumes.contains_key(&key) {
                return; // Already known, skip
            }

            // Add to known volumes
            guard.known_volumes.insert(
                key,
                VolumeInfo {
                    mount_point: event.mount_point.clone(),
                    volume_name: event.volume_name.clone(),
                    device_serial: event.device_serial.clone(),
                    file_system: event.file_system.clone(),
                },
            );

            // Send the event
            let _ = guard.event_tx.send(event);
        }
    }
}

/// Callback when a disk disappears (is unmounted)
/// Note: This callback is often unreliable on macOS, so we also use polling
extern "C" fn disk_disappeared_callback(disk: DADiskRef, context: *mut c_void) {
    if context.is_null() || disk.is_null() {
        return;
    }

    // SAFETY: Same as disk_appeared_callback — borrowed, not owned.
    unsafe {
        let state = &*(context as *const Mutex<WatcherState>);

        // Try to get disk info - may fail for disappearing disks
        if let Some((bsd_name, event)) = extract_disk_info_with_bsd(disk, UsbEventKind::Unmounted) {
            let mut guard = match state.lock() {
                Ok(g) => g,
                Err(e) => {
                    tracing::error!(error = %e, "Failed to lock watcher state");
                    return;
                }
            };

            let key = bsd_name.unwrap_or_else(|| event.mount_point.to_string_lossy().to_string());

            // Remove from known volumes
            guard.known_volumes.remove(&key);

            // Send the event
            let _ = guard.event_tx.send(event);
        }
    }
}

/// Extract disk information from a `DADisk` reference, returning BSD name separately
unsafe fn extract_disk_info_with_bsd(
    disk: DADiskRef,
    kind: UsbEventKind,
) -> Option<(Option<String>, UsbEvent)> {
    let desc_dict = DADiskCopyDescription(disk);
    if desc_dict.is_null() {
        return None;
    }

    // Wrap in CFDictionary for safe access
    let dict: CFDictionary<CFString, CFType> = CFDictionary::wrap_under_create_rule(desc_dict);

    // Get BSD name first (we need it for tracking)
    let bsd_name = get_string_value(&dict, DA_DISK_DESCRIPTION_MEDIA_BSD_NAME_KEY);

    // Get mount point (volume path)
    let mount_point =
        get_string_value(&dict, DA_DISK_DESCRIPTION_VOLUME_PATH_KEY).or_else(|| {
            // Try to get it from URL if direct path fails
            get_url_path(&dict, DA_DISK_DESCRIPTION_VOLUME_PATH_KEY)
        })?;

    let mount_path = PathBuf::from(&mount_point);

    // Skip non-volume disks (partitions without mount points)
    if mount_point.is_empty() {
        return None;
    }

    // Build the event
    let mut event = UsbEvent::mounted(mount_path);
    event.kind = kind;

    // Volume name
    if let Some(name) = get_string_value(&dict, DA_DISK_DESCRIPTION_VOLUME_NAME_KEY) {
        event = event.with_volume_name(name);
    }

    // BSD name
    if let Some(ref bsd) = bsd_name {
        event = event.with_bsd_name(bsd.clone());
    }

    // Volume UUID
    if let Some(uuid) = get_uuid_value(&dict, DA_DISK_DESCRIPTION_VOLUME_UUID_KEY) {
        event = event.with_uuid(uuid);
    }

    // File system type
    if let Some(fs) = get_string_value(&dict, DA_DISK_DESCRIPTION_VOLUME_KIND_KEY) {
        event = event.with_file_system(fs);
    }

    // Writable
    if let Some(writable) = get_bool_value(&dict, DA_DISK_DESCRIPTION_MEDIA_WRITABLE_KEY) {
        event = event.with_writable(writable);
    }

    // Media size (capacity)
    if let Some(size) = get_number_value(&dict, DA_DISK_DESCRIPTION_MEDIA_SIZE_KEY) {
        event = event.with_capacity(size as u64);
    }

    // Try to get device serial from IOKit (more complex, skip for now)
    // In a full implementation, we'd query IOKit for the serial number
    // For now, we'll use a combination of vendor + model + BSD name as a pseudo-identifier
    let vendor = get_string_value(&dict, DA_DISK_DESCRIPTION_DEVICE_VENDOR_KEY);
    let model = get_string_value(&dict, DA_DISK_DESCRIPTION_DEVICE_MODEL_KEY);

    if vendor.is_some() || model.is_some() {
        let pseudo_serial = format!(
            "{}-{}",
            vendor.as_deref().unwrap_or("unknown"),
            model.as_deref().unwrap_or("unknown")
        );
        event.device_serial = Some(pseudo_serial);
    }

    Some((bsd_name, event))
}

/// Get a string value from the disk description dictionary
fn get_string_value(dict: &CFDictionary<CFString, CFType>, key: &str) -> Option<String> {
    let key_cf = CFString::new(key);
    dict.find(&key_cf).and_then(|value| {
        // Try to downcast to CFString
        if value.instance_of::<CFString>() {
            // Safety: we just checked the type
            let cf_string: CFString =
                unsafe { CFString::wrap_under_get_rule(value.as_CFTypeRef().cast()) };
            Some(cf_string.to_string())
        } else {
            None
        }
    })
}

/// Get a URL path from the disk description dictionary
fn get_url_path(dict: &CFDictionary<CFString, CFType>, key: &str) -> Option<String> {
    use core_foundation::url::CFURL;

    let key_cf = CFString::new(key);
    dict.find(&key_cf).and_then(|value| {
        // The volume path is actually a CFURL
        if value.instance_of::<CFURL>() {
            // Safety: we just checked the type
            let url: CFURL = unsafe { CFURL::wrap_under_get_rule(value.as_CFTypeRef().cast()) };
            url.to_path().map(|p| p.to_string_lossy().to_string())
        } else {
            None
        }
    })
}

/// Get a UUID value from the disk description dictionary
fn get_uuid_value(dict: &CFDictionary<CFString, CFType>, key: &str) -> Option<String> {
    use core_foundation::uuid::CFUUID;

    let key_cf = CFString::new(key);
    dict.find(&key_cf).and_then(|value| {
        if value.instance_of::<CFUUID>() {
            // Safety: we just checked the type
            let cf_uuid: CFUUID =
                unsafe { CFUUID::wrap_under_get_rule(value.as_CFTypeRef().cast()) };
            // Use the Debug/Description output which gives us the UUID string
            Some(format!("{cf_uuid:?}"))
        } else {
            None
        }
    })
}

/// Get a boolean value from the disk description dictionary
fn get_bool_value(dict: &CFDictionary<CFString, CFType>, key: &str) -> Option<bool> {
    use core_foundation::boolean::CFBoolean;

    let key_cf = CFString::new(key);
    dict.find(&key_cf).and_then(|value| {
        if value.instance_of::<CFBoolean>() {
            // Safety: we just checked the type
            let b: CFBoolean =
                unsafe { CFBoolean::wrap_under_get_rule(value.as_CFTypeRef().cast()) };
            Some(b.into())
        } else {
            None
        }
    })
}

/// Get a number value from the disk description dictionary
fn get_number_value(dict: &CFDictionary<CFString, CFType>, key: &str) -> Option<i64> {
    use core_foundation::number::CFNumber;

    let key_cf = CFString::new(key);
    dict.find(&key_cf).and_then(|value| {
        if value.instance_of::<CFNumber>() {
            // Safety: we just checked the type
            let num: CFNumber =
                unsafe { CFNumber::wrap_under_get_rule(value.as_CFTypeRef().cast()) };
            num.to_i64()
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_current_volumes() {
        let volumes = scan_current_volumes_set();
        // Should at least have the root volume
        assert!(volumes.contains(&PathBuf::from("/")));
    }

    #[test]
    fn test_scan_current_volumes_map() {
        let volumes = scan_current_volumes_map();
        // Should at least have the root volume
        assert!(volumes.contains_key("root"));
    }

    #[test]
    fn test_create_watcher() {
        let watcher = UsbWatcher::new();
        assert!(watcher.is_ok());
    }

    #[test]
    fn test_current_volumes() {
        let volumes = UsbWatcher::current_volumes();
        assert!(!volumes.is_empty());
        assert!(volumes.iter().any(|p| p == &PathBuf::from("/")));
    }

    #[test]
    fn test_watcher_with_filter() {
        let config = UsbWatchConfig {
            volume_filter: vec!["TestDrive".to_string()],
            serial_filter: vec![],
        };
        let watcher = UsbWatcher::with_config(config);
        assert!(watcher.is_ok());
    }
}
