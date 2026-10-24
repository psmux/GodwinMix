//! The box itself: name, cores, memory, how long it has been up and its
//! load average. Each is one system call or one small file read.

use godwinmix_protocol::machine::MachineStatus;

pub fn status(cpu_pct: Option<f64>) -> MachineStatus {
    let memory = godwinmix_govern::load::sys::memory_mib();
    let started = crate::control::methods::lifecycle::started_ms();
    MachineStatus {
        host_name: host_name(),
        cores: std::thread::available_parallelism().map(|n| n.get() as u32).unwrap_or(1),
        cpu_pct,
        memory_total_mib: memory.map(|m| m.0),
        memory_available_mib: memory.map(|m| m.1),
        uptime_secs: uptime_secs(),
        process_uptime_secs: super::now_ms().saturating_sub(started) / 1000,
        load_average: load_average(),
    }
}

#[cfg(unix)]
fn host_name() -> String {
    let mut buf = [0u8; 256];
    // SAFETY: gethostname writes at most the length given into the buffer.
    let rc = unsafe { libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) };
    if rc != 0 {
        return String::new();
    }
    let end = buf.iter().position(|b| *b == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..end]).into_owned()
}

#[cfg(not(unix))]
fn host_name() -> String {
    std::env::var("COMPUTERNAME").unwrap_or_default()
}

#[cfg(unix)]
fn load_average() -> Option<[f64; 3]> {
    let mut l = [0f64; 3];
    // SAFETY: getloadavg writes up to three doubles into the array.
    let n = unsafe { libc::getloadavg(l.as_mut_ptr(), 3) };
    (n == 3).then(|| l.map(|v| (v * 100.0).round() / 100.0))
}

#[cfg(not(unix))]
fn load_average() -> Option<[f64; 3]> {
    None
}

#[cfg(target_os = "linux")]
fn uptime_secs() -> Option<u64> {
    let text = std::fs::read_to_string("/proc/uptime").ok()?;
    text.split_whitespace().next()?.parse::<f64>().ok().map(|s| s as u64)
}

#[cfg(target_os = "macos")]
fn uptime_secs() -> Option<u64> {
    let mut tv: libc::timeval = libc::timeval { tv_sec: 0, tv_usec: 0 };
    let mut len = std::mem::size_of::<libc::timeval>();
    // SAFETY: kern.boottime is a timeval, and the length says so.
    let rc = unsafe { libc::sysctlbyname(c"kern.boottime".as_ptr(), &mut tv as *mut _ as *mut libc::c_void, &mut len, std::ptr::null_mut(), 0) };
    if rc != 0 || tv.tv_sec <= 0 {
        return None;
    }
    Some((super::now_ms() / 1000).saturating_sub(tv.tv_sec as u64))
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn uptime_secs() -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_box_has_a_name_cores_and_memory() {
        let s = super::status(Some(12.5));
        assert!(s.cores >= 1);
        assert_eq!(s.cpu_pct, Some(12.5));
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            assert!(!s.host_name.is_empty());
            assert!(s.memory_total_mib.unwrap_or(0) > 0);
            assert!(s.uptime_secs.unwrap_or(0) > 0);
        }
    }
}
