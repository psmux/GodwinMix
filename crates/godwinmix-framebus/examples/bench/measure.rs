//! CPU time, memory and the measuring window every process agrees on.

use std::time::Duration;

use godwinmix_framebus::monotonic_ns;

/// User plus system CPU this process has used, in nanoseconds, and its peak
/// resident memory in bytes.
pub fn usage() -> (u64, u64) {
    // SAFETY: getrusage fills the struct we hand it.
    let mut ru: libc::rusage = unsafe { std::mem::zeroed() };
    unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut ru) };
    let ns = |t: libc::timeval| t.tv_sec as u64 * 1_000_000_000 + t.tv_usec as u64 * 1000;
    // Linux reports kilobytes, macOS bytes.
    let rss = if cfg!(target_os = "linux") {
        ru.ru_maxrss as u64 * 1024
    } else {
        ru.ru_maxrss as u64
    };
    (ns(ru.ru_utime) + ns(ru.ru_stime), rss)
}

/// Sleep until monotonic time `at`.
pub fn sleep_until(at: u64) {
    let now = monotonic_ns();
    if at > now {
        std::thread::sleep(Duration::from_nanos(at - now));
    }
}

/// CPU used between `t0` and `t1` as a percentage of one core. Blocks until
/// `t1`.
pub fn cpu_over(t0: u64, t1: u64) -> (f64, u64) {
    sleep_until(t0);
    let (c0, _) = usage();
    sleep_until(t1);
    let (c1, rss) = usage();
    ((c1 - c0) as f64 * 100.0 / (t1 - t0) as f64, rss)
}

/// The value at quantile `q` of `v`, which is sorted here.
pub fn quantile(v: &mut [u64], q: f64) -> u64 {
    if v.is_empty() {
        return 0;
    }
    v.sort_unstable();
    v[((v.len() - 1) as f64 * q).round() as usize]
}

/// One `BENCH k=v ...` line for the parent.
pub fn report(pairs: &[(&str, String)]) {
    let body: Vec<String> = pairs.iter().map(|(k, v)| format!("{k}={v}")).collect();
    println!("BENCH {}", body.join(" "));
}
