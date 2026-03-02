//! Tests for Linux platform storage access

use super::*;
use std::io::Write;
use tempfile::NamedTempFile;

#[test]
fn test_open_regular_file() {
    // Regular files may not support O_DIRECT on all filesystems
    // but the fallback should work
    let mut temp = NamedTempFile::new().unwrap();
    temp.write_all(&[0u8; 4096]).unwrap();

    let result = open_device_direct(temp.path());
    assert!(result.is_ok());
}

#[test]
fn test_storage_access_operations() {
    let mut temp = NamedTempFile::new().unwrap();
    temp.write_all(&[0u8; 8192]).unwrap();

    // Use buffered mode for test file (O_DIRECT often fails on tmpfs)
    let mut access = LinuxStorageAccess::open_buffered(temp.path()).unwrap();

    // Test seek
    assert_eq!(access.seek(4096).unwrap(), 4096);

    // Test write
    access.seek(0).unwrap();
    access.write(&[0xBB; 512]).unwrap();
    access.flush().unwrap();

    // Test read
    access.seek(0).unwrap();
    let mut buf = [0u8; 512];
    let n = access.read(&mut buf).unwrap();
    assert_eq!(n, 512);
    assert!(buf.iter().all(|&b| b == 0xBB));
}

#[test]
fn test_unmount_nonexistent() {
    // Unmounting a non-existent path should return Ok (ENOENT/EINVAL is ignored)
    let result = unmount(Path::new("/nonexistent/path/that/doesnt/exist"));
    assert!(result.is_ok());
}
