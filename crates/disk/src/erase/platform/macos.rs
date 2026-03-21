//! macOS-specific storage access for disk wiping
//!
//! Uses the raw device (/dev/rdisk*) with `F_NOCACHE` for direct I/O.

use std::fs::{File, OpenOptions};
use std::io::{Read, Result, Seek, SeekFrom, Write};
use std::os::unix::io::AsRawFd;
use std::path::Path;

use super::StorageAccess;

/// macOS-specific storage access using `F_NOCACHE`
pub struct MacOsStorageAccess {
    file: File,
}

impl MacOsStorageAccess {
    /// Open a device with direct I/O (`F_NOCACHE`)
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(false)
            .truncate(false)
            .open(path.as_ref())?;

        // SAFETY: fcntl(F_NOCACHE) is a safe ioctl on a valid fd (just opened above).
        // Disables page cache so writes go directly to device.
        unsafe {
            let fd = file.as_raw_fd();
            if libc::fcntl(fd, libc::F_NOCACHE, 1) == -1 {
                return Err(std::io::Error::last_os_error());
            }
        }

        Ok(Self { file })
    }
}

impl StorageAccess for MacOsStorageAccess {
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
        // For raw block devices on macOS, sync_all() fails with ENOTTY
        // because it tries to use fsync which doesn't work on /dev/rdisk*
        //
        // However, since we're using F_NOCACHE, writes go directly to the device
        // without buffering, so we just need to flush any stdio buffers.
        // The kernel doesn't buffer raw device writes.
        self.file.flush()?;

        // Try sync_all but ignore ENOTTY (inappropriate ioctl for device)
        // This error is expected for raw block devices
        match self.file.sync_all() {
            Ok(()) => Ok(()),
            Err(e) if e.raw_os_error() == Some(libc::ENOTTY) => {
                // Expected for raw devices, data is already on disk due to F_NOCACHE
                Ok(())
            }
            Err(e) => Err(e),
        }
    }
}

/// Open a device for direct I/O on macOS
pub fn open_device_direct<P: AsRef<Path>>(path: P) -> Result<Box<dyn StorageAccess>> {
    let access = MacOsStorageAccess::open(path)?;
    Ok(Box::new(access))
}

/// Unmount a volume on macOS
///
/// Uses the `unmount` syscall with `MNT_FORCE` flag.
pub fn unmount(path: &Path) -> Result<()> {
    use std::ffi::CString;

    let path_str = path.to_string_lossy();
    let cpath = CString::new(path_str.as_bytes()).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("Invalid path: {e}"),
        )
    })?;

    // SAFETY: cpath is a valid null-terminated C string from CString::new.
    let result = unsafe { libc::unmount(cpath.as_ptr(), libc::MNT_FORCE) };

    if result == 0 {
        Ok(())
    } else {
        let err = std::io::Error::last_os_error();
        // ENOENT means not mounted, which is fine
        if err.raw_os_error() == Some(libc::ENOENT) {
            Ok(())
        } else {
            Err(err)
        }
    }
}

/// Get the size of a block device using ioctl
///
/// This is an alternative to getting size from `VolumeInfo`.
#[allow(dead_code)]
pub fn get_block_device_size(file: &File) -> Result<u64> {
    // DKIOCGETBLOCKSIZE = _IOR('d', 24, u32)
    // DKIOCGETBLOCKCOUNT = _IOR('d', 25, u64)
    const DKIOCGETBLOCKSIZE: libc::c_ulong = 0x40046418;
    const DKIOCGETBLOCKCOUNT: libc::c_ulong = 0x40086419;

    let fd = file.as_raw_fd();

    let mut block_size: u32 = 0;
    let mut block_count: u64 = 0;

    // SAFETY: fd is a valid file descriptor (from File). ioctl writes into
    // the provided pointers; we check for -1 (error) before using the values.
    unsafe {
        if libc::ioctl(fd, DKIOCGETBLOCKSIZE, &mut block_size) == -1 {
            return Err(std::io::Error::last_os_error());
        }
        if libc::ioctl(fd, DKIOCGETBLOCKCOUNT, &mut block_count) == -1 {
            return Err(std::io::Error::last_os_error());
        }
    }

    Ok(u64::from(block_size).saturating_mul(block_count))
}

#[cfg(test)]
#[path = "macos_test.rs"]
mod macos_test;
