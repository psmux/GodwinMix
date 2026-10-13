//! Keeping open files out of the child processes the mixer starts.
//!
//! On Windows a new process inherits every handle in its parent that is marked
//! inheritable, and Rust's `Command` asks for exactly that. GStreamer's
//! `filesrc` opens a clip with the C runtime and does not ask for the handle to
//! be private, so every clip a source is playing is inheritable. A plugin
//! process started while a clip is open (the camera and screen plugins start
//! with the show) then holds that clip too. Removing the source closes the
//! mixer's handle and leaves the plugin's, so the file cannot be renamed or
//! deleted until the plugin exits, which is the life of the show.
//!
//! Linux has the same problem by another road. Rust opens its own descriptors
//! close on exec, but `filesrc` does not, so a plugin started while a clip
//! plays keeps a descriptor on it: the clip's disk space is not given back
//! when it is deleted, until the plugin exits. CI found that with the test
//! written for Windows. On Unix the same call marks open regular files close
//! on exec.

/// Mark every open disk file in this process as not inheritable, so a child
/// started next does not take them with it.
///
/// Call it just before starting a process. Pipes and the console are left
/// alone, because they are how a child is given its standard streams. Returns
/// how many handles were changed. A handle opened by another thread in the
/// moments between this call and the start is not covered; there is no way to
/// close that without a handle list, which `std` offers only on nightly.
pub fn keep_files_from_children() -> usize {
    imp::keep_files_from_children()
}

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;

    type Handle = *mut c_void;
    const FILE_TYPE_DISK: u32 = 1;
    const HANDLE_FLAG_INHERIT: u32 = 1;

    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> Handle;
        fn GetProcessHandleCount(process: Handle, count: *mut u32) -> i32;
        fn GetFileType(file: Handle) -> u32;
        fn GetHandleInformation(object: Handle, flags: *mut u32) -> i32;
        fn SetHandleInformation(object: Handle, mask: u32, flags: u32) -> i32;
        fn GetStdHandle(which: u32) -> isize;
    }

    /// Handle values are multiples of four. The walk stops once it has met as
    /// many open handles as the process owns, or at this value, whichever
    /// comes first.
    const LAST_VALUE: usize = 1 << 22;

    pub fn keep_files_from_children() -> usize {
        // SAFETY: only queries and one flag write on handle values of this
        // process; a value that is not an open handle makes the call fail and
        // nothing else. The standard handles are skipped, since they are the
        // ones a child is meant to get.
        unsafe {
            let mut total = 0u32;
            if GetProcessHandleCount(GetCurrentProcess(), &mut total) == 0 {
                return 0;
            }
            let std_handles = [GetStdHandle(-10i32 as u32), GetStdHandle(-11i32 as u32), GetStdHandle(-12i32 as u32)];
            let (mut seen, mut changed) = (0u32, 0usize);
            let mut value = 4usize;
            while seen < total && value < LAST_VALUE {
                let handle = value as Handle;
                value += 4;
                let mut flags = 0u32;
                if GetHandleInformation(handle, &mut flags) == 0 {
                    continue;
                }
                seen += 1;
                if flags & HANDLE_FLAG_INHERIT == 0 || std_handles.contains(&(handle as isize)) {
                    continue;
                }
                if GetFileType(handle) == FILE_TYPE_DISK && SetHandleInformation(handle, HANDLE_FLAG_INHERIT, 0) != 0 {
                    changed += 1;
                }
            }
            changed
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::os::windows::io::AsRawHandle;

        fn inheritable(f: &std::fs::File) -> bool {
            let mut flags = 0u32;
            unsafe { GetHandleInformation(f.as_raw_handle(), &mut flags) };
            flags & HANDLE_FLAG_INHERIT != 0
        }

        #[test]
        fn an_open_file_stops_being_inheritable() {
            let path = std::env::temp_dir().join(format!("gmx-inherit-{}.bin", std::process::id()));
            std::fs::write(&path, b"x").unwrap();
            let file = std::fs::File::open(&path).unwrap();
            unsafe { SetHandleInformation(file.as_raw_handle(), HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT) };
            assert!(inheritable(&file), "the file was meant to start out inheritable");
            assert!(keep_files_from_children() >= 1);
            assert!(!inheritable(&file), "the file can still be taken by a child");
            drop(file);
            let _ = std::fs::remove_file(&path);
        }
    }
}

#[cfg(unix)]
mod imp {
    /// Where this process lists its own descriptors.
    const FD_DIR: &str = if cfg!(target_os = "linux") { "/proc/self/fd" } else { "/dev/fd" };

    pub fn keep_files_from_children() -> usize {
        // Read first, change after: the listing holds a descriptor of its own
        // while it is open.
        let fds: Vec<i32> = match std::fs::read_dir(FD_DIR) {
            Ok(dir) => dir.flatten().filter_map(|e| e.file_name().to_str()?.parse().ok()).filter(|fd| *fd > 2).collect(),
            Err(_) => return 0,
        };
        fds.into_iter().filter(|fd| close_on_exec(*fd)).count()
    }

    /// Mark one descriptor close on exec when it is a regular file that is not
    /// already. Pipes and sockets are left alone: a child is given those on
    /// purpose, as its standard streams or a frame socket.
    fn close_on_exec(fd: i32) -> bool {
        // SAFETY: fstat and fcntl on a descriptor number; one that has closed
        // since the listing makes the call fail and nothing else.
        unsafe {
            let mut st: libc::stat = std::mem::zeroed();
            if libc::fstat(fd, &mut st) != 0 || (st.st_mode & libc::S_IFMT) != libc::S_IFREG {
                return false;
            }
            let flags = libc::fcntl(fd, libc::F_GETFD);
            flags >= 0 && flags & libc::FD_CLOEXEC == 0 && libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC) == 0
        }
    }

    #[cfg(test)]
    mod tests {
        use std::os::fd::AsRawFd;

        #[test]
        fn an_open_file_is_closed_on_exec() {
            let path = std::env::temp_dir().join(format!("gmx-inherit-{}.bin", std::process::id()));
            std::fs::write(&path, b"x").unwrap();
            let file = std::fs::File::open(&path).unwrap();
            let fd = file.as_raw_fd();
            // SAFETY: flags on a descriptor this test owns.
            let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
            unsafe { libc::fcntl(fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC) };
            assert!(super::keep_files_from_children() >= 1);
            let after = unsafe { libc::fcntl(fd, libc::F_GETFD) };
            assert_ne!(after & libc::FD_CLOEXEC, 0, "the file would still go to a child");
            drop(file);
            let _ = std::fs::remove_file(&path);
        }
    }
}

#[cfg(not(any(windows, unix)))]
mod imp {
    pub fn keep_files_from_children() -> usize {
        0
    }
}
