//! What the core costs while the viewers run, read with `ps`.

use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

fn ps(field: &str, pid: u32) -> String {
    Command::new("ps")
        .args(["-o", field, "-p", &pid.to_string()])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

/// `[[dd-]hh:]mm:ss.ss` as seconds.
pub fn parse_time(text: &str) -> f64 {
    let (days, clock) = text.trim().split_once('-').unwrap_or(("0", text.trim()));
    let days: f64 = days.parse().unwrap_or(0.0);
    days * 86400.0 + clock.split(':').fold(0.0, |total, part| total * 60.0 + part.parse::<f64>().unwrap_or(0.0))
}

/// The process's CPU time so far.
pub fn cpu_seconds(pid: u32) -> f64 {
    parse_time(&ps("time=", pid))
}

/// Resident memory in MiB once a second, on a thread, until `stop`.
pub fn sample_rss(pid: u32, stop: Arc<AtomicBool>) -> std::thread::JoinHandle<Vec<f64>> {
    std::thread::spawn(move || {
        let mut out = Vec::new();
        while !stop.load(Ordering::Relaxed) {
            if let Ok(kib) = ps("rss=", pid).parse::<f64>() {
                out.push(kib / 1024.0);
            }
            std::thread::sleep(Duration::from_secs(1));
        }
        out
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn ps_times_in_every_shape() {
        assert_eq!(super::parse_time("01:02.50"), 62.5);
        assert_eq!(super::parse_time("1:00:00.00"), 3600.0);
        assert_eq!(super::parse_time("2-00:00:01"), 2.0 * 86400.0 + 1.0);
    }
}
