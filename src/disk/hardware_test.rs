//! Tests for USB hardware information

use super::*;

#[test]
fn test_usb_speed_names() {
    assert_eq!(UsbSpeed::SuperSpeed.name(), "USB 3.0 SuperSpeed");
    assert_eq!(UsbSpeed::HighSpeed.name(), "USB 2.0 High Speed");
    assert_eq!(UsbSpeed::SuperSpeedPlus.name(), "USB 3.1 Gen 2 SuperSpeed+");
}

#[test]
fn test_usb_speed_from_device_speed() {
    assert_eq!(UsbSpeed::from_device_speed(0), UsbSpeed::LowSpeed);
    assert_eq!(UsbSpeed::from_device_speed(2), UsbSpeed::HighSpeed);
    assert_eq!(UsbSpeed::from_device_speed(3), UsbSpeed::SuperSpeed);
    assert_eq!(UsbSpeed::from_device_speed(4), UsbSpeed::SuperSpeedPlus);
}

#[test]
fn test_usb_speed_all_variants() {
    assert_eq!(UsbSpeed::from_device_speed(1), UsbSpeed::FullSpeed);
    assert_eq!(UsbSpeed::from_device_speed(5), UsbSpeed::SuperSpeed20Gbps);
    assert_eq!(UsbSpeed::from_device_speed(6), UsbSpeed::Usb4);
    assert_eq!(UsbSpeed::from_device_speed(99), UsbSpeed::Unknown(99));
}

#[test]
fn test_usb_speed_max_throughput() {
    assert_eq!(UsbSpeed::LowSpeed.max_throughput(), "1.5 Mbps");
    assert_eq!(UsbSpeed::FullSpeed.max_throughput(), "12 Mbps");
    assert_eq!(UsbSpeed::HighSpeed.max_throughput(), "480 Mbps (~60 MB/s)");
    assert_eq!(UsbSpeed::SuperSpeed.max_throughput(), "5 Gbps (~625 MB/s)");
    assert_eq!(UsbSpeed::Usb4.max_throughput(), "40 Gbps (~5 GB/s)");
}

#[test]
fn test_usb_speed_realistic_throughput() {
    assert_eq!(UsbSpeed::HighSpeed.realistic_throughput(), "20-40 MB/s");
    assert_eq!(UsbSpeed::SuperSpeed.realistic_throughput(), "100-400 MB/s");
    assert_eq!(UsbSpeed::Unknown(99).realistic_throughput(), "Unknown");
}

#[test]
fn test_usb_speed_max_bytes_per_second() {
    assert_eq!(UsbSpeed::LowSpeed.max_bytes_per_second(), 187_500);
    assert_eq!(UsbSpeed::HighSpeed.max_bytes_per_second(), 60_000_000);
    assert_eq!(UsbSpeed::SuperSpeed.max_bytes_per_second(), 625_000_000);
    assert_eq!(UsbSpeed::Usb4.max_bytes_per_second(), 5_000_000_000);
    assert_eq!(UsbSpeed::Unknown(99).max_bytes_per_second(), 0);
}

#[test]
fn test_bcd_to_version() {
    assert_eq!(bcd_to_version(0x0200), "2.0");
    assert_eq!(bcd_to_version(0x0300), "3.0");
    assert_eq!(bcd_to_version(0x0310), "3.1");
    assert_eq!(bcd_to_version(0x0320), "3.2");
}

#[test]
fn test_bcd_to_version_with_patch() {
    assert_eq!(bcd_to_version(0x0201), "2.0.1");
    assert_eq!(bcd_to_version(0x0312), "3.1.2");
}

#[test]
fn test_parse_ioreg_property() {
    let line = r#"    "USB Product Name" = "SanDisk 3.2Gen1""#;
    let (key, value) = parse_ioreg_property(line).unwrap();
    assert_eq!(key, "USB Product Name");
    assert_eq!(value, "SanDisk 3.2Gen1");

    let line2 = r#"  |   "idVendor" = 1921"#;
    let (key2, value2) = parse_ioreg_property(line2).unwrap();
    assert_eq!(key2, "idVendor");
    assert_eq!(value2, "1921");
}

#[test]
fn test_parse_ioreg_property_with_pipes() {
    let line = r#"  | |   "Device Speed" = 3"#;
    let (key, value) = parse_ioreg_property(line).unwrap();
    assert_eq!(key, "Device Speed");
    assert_eq!(value, "3");
}

#[test]
fn test_parse_ioreg_property_invalid() {
    // No quotes
    assert!(parse_ioreg_property("invalid line").is_none());
    // No equals
    assert!(parse_ioreg_property(r#""key" without equals"#).is_none());
}

#[test]
fn test_get_usb_devices() {
    // This test requires actual hardware, so just verify it doesn't panic
    let result = get_usb_devices();
    assert!(result.is_ok());
}

#[test]
fn test_usb_device_builder_incomplete() {
    // Builder without required fields should return None
    let builder = UsbDeviceBuilder::new();
    assert!(builder.build().is_none());

    // Builder with only product_name but no vendor_id should return None
    let mut builder2 = UsbDeviceBuilder::new();
    builder2.product_name = Some("Test Device".to_string());
    assert!(builder2.build().is_none());
}

#[test]
fn test_usb_device_builder_complete() {
    let mut builder = UsbDeviceBuilder::new();
    builder.product_name = Some("Test Device".to_string());
    builder.vendor_id = Some(0x1234);

    let device = builder.build().unwrap();
    assert_eq!(device.product_name, "Test Device");
    assert_eq!(device.vendor_id, 0x1234);
    assert_eq!(device.vendor_name, "Unknown");
    assert_eq!(device.serial_number, "Unknown");
}

#[test]
fn test_display_vendor_known_ids() {
    let mut builder = UsbDeviceBuilder::new();
    builder.product_name = Some("Test".to_string());
    builder.vendor_id = Some(0x0781); // SanDisk
    builder.vendor_name = Some("USB".to_string());
    let device = builder.build().unwrap();

    assert_eq!(device.display_vendor(), "SanDisk");
}

#[test]
fn test_display_vendor_unknown_id() {
    let mut builder = UsbDeviceBuilder::new();
    builder.product_name = Some("Test".to_string());
    builder.vendor_id = Some(0xFFFF); // Unknown
    builder.vendor_name = Some("My Vendor".to_string());
    let device = builder.build().unwrap();

    assert_eq!(device.display_vendor(), "My Vendor");
}

#[test]
fn test_display_vendor_empty_name() {
    let mut builder = UsbDeviceBuilder::new();
    builder.product_name = Some("Test".to_string());
    builder.vendor_id = Some(0xFFFF);
    builder.vendor_name = Some("USB".to_string()); // Generic "USB" name
    let device = builder.build().unwrap();

    assert_eq!(device.display_vendor(), "Unknown Vendor");
}
