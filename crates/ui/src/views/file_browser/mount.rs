use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// A mounted filesystem entry.
#[derive(Clone)]
pub struct MountInfo {
    #[allow(dead_code)]
    pub mount_point: PathBuf,
    pub fstype: String,
}

/// Get all mounted filesystems, cached for the process lifetime.
pub fn all_mounts() -> &'static HashMap<PathBuf, MountInfo> {
    static MOUNTS: OnceLock<HashMap<PathBuf, MountInfo>> = OnceLock::new();
    MOUNTS.get_or_init(load_mounts)
}

/// Look up the filesystem type for a path if it's a mount point.
pub fn fstype_for_path(path: &Path) -> Option<&'static str> {
    all_mounts().get(path).map(|m| m.fstype.as_str())
}

#[cfg(target_os = "macos")]
fn load_mounts() -> HashMap<PathBuf, MountInfo> {
    use std::ffi::CStr;
    use std::os::raw::c_char;
    use std::{mem, ptr};

    let mut map = HashMap::new();

    // SAFETY: First call with null buffer returns the number of mounted filesystems.
    let count = unsafe { libc::getfsstat(ptr::null_mut(), 0, libc::MNT_NOWAIT) };
    if count <= 0 {
        return map;
    }

    let mut buf: Vec<libc::statfs> = Vec::with_capacity(count as usize);
    // SAFETY: statfs is a C struct — zero-initialization is valid.
    buf.resize_with(count as usize, || unsafe { mem::zeroed() });
    let bufsize = buf.len() * mem::size_of::<libc::statfs>();

    // SAFETY: buf is properly sized and aligned for `count` statfs entries.
    let actual =
        unsafe { libc::getfsstat(buf.as_mut_ptr(), bufsize as libc::c_int, libc::MNT_NOWAIT) };
    if actual <= 0 {
        return map;
    }
    buf.truncate(actual as usize);

    for mnt in &buf {
        // SAFETY: f_mntonname and f_fstypename are fixed-size char arrays in the
        // statfs struct, guaranteed null-terminated by the kernel (POSIX).
        let mount_point =
            unsafe { CStr::from_ptr(mnt.f_mntonname.as_ptr().cast::<c_char>()) }.to_string_lossy();
        let fstype = unsafe { CStr::from_ptr(mnt.f_fstypename.as_ptr().cast::<c_char>()) }
            .to_string_lossy()
            .into_owned();

        let dest = PathBuf::from(mount_point.as_ref());
        map.insert(
            dest.clone(),
            MountInfo {
                mount_point: dest,
                fstype,
            },
        );
    }

    map
}

#[cfg(not(target_os = "macos"))]
fn load_mounts() -> HashMap<PathBuf, MountInfo> {
    HashMap::new()
}
