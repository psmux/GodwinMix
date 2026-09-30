//! What every Unix answers the same way.

/// User plus system CPU of this process, every thread, from `getrusage`.
pub fn process_cpu_ns() -> Option<u64> {
    // SAFETY: getrusage writes into the struct we own and reads nothing else.
    let u = unsafe {
        let mut u: libc::rusage = std::mem::zeroed();
        if libc::getrusage(libc::RUSAGE_SELF, &mut u) != 0 {
            return None;
        }
        u
    };
    let ns = |t: libc::timeval| t.tv_sec as u64 * 1_000_000_000 + t.tv_usec as u64 * 1_000;
    Some(ns(u.ru_utime) + ns(u.ru_stime))
}
