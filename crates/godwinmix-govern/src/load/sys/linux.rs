//! Linux: `/proc` for the CPU and memory, sysfs for a GPU that says how busy
//! it is.
//!
//! AMD cards (and so the VA encoder on them) expose `gpu_busy_percent`.
//! NVIDIA exposes encoder load only through NVML, a library this crate will
//! not link for one number; NVENC work is counted from the calibrated share
//! each ticket declares instead, and its session limit from calibration.

use std::io::Read;
use std::sync::OnceLock;

/// Busy and total jiffies over every core, from the first line of
/// `/proc/stat`. I/O wait counts as idle: a core waiting on a disk can run
/// an encoder.
pub fn system_ticks() -> Option<(u64, u64)> {
    let mut buf = [0u8; 512];
    let n = std::fs::File::open("/proc/stat").ok()?.read(&mut buf).ok()?;
    let text = std::str::from_utf8(&buf[..n]).ok()?;
    parse_stat(text.lines().next()?)
}

pub(crate) fn parse_stat(line: &str) -> Option<(u64, u64)> {
    let mut it = line.split_whitespace();
    if it.next()? != "cpu" {
        return None;
    }
    // user nice system idle iowait irq softirq steal; guest is already
    // inside user, so it is not added twice.
    let v: Vec<u64> = it.take(8).filter_map(|x| x.parse().ok()).collect();
    if v.len() < 4 {
        return None;
    }
    let total: u64 = v.iter().sum();
    let idle = v[3] + v.get(4).copied().unwrap_or(0);
    Some((total - idle, total))
}

/// Total and available memory in MiB, from `/proc/meminfo`.
pub fn memory_mib() -> Option<(u64, u64)> {
    let text = std::fs::read_to_string("/proc/meminfo").ok()?;
    let field = |name: &str| -> Option<u64> {
        let line = text.lines().find(|l| l.starts_with(name))?;
        line.split_whitespace().nth(1)?.parse::<u64>().ok()
    };
    let total = field("MemTotal:")? / 1024;
    let avail = field("MemAvailable:").map(|k| k / 1024).unwrap_or(total);
    Some((total, avail))
}

/// `model name` on x86, `Model` on a Raspberry Pi, `Hardware` on older ARM.
pub fn cpu_model() -> String {
    let text = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    for key in ["model name", "Model", "Hardware", "cpu model"] {
        if let Some(v) = text
            .lines()
            .find(|l| l.split(':').next().is_some_and(|k| k.trim() == key))
            .and_then(|l| l.split_once(':'))
        {
            return v.1.trim().to_string();
        }
    }
    "unknown".into()
}

/// The GPUs that report how busy they are, found once.
fn busy_files() -> &'static [std::path::PathBuf] {
    static FILES: OnceLock<Vec<std::path::PathBuf>> = OnceLock::new();
    FILES.get_or_init(|| {
        let Ok(dir) = std::fs::read_dir("/sys/class/drm") else { return Vec::new() };
        let mut out: Vec<_> = dir
            .flatten()
            .map(|e| e.path().join("device/gpu_busy_percent"))
            .filter(|p| p.is_file())
            .collect();
        out.sort();
        out.dedup();
        out
    })
}

/// How busy the VA capable GPU is, in thousandths, the busiest card when
/// there are several.
pub fn device_busy() -> Vec<(String, u32)> {
    let busiest = busy_files()
        .iter()
        .filter_map(|p| std::fs::read_to_string(p).ok()?.trim().parse::<u32>().ok())
        .max();
    busiest.map(|pct| vec![("va".to_string(), pct.min(100) * 10)]).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_stat_line_parses_with_iowait_as_idle() {
        let (busy, total) = super::parse_stat("cpu  100 0 50 800 50 0 0 0 0 0").unwrap();
        assert_eq!(total, 1000);
        assert_eq!(busy, 150);
    }
}
