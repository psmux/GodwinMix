//! Windows: every process this one starts ends when this one does.
//!
//! Linux and macOS end nothing either, but there a show watches its link and
//! a plugin reads its stdin, and both go when the parent goes. Windows left
//! them running: a plugin from a mixer the desktop app had quit was still
//! holding its port ten seconds later, and a calibration held the GPU. A job
//! object that kills on close is the Windows way to say "these are mine". This
//! process joins one at start, every child it starts afterwards is in it, and
//! when the last handle closes (this process exiting, cleanly or not) Windows
//! ends them all. A show does the same for its own plugins.

#[cfg(windows)]
mod imp {
    #[repr(C)]
    #[derive(Default)]
    struct BasicLimits {
        per_process_user_time_limit: i64,
        per_job_user_time_limit: i64,
        limit_flags: u32,
        minimum_working_set_size: usize,
        maximum_working_set_size: usize,
        active_process_limit: u32,
        affinity: usize,
        priority_class: u32,
        scheduling_class: u32,
    }

    #[repr(C)]
    #[derive(Default)]
    struct IoCounters {
        read_operations: u64,
        write_operations: u64,
        other_operations: u64,
        read_bytes: u64,
        write_bytes: u64,
        other_bytes: u64,
    }

    #[repr(C)]
    #[derive(Default)]
    struct ExtendedLimits {
        basic: BasicLimits,
        io: IoCounters,
        process_memory_limit: usize,
        job_memory_limit: usize,
        peak_process_memory_used: usize,
        peak_job_memory_used: usize,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateJobObjectW(attributes: *const core::ffi::c_void, name: *const u16) -> isize;
        fn SetInformationJobObject(job: isize, class: i32, info: *const core::ffi::c_void, len: u32) -> i32;
        fn AssignProcessToJobObject(job: isize, process: isize) -> i32;
        fn GetCurrentProcess() -> isize;
    }

    const EXTENDED_LIMIT_INFORMATION: i32 = 9;
    const KILL_ON_JOB_CLOSE: u32 = 0x2000;

    /// False, with nothing changed, if Windows would not do it; the mixer runs
    /// either way and only loses the clean up.
    pub fn contain_children() -> bool {
        // SAFETY: an unnamed job with default security; zero is checked.
        let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if job == 0 {
            return false;
        }
        let mut limits = ExtendedLimits::default();
        limits.basic.limit_flags = KILL_ON_JOB_CLOSE;
        let len = std::mem::size_of::<ExtendedLimits>() as u32;
        // SAFETY: a pointer to a struct we own, with its size.
        let set = unsafe { SetInformationJobObject(job, EXTENDED_LIMIT_INFORMATION, (&limits as *const ExtendedLimits).cast(), len) };
        // SAFETY: this process's pseudo handle and the job made above.
        // The job handle is never closed: it closes when this process ends,
        // which is the moment it exists for.
        set != 0 && unsafe { AssignProcessToJobObject(job, GetCurrentProcess()) } != 0
    }
}

#[cfg(windows)]
pub use imp::contain_children;

/// Nothing to do: see the module comment.
#[cfg(not(windows))]
pub fn contain_children() -> bool {
    true
}

#[cfg(all(test, windows))]
mod tests {
    #[test]
    fn this_process_can_hold_its_children() {
        assert!(super::contain_children());
    }
}
