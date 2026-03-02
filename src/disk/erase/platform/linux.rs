//! Linux-specific storage access for disk wiping
//!
//! Uses O_DIRECT for direct I/O, bypassing the page cache.

use std::fs::{File, OpenOptions};
use std::io::{Read, Result, Seek, SeekFrom, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use super::StorageAccess;

/// O_DIRECT flag for direct I/O on Linux
const O_DIRECT: i32 = 0o40000;

/// Linux-specific storage access using O_DIRECT
pub struct LinuxStorageAccess {
    file: File,
}

impl LinuxStorageAccess {
    /// Open a device with direct I/O (O_DIRECT)
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(false)
            .truncate(false)
            .custom_flags(O_DIRECT)
            .open(path.as_ref())?;

        Ok(Self { file })
    }

    /// Open without O_DIRECT (fallback for filesystems that don't support it)
    pub fn open_buffered<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(false)
            .truncate(false)
            .open(path.as_ref())?;

        Ok(Self { file })
    }
}

impl StorageAccess for LinuxStorageAccess {
    fn seek(&mut self, position: u64) -> Result<u64> {
        self.file.seek(SeekFrom::Start(position))
    }

    fn read(&mut self, buffer: &mut [u8]) -> Result<usize> {
        self.file.read(buffer)
    }

    fn write(&mut self, data: &[u8]) -> Result<()> {
        self.file.write_all(data)
    }

    fn flush(&mut self) -> Result<()> {
        self.file.flush()?;
        // Also sync to ensure data hits the physical media
        self.file.sync_all()
    }
}

/// Open a device for direct I/O on Linux
pub fn open_device_direct<P: AsRef<Path>>(path: P) -> Result<Box<dyn StorageAccess>> {
    // Try with O_DIRECT first
    match LinuxStorageAccess::open(&path) {
        Ok(access) => Ok(Box::new(access)),
        Err(e) => {
            // Some filesystems (like tmpfs) don't support O_DIRECT
            // Fall back to buffered I/O with sync
            if e.raw_os_error() == Some(libc::EINVAL) {
                let access = LinuxStorageAccess::open_buffered(path)?;
                Ok(Box::new(access))
            } else {
                Err(e)
            }
        }
    }
}

/// Unmount a volume on Linux
///
/// Uses the `umount2` syscall with MNT_FORCE flag.
pub fn unmount(path: &Path) -> Result<()> {
    use std::ffi::CString;

    let path_str = path.to_string_lossy();
    let cpath = CString::new(path_str.as_bytes()).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("Invalid path: {}", e),
        )
    })?;

    // MNT_FORCE = 1, MNT_DETACH = 2
    // We use MNT_DETACH for lazy unmount which is more reliable
    const MNT_DETACH: libc::c_int = 2;

    let result = unsafe { libc::umount2(cpath.as_ptr(), MNT_DETACH) };

    match result {
        0 => Ok(()),
        _ => {
            let err = std::io::Error::last_os_error();
            // ENOENT or EINVAL means not mounted, which is fine
            if err.raw_os_error() == Some(libc::ENOENT) || err.raw_os_error() == Some(libc::EINVAL)
            {
                Ok(())
            } else {
                Err(err)
            }
        }
    }
}

/// Get the size of a block device using ioctl
///
/// This is an alternative to getting size from VolumeInfo.
#[allow(dead_code)]
pub fn get_block_device_size(file: &File) -> Result<u64> {
    use std::os::unix::io::AsRawFd;

    // BLKGETSIZE64 = _IOR(0x12, 114, size_t)
    const BLKGETSIZE64: libc::c_ulong = 0x80081272;

    let fd = file.as_raw_fd();
    let mut size: u64 = 0;

    unsafe {
        if libc::ioctl(fd, BLKGETSIZE64, &mut size) == -1 {
            return Err(std::io::Error::last_os_error());
        }
    }

    Ok(size)
}

#[cfg(test)]
#[path = "linux_test.rs"]
mod linux_test;
