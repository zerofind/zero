//! Tests for macOS platform storage access

use super::*;
use std::io::Write;
use tempfile::NamedTempFile;

#[test]
fn test_open_regular_file() {
    // We can test with a regular file (F_NOCACHE still works)
    let mut temp = NamedTempFile::new().unwrap();
    temp.write_all(&[0u8; 4096]).unwrap();

    let result = MacOsStorageAccess::open(temp.path());
    assert!(result.is_ok());
}

#[test]
fn test_storage_access_operations() {
    let mut temp = NamedTempFile::new().unwrap();
    temp.write_all(&[0u8; 8192]).unwrap();

    let mut access = MacOsStorageAccess::open(temp.path()).unwrap();

    // Test seek
    assert_eq!(access.seek(4096).unwrap(), 4096);

    // Test write
    access.seek(0).unwrap();
    access.write(&[0xAA; 512]).unwrap();
    access.flush().unwrap();

    // Test read
    access.seek(0).unwrap();
    let mut buf = [0u8; 512];
    let n = access.read(&mut buf).unwrap();
    assert_eq!(n, 512);
    assert!(buf.iter().all(|&b| b == 0xAA));
}

#[test]
fn test_unmount_nonexistent() {
    // Unmounting a non-existent path should return Ok (ENOENT is ignored)
    let result = unmount(Path::new("/nonexistent/path/that/doesnt/exist"));
    assert!(result.is_ok());
}
