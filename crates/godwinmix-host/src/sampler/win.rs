//! Windows: CPU time and resident memory for a pid, from `GetProcessTimes`
//! and `K32GetProcessMemoryInfo`, declared here rather than pulled in with a
//! bindings crate for four calls. No process is started to read them.

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct FileTime {
    low: u32,
    high: u32,
}

#[repr(C)]
#[derive(Default)]
struct MemoryCounters {
    cb: u32,
    page_fault_count: u32,
    peak_working_set_size: usize,
    working_set_size: usize,
    quota_peak_paged_pool_usage: usize,
    quota_paged_pool_usage: usize,
    quota_peak_non_paged_pool_usage: usize,
    quota_non_paged_pool_usage: usize,
    pagefile_usage: usize,
    peak_pagefile_usage: usize,
}

#[link(name = "kernel32")]
extern "system" {
    fn OpenProcess(access: u32, inherit: i32, pid: u32) -> isize;
    fn CloseHandle(handle: isize) -> i32;
    fn GetExitCodeProcess(handle: isize, code: *mut u32) -> i32;
    fn GetProcessTimes(
        process: isize,
        creation: *mut FileTime,
        exit: *mut FileTime,
        kernel: *mut FileTime,
        user: *mut FileTime,
    ) -> i32;
    fn K32GetProcessMemoryInfo(process: isize, counters: *mut MemoryCounters, cb: u32) -> i32;
}

const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
const PROCESS_VM_READ: u32 = 0x0010;
const STILL_ACTIVE: u32 = 259;

/// Kernel plus user time in 100 ns ticks, and the resident size in bytes,
/// for a process that is still running. None for one that has gone or that
/// this process may not look at.
pub fn read(pid: u32) -> Option<(u64, u64)> {
    // SAFETY: a plain handle request; a zero answer is checked.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ, 0, pid) };
    if handle == 0 {
        return None;
    }
    let out = read_open(handle);
    // SAFETY: the handle was opened above and is closed once.
    unsafe { CloseHandle(handle) };
    out
}

fn read_open(handle: isize) -> Option<(u64, u64)> {
    let mut code = 0u32;
    // SAFETY: an out pointer to a u32 we own.
    if unsafe { GetExitCodeProcess(handle, &mut code) } == 0 || code != STILL_ACTIVE {
        return None;
    }
    let (mut creation, mut exit, mut kernel, mut user) = Default::default();
    // SAFETY: four out pointers to structs we own.
    if unsafe { GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user) } == 0 {
        return None;
    }
    let ticks = |t: FileTime| (u64::from(t.high) << 32) | u64::from(t.low);
    let mut counters = MemoryCounters { cb: std::mem::size_of::<MemoryCounters>() as u32, ..Default::default() };
    // SAFETY: an out pointer to a struct we own, with its size in `cb`.
    if unsafe { K32GetProcessMemoryInfo(handle, &mut counters, counters.cb) } == 0 {
        return None;
    }
    Some((ticks(kernel) + ticks(user), counters.working_set_size as u64))
}

/// The bytes a process has committed for itself, which Task Manager calls its
/// commit size and Performance Monitor its private bytes. Unlike the working
/// set this does not shrink when Windows trims a process under pressure, so
/// it is the number that says a process is growing.
pub fn private_bytes(pid: u32) -> Option<u64> {
    // SAFETY: a plain handle request; a zero answer is checked.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ, 0, pid) };
    if handle == 0 {
        return None;
    }
    let mut counters = MemoryCounters { cb: std::mem::size_of::<MemoryCounters>() as u32, ..Default::default() };
    // SAFETY: an out pointer to a struct we own, with its size in `cb`.
    let ok = unsafe { K32GetProcessMemoryInfo(handle, &mut counters, counters.cb) } != 0;
    // SAFETY: the handle was opened above and is closed once.
    unsafe { CloseHandle(handle) };
    ok.then_some(counters.pagefile_usage as u64)
}
