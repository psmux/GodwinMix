//! One clock both sides of the bus agree on.

/// Nanoseconds on the system's monotonic clock. Unlike `Instant`, the same
/// value means the same moment in every process on the machine, which is
/// what end to end latency across processes needs.
#[cfg(unix)]
pub fn monotonic_ns() -> u64 {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: a valid pointer to a timespec; CLOCK_MONOTONIC always exists.
    unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
    ts.tv_sec as u64 * 1_000_000_000 + ts.tv_nsec as u64
}

/// Windows: the process start relative clock is all std offers without a
/// system call crate, and the bus is in process only there for now.
#[cfg(not(unix))]
pub fn monotonic_ns() -> u64 {
    use std::sync::OnceLock;
    use std::time::Instant;
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_nanos() as u64
}
