//! Windows: `GetSystemTimes`, `GetProcessTimes` and `GlobalMemoryStatusEx`,
//! declared here rather than pulled in with a bindings crate for three calls.
//!
//! GPU encoder load lives behind performance counters (the "GPU Engine"
//! set), which cost a query handle and a string parse a sample; not read
//! yet. NVENC, AMF and Quick Sync work is counted from the calibrated share
//! each ticket declares instead.

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct FileTime {
    low: u32,
    high: u32,
}

impl FileTime {
    /// In 100 ns units.
    fn ticks(self) -> u64 {
        (u64::from(self.high) << 32) | u64::from(self.low)
    }
}

#[repr(C)]
struct MemoryStatusEx {
    length: u32,
    memory_load: u32,
    total_phys: u64,
    avail_phys: u64,
    total_page_file: u64,
    avail_page_file: u64,
    total_virtual: u64,
    avail_virtual: u64,
    avail_extended_virtual: u64,
}

#[link(name = "kernel32")]
extern "system" {
    fn GetSystemTimes(idle: *mut FileTime, kernel: *mut FileTime, user: *mut FileTime) -> i32;
    fn GetCurrentProcess() -> isize;
    fn GetProcessTimes(
        process: isize,
        creation: *mut FileTime,
        exit: *mut FileTime,
        kernel: *mut FileTime,
        user: *mut FileTime,
    ) -> i32;
    fn GlobalMemoryStatusEx(status: *mut MemoryStatusEx) -> i32;
}

/// Busy and total 100 ns ticks over every core. Kernel time includes idle
/// time on Windows, so busy is kernel plus user minus idle.
pub fn system_ticks() -> Option<(u64, u64)> {
    let (mut idle, mut kernel, mut user) = (FileTime::default(), FileTime::default(), FileTime::default());
    // SAFETY: three out pointers to structs we own.
    if unsafe { GetSystemTimes(&mut idle, &mut kernel, &mut user) } == 0 {
        return None;
    }
    let total = kernel.ticks() + user.ticks();
    Some((total.saturating_sub(idle.ticks()), total))
}

pub fn process_cpu_ns_win() -> Option<u64> {
    let mut t = [FileTime::default(); 4];
    let [a, b, c, d] = &mut t;
    // SAFETY: the pseudo handle for this process and four out pointers.
    if unsafe { GetProcessTimes(GetCurrentProcess(), a, b, c, d) } == 0 {
        return None;
    }
    Some((t[2].ticks() + t[3].ticks()) * 100)
}

pub fn memory_mib() -> Option<(u64, u64)> {
    // SAFETY: zeroed is a valid value, and the length field is set before
    // the call as the API requires.
    let mut s: MemoryStatusEx = unsafe { std::mem::zeroed() };
    s.length = std::mem::size_of::<MemoryStatusEx>() as u32;
    if unsafe { GlobalMemoryStatusEx(&mut s) } == 0 {
        return None;
    }
    Some((s.total_phys / 1_048_576, s.avail_phys / 1_048_576))
}

pub fn cpu_model() -> String {
    std::env::var("PROCESSOR_IDENTIFIER").unwrap_or_else(|_| "unknown".into())
}

pub fn device_busy() -> Vec<(String, u32)> {
    Vec::new()
}
