//! Free space on the filesystem holding a path.
//!
//! One plugin needs this (`file-record`, to go degraded before the disk fills
//! rather than after), and it is the same two system calls the core's own
//! doctor makes in `crates/godwinmix-core/src/observe/doctor.rs`. It is
//! repeated here rather than exported from the core because a plugin links the
//! SDK and not the engine, which is the whole point of the sidecar contract.

use std::path::Path;

/// Free bytes on the filesystem holding `dir`, or `None` where we cannot ask.
///
/// `None` is not an error. A plugin that cannot read the free space records
/// anyway and says in `health` that it is recording blind, because refusing to
/// record because a statistic is missing would be the worse failure.
#[cfg(unix)]
pub fn free_bytes(dir: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(dir.as_os_str().as_bytes()).ok()?;
    // SAFETY: `statvfs` fills a struct we own from a path that outlives the
    // call. A zeroed struct is a valid starting value for it.
    unsafe {
        let mut s: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(c.as_ptr(), &mut s) != 0 {
            return None;
        }
        // `f_frsize` is the fragment size and is what `f_bavail` counts. Some
        // platforms leave it zero, and then `f_bsize` is the answer.
        let unit = if s.f_frsize > 0 {
            s.f_frsize as u64
        } else {
            s.f_bsize as u64
        };
        Some(unit.saturating_mul(s.f_bavail as u64))
    }
}

#[cfg(windows)]
pub fn free_bytes(dir: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" {
        fn GetDiskFreeSpaceExW(
            directory: *const u16,
            free_to_caller: *mut u64,
            total: *mut u64,
            total_free: *mut u64,
        ) -> i32;
    }
    let wide: Vec<u16> = dir
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut free = 0u64;
    // SAFETY: the path is NUL terminated and outlives the call; the three out
    // parameters are stack locals we own.
    unsafe {
        let ok = GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut free,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );
        (ok != 0).then_some(free)
    }
}

#[cfg(not(any(unix, windows)))]
pub fn free_bytes(_dir: &Path) -> Option<u64> {
    None
}

/// Free bytes as a person would say it: "3.4 GB".
pub fn human(bytes: u64) -> String {
    const GB: f64 = 1_000_000_000.0;
    const MB: f64 = 1_000_000.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.1} GB", b / GB)
    } else if b >= MB {
        format!("{:.0} MB", b / MB)
    } else {
        format!("{bytes} bytes")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_temporary_directory_has_some_space_on_it() {
        let free = free_bytes(&std::env::temp_dir());
        // Every platform this ships on answers. If one does not, the plugin
        // still records, so this is an assertion about the platforms and not
        // about the plugin.
        assert!(
            free.is_some_and(|b| b > 0),
            "no reading for the temp directory"
        );
    }

    #[test]
    fn a_path_that_is_not_there_has_no_reading_rather_than_a_wrong_one() {
        assert_eq!(free_bytes(Path::new("/no/such/place/at/all")), None);
    }

    #[test]
    fn sizes_read_the_way_a_person_says_them() {
        assert_eq!(human(4_200_000_000), "4.2 GB");
        assert_eq!(human(512_000_000), "512 MB");
        assert_eq!(human(900), "900 bytes");
    }
}
