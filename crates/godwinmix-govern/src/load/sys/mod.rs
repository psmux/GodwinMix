//! What the operating system says about load, one small file per platform.
//!
//! Every function is one or two system calls or one small `/proc` read, so a
//! sample costs microseconds. A platform that cannot answer returns `None`
//! and the governor falls back to counting what it granted.

#[cfg(unix)]
mod unix;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::*;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::*;

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
mod other;
#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
pub use other::*;

/// CPU time this process has used, all threads, in nanoseconds.
pub fn process_cpu_ns() -> Option<u64> {
    #[cfg(unix)]
    {
        unix::process_cpu_ns()
    }
    #[cfg(windows)]
    {
        windows::process_cpu_ns_win()
    }
    #[cfg(not(any(unix, windows)))]
    {
        None
    }
}
