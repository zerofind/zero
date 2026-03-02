# zero-watcher

File system and USB mount watching for zero.

## Overview

This crate provides real-time monitoring of:
- **File system changes** — create, modify, delete, rename events
- **USB mount/unmount events** — detect when drives are connected/disconnected

Designed for integration with the zero sync engine to enable:
- Auto-sync when files change in watched folders
- Auto-sync when specific USB drives are connected

## Architecture

The library is designed with future Swift FFI in mind:
- Events are serialized as JSON for easy cross-language communication
- Callbacks are used for event delivery
- All public APIs are designed to be C-FFI compatible in the future

```
┌─────────────────────────────────┐
│     Swift/GUI Application       │
└──────────────┬──────────────────┘
               │ (future C FFI)
┌──────────────▼──────────────────┐
│      zero-watcher             │
│                                 │
│  ┌───────────┐  ┌────────────┐  │
│  │FileWatcher│  │ UsbWatcher │  │
│  │ (notify)  │  │(DiskArb.)  │  │
│  └───────────┘  └────────────┘  │
└─────────────────────────────────┘
```

## Usage

### File Watching

```rust
use zero_watcher::{FileWatcher, FileWatchConfig};
use std::path::Path;
use std::time::Duration;

fn main() -> anyhow::Result<()> {
    // Create watcher with default config (100ms debounce)
    let mut watcher = FileWatcher::new()?;
    
    // Watch a directory
    watcher.watch(Path::new("/Users/me/Documents"))?;
    
    // Poll for events
    loop {
        if let Some(event) = watcher.next_event_timeout(Duration::from_millis(100)) {
            println!("{:?}: {:?}", event.kind, event.paths);
        }
    }
}
```

### USB Watching

```rust
use zero_watcher::UsbWatcher;
use std::time::Duration;

fn main() -> anyhow::Result<()> {
    let mut watcher = UsbWatcher::new()?;
    
    // List currently mounted volumes
    for vol in UsbWatcher::current_volumes() {
        println!("Mounted: {}", vol.display());
    }
    
    // Watch for mount/unmount events
    loop {
        if let Some(event) = watcher.next_event_timeout(Duration::from_millis(100)) {
            println!("{:?}: {} ({})", 
                event.kind, 
                event.mount_point.display(),
                event.volume_name.as_deref().unwrap_or("unknown")
            );
        }
    }
}
```

## CLI Tool

The crate includes a CLI for testing:

```bash
# Watch files in a directory
zero-watch files /path/to/watch

# Watch for USB events
zero-watch usb

# Watch both
zero-watch all /path/to/watch

# Measure file event latency
zero-watch latency /path/to/watch --iterations 20

# JSON output for scripting
zero-watch files /path --json
```

## Event Types

### FileEvent

```json
{
  "kind": "modified",
  "paths": ["/Users/me/Documents/file.txt"],
  "watch_root": "/Users/me/Documents",
  "timestamp_ms": 1699000000000
}
```

Kinds: `created`, `modified`, `deleted`, `renamed`, `metadata`, `other`

### UsbEvent

```json
{
  "kind": "mounted",
  "mount_point": "/Volumes/MyDrive",
  "volume_name": "MyDrive",
  "device_serial": "SanDisk-SanDisk 3.2Gen1",
  "bsd_name": "disk4s1",
  "volume_uuid": "A1B2C3D4-...",
  "file_system": "apfs",
  "is_writable": true,
  "capacity_bytes": 128000000000,
  "timestamp_ms": 1699000000000
}
```

Kinds: `mounted`, `unmounted`, `unmounting`

## Performance

Tested on macOS with FSEvents backend:

| Metric | Value |
|--------|-------|
| File event latency | ~11ms |
| Debounce default | 100ms |
| CPU usage (idle) | <0.1% |

## Configuration

### FileWatchConfig

```rust
FileWatchConfig {
    debounce_ms: 100,           // Coalesce rapid changes
    recursive: true,             // Watch subdirectories
    ignore_patterns: vec![       // Skip these patterns
        ".git",
        ".DS_Store",
        "*.swp",
        "*~",
        ".zero",
    ],
}
```

### UsbWatchConfig

```rust
UsbWatchConfig {
    volume_filter: vec![],       // Only these volume names (empty = all)
    serial_filter: vec![],       // Only these device serials (empty = all)
}
```

## macOS Frameworks

- **File watching**: Uses the `notify` crate with FSEvents backend
- **USB watching**: Uses DiskArbitration framework via core-foundation bindings

## Future Work

- [ ] C FFI exports for Swift integration
- [ ] IOKit integration for true USB serial numbers
- [ ] Async/await API with tokio channels
- [ ] Linux support (inotify + udev)
- [ ] Windows support (ReadDirectoryChangesW + WMI)

## License

MIT