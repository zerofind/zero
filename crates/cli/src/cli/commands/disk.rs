//! Disk and drives command handlers

use disk::{DiskInfo, VolumeInfo};
use foundation::output::{
    DriveInfo, DrivesData, Outputter, Response, UsbHardwareInfo, format_bytes,
};
use foundation::{cmd_error, cmd_success};
use std::time::Instant;

#[allow(clippy::unnecessary_wraps)] // Returns Result for CLI command consistency
pub fn cmd_drives(out: &Outputter) -> anyhow::Result<()> {
    let start = Instant::now();

    // Get all volumes using our disk module
    let volumes = match VolumeInfo::all() {
        Ok(v) => v,
        Err(e) => {
            if out.is_json() {
                let data = DrivesData { drives: vec![] };
                let response = Response::success("drives", 0, data);
                out.result(&response);
            } else {
                out.error(&format!("Failed to get volume info: {e}"));
            }
            return Ok(());
        }
    };

    let mut drives: Vec<DriveInfo> = Vec::new();

    for volume in volumes {
        // Skip system volumes we don't care about (simulators, etc.)
        if volume.mount_point.contains("/CoreSimulator/")
            || volume.mount_point.contains("/System/Volumes/")
            || volume.name.contains("Simulator")
        {
            continue;
        }

        // Try to get USB hardware info if this is a USB device
        let usb_info = if volume.is_usb() {
            DiskInfo::for_path(std::path::Path::new(&volume.mount_point))
                .ok()
                .and_then(|info| info.usb)
                .map(|usb| UsbHardwareInfo {
                    product_name: usb.product_name.clone(),
                    vendor_name: usb.display_vendor().to_string(),
                    vendor_id: usb.vendor_id,
                    product_id: usb.product_id,
                    serial_number: usb.serial_number.clone(),
                    speed: usb.speed.name().to_string(),
                    max_throughput: usb.speed.max_throughput().to_string(),
                    realistic_throughput: usb.speed.realistic_throughput().to_string(),
                    usb_version: usb.usb_version.clone(),
                    power_ma: usb.power_ma,
                })
        } else {
            None
        };

        drives.push(DriveInfo {
            name: volume.name.clone(),
            path: volume.mount_point.clone(),
            total_bytes: volume.size_bytes,
            used_bytes: volume.used_bytes(),
            free_bytes: volume.free_bytes,
            used_percent: volume.usage_percent(),
            file_system: Some(volume.file_system.clone()),
            bsd_name: Some(volume.bsd_name.clone()),
            protocol: Some(volume.physical_drive.protocol.clone()),
            is_internal: Some(volume.physical_drive.is_internal),
            device_name: Some(volume.physical_drive.device_name.clone()),
            usb: usb_info,
        });
    }

    // Sort: external drives first, then by name
    drives.sort_by(|a, b| {
        let a_external = a.is_internal.is_some_and(|i| !i);
        let b_external = b.is_internal.is_some_and(|i| !i);
        match (b_external, a_external) {
            (true, false) => std::cmp::Ordering::Greater,
            (false, true) => std::cmp::Ordering::Less,
            _ => a.name.cmp(&b.name),
        }
    });

    let duration_ms = start.elapsed().as_millis() as u64;

    let data = DrivesData {
        drives: drives.clone(),
    };
    cmd_success!(out, "drives", duration_ms, data, {
        out.header("Connected Drives");
        if drives.is_empty() {
            out.info("No drives found");
            return Ok(());
        }
        for drive in &drives {
            let total_str = format_bytes(drive.total_bytes);
            let free_str = format_bytes(drive.free_bytes);
            out.info(&format!(
                "{} ({})",
                drive.name,
                drive.device_name.as_deref().unwrap_or("Unknown")
            ));
            if let Some(ref usb) = drive.usb {
                out.println(&format!(
                    "├─ Vendor: {} (ID: 0x{:04x})",
                    usb.vendor_name, usb.vendor_id
                ));
                out.println(&format!(
                    "├─ Product: {} (ID: 0x{:04x})",
                    usb.product_name, usb.product_id
                ));
                out.println(&format!("├─ Speed: {} ({})", usb.speed, usb.max_throughput));
                out.println(&format!("├─ Realistic: {}", usb.realistic_throughput));
                out.println(&format!("├─ Serial: {}", usb.serial_number));
                if let Some(power) = usb.power_ma {
                    out.println(&format!("├─ Power: {power} mA"));
                }
            } else {
                out.println(&format!(
                    "├─ Protocol: {}",
                    drive.protocol.as_deref().unwrap_or("Unknown")
                ));
            }
            out.println(&format!(
                "├─ Capacity: {} ({} free, {:.0}% used)",
                total_str, free_str, drive.used_percent
            ));
            out.println(&format!(
                "├─ File System: {}",
                drive.file_system.as_deref().unwrap_or("Unknown")
            ));
            out.println(&format!("├─ Mount Point: {}", drive.path));
            out.println(&format!(
                "└─ BSD Device: /dev/{}",
                drive.bsd_name.as_deref().unwrap_or("unknown")
            ));
            out.newline();
        }
    });

    Ok(())
}

#[allow(clippy::unnecessary_wraps)] // Returns Result for CLI command consistency
pub fn cmd_disk(out: &Outputter, path: &std::path::Path) -> anyhow::Result<()> {
    let start = Instant::now();

    // Get disk info for the path
    let disk_info = match DiskInfo::for_path(path) {
        Ok(info) => info,
        Err(e) => {
            cmd_error!(
                out,
                "disk",
                start.elapsed().as_millis() as u64,
                "DISK_ERROR",
                format!("Failed to get disk info: {}", e)
            );
            return Ok(());
        }
    };

    let duration_ms = start.elapsed().as_millis() as u64;

    // Build drive info struct for JSON output
    let volume = &disk_info.volume;
    let usb_info = disk_info.usb.as_ref().map(|usb| UsbHardwareInfo {
        product_name: usb.product_name.clone(),
        vendor_name: usb.display_vendor().to_string(),
        vendor_id: usb.vendor_id,
        product_id: usb.product_id,
        serial_number: usb.serial_number.clone(),
        speed: usb.speed.name().to_string(),
        max_throughput: usb.speed.max_throughput().to_string(),
        realistic_throughput: usb.speed.realistic_throughput().to_string(),
        usb_version: usb.usb_version.clone(),
        power_ma: usb.power_ma,
    });

    let drive_info = DriveInfo {
        name: volume.name.clone(),
        path: volume.mount_point.clone(),
        total_bytes: volume.size_bytes,
        used_bytes: volume.used_bytes(),
        free_bytes: volume.free_bytes,
        used_percent: volume.usage_percent(),
        file_system: Some(volume.file_system.clone()),
        bsd_name: Some(volume.bsd_name.clone()),
        protocol: Some(volume.physical_drive.protocol.clone()),
        is_internal: Some(volume.physical_drive.is_internal),
        device_name: Some(volume.physical_drive.device_name.clone()),
        usb: usb_info,
    };

    cmd_success!(out, "disk", duration_ms, drive_info, {
        // Build complete tree output
        out.header(&format!("Disk Info for: {}", path.display()));

        // Header: Volume name and device
        out.info(&format!(
            "{} ({})",
            volume.name, volume.physical_drive.device_name
        ));

        // USB details if available
        if let Some(ref usb) = disk_info.usb {
            out.println(&format!(
                "├─ Vendor: {} (ID: 0x{:04x})",
                usb.display_vendor(),
                usb.vendor_id
            ));
            out.println(&format!(
                "├─ Product: {} (ID: 0x{:04x})",
                usb.product_name, usb.product_id
            ));
            out.println(&format!(
                "├─ Speed: {} ({})",
                usb.speed.name(),
                usb.speed.max_throughput()
            ));
            out.println(&format!(
                "├─ Realistic: {}",
                usb.speed.realistic_throughput()
            ));
            out.println(&format!("├─ Serial: {}", usb.serial_number));
            if let Some(power) = usb.power_ma {
                out.println(&format!("├─ Power: {power} mA"));
            }
        } else {
            out.println(&format!("├─ Protocol: {}", volume.physical_drive.protocol));
            if let Some(ref medium) = volume.physical_drive.medium_type {
                out.println(&format!("├─ Type: {}", medium.to_uppercase()));
            }
        }

        // Capacity info
        let total_gb = volume.size_bytes as f64 / 1_000_000_000.0;
        let free_gb = volume.free_bytes as f64 / 1_000_000_000.0;
        out.println(&format!(
            "├─ Capacity: {:.1} GB ({:.1} GB free, {:.0}% used)",
            total_gb,
            free_gb,
            volume.usage_percent()
        ));
        out.println(&format!("├─ File System: {}", volume.file_system));
        out.println(&format!("├─ Mount Point: {}", volume.mount_point));
        out.println(&format!("├─ BSD Device: /dev/{}", volume.bsd_name));
        out.println(&format!("├─ Volume UUID: {}", volume.volume_uuid));
        out.println(&format!(
            "├─ Writable: {}",
            if volume.writable { "Yes" } else { "No" }
        ));

        // Build remaining optional items to determine last one
        let mut remaining: Vec<(&str, String)> = Vec::new();

        remaining.push((
            "Ownership",
            if volume.ignore_ownership {
                "Ignored".to_string()
            } else {
                "Enabled".to_string()
            },
        ));

        if let Some(ref partition_type) = volume.physical_drive.partition_map_type {
            remaining.push(("Partition Map", partition_type.clone()));
        }
        if let Some(ref smart) = volume.physical_drive.smart_status {
            remaining.push(("SMART Status", smart.clone()));
        }

        // Print all but last with ├─, last with └─
        for (i, (label, value)) in remaining.iter().enumerate() {
            let prefix = if i == remaining.len() - 1 {
                "└─"
            } else {
                "├─"
            };
            out.println(&format!("{prefix} {label}: {value}"));
        }
    });

    Ok(())
}
