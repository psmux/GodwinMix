//! `nvidia-smi`, for a machine where NVML cannot be opened: one query a
//! second, every GPU on one line each, parsed here. Slower than NVML (a
//! process start each time, tens of milliseconds) and the same numbers.

use super::gpu::Read;
use godwinmix_protocol::machine::GpuSample;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// The fields asked for, in the order a line answers them.
pub const FIELDS: &[&str] = &[
    "index", "name", "driver_version", "utilization.gpu", "utilization.encoder", "utilization.decoder",
    "memory.used", "memory.total", "encoder.stats.sessionCount", "encoder.stats.averageFps", "temperature.gpu",
    "power.draw", "power.limit", "clocks.gr", "clocks.mem", "clocks.video", "fan.speed",
];

/// A query that has not answered in this long is killed.
const LIMIT: Duration = Duration::from_secs(3);

/// Run the query once. `Err` says in one sentence why there is no answer.
pub fn query() -> Result<Vec<Read>, String> {
    let mut child = Command::new("nvidia-smi")
        .arg(format!("--query-gpu={}", FIELDS.join(",")))
        .arg("--format=csv,noheader,nounits")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "No NVIDIA GPU here: neither NVML nor nvidia-smi answered.".to_string())?;
    let until = Instant::now() + LIMIT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < until => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("nvidia-smi did not answer within three seconds.".into());
            }
        }
    }
    let out = child.wait_with_output().map_err(|e| format!("nvidia-smi could not be read: {e}."))?;
    if !out.status.success() {
        return Err("nvidia-smi found no GPU it could read (is the driver loaded?).".into());
    }
    let reads = parse(&String::from_utf8_lossy(&out.stdout), super::now_ms());
    if reads.is_empty() {
        return Err("nvidia-smi listed no GPU.".into());
    }
    Ok(reads)
}

/// Every line of a `--format=csv,noheader,nounits` answer to [`FIELDS`].
/// A field the card does not have (`[N/A]`, `[Not Supported]`) is left out.
pub fn parse(text: &str, at_ms: u64) -> Vec<Read> {
    text.lines().filter_map(|line| line_of(line, at_ms)).collect()
}

fn line_of(line: &str, at_ms: u64) -> Option<Read> {
    let f: Vec<&str> = line.split(',').map(str::trim).collect();
    if f.len() < FIELDS.len() {
        return None;
    }
    let num = |i: usize| f[i].parse::<f64>().ok().filter(|v| v.is_finite() && *v >= 0.0);
    let int = |i: usize| num(i).map(|v| v.round() as u32);
    let mib = |i: usize| num(i).map(|v| v.round() as u64);
    let watts = |i: usize| num(i).map(|v| (v * 10.0).round() / 10.0);
    let sample = GpuSample {
        at_ms,
        gpu_pct: int(3),
        encoder_pct: int(4),
        decoder_pct: int(5),
        memory_used_mib: mib(6),
        memory_total_mib: mib(7),
        encoder_sessions: int(8),
        encoder_fps: int(9),
        temperature_c: int(10),
        power_w: watts(11),
        power_limit_w: watts(12),
        graphics_mhz: int(13),
        memory_mhz: int(14),
        video_mhz: int(15),
        fan_pct: int(16),
    };
    let driver = Some(f[2].to_string()).filter(|d| !d.starts_with('['));
    Some(Read { index: int(0)?, name: f[1].to_string(), driver, sample })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The answer of an RTX 3060 carrying fifty channels, and a second
    /// card with no fan and no power reading, as nvidia-smi writes them.
    const CAPTURED: &str = include_str!("../../tests/fixtures/nvidia-smi.csv");

    #[test]
    fn a_captured_answer_reads_every_field_it_has() {
        let reads = parse(CAPTURED, 7);
        assert_eq!(reads.len(), 2);
        let a = &reads[0];
        assert_eq!((a.index, a.name.as_str(), a.driver.as_deref()), (0, "NVIDIA GeForce RTX 3060", Some("550.144.03")));
        let s = &a.sample;
        assert_eq!((s.at_ms, s.gpu_pct, s.encoder_pct, s.decoder_pct), (7, Some(38), Some(91), Some(44)));
        assert_eq!((s.memory_used_mib, s.memory_total_mib), (Some(6241), Some(12288)));
        assert_eq!((s.encoder_sessions, s.encoder_fps, s.temperature_c), (Some(51), Some(30), Some(63)));
        assert_eq!((s.power_w, s.power_limit_w), (Some(101.4), Some(170.0)));
        assert_eq!((s.graphics_mhz, s.memory_mhz, s.video_mhz, s.fan_pct), (Some(1807), Some(7300), Some(1612), Some(47)));
    }

    #[test]
    fn a_field_the_card_does_not_have_is_left_out() {
        let b = &parse(CAPTURED, 0)[1];
        assert_eq!(b.index, 1);
        assert_eq!((b.sample.fan_pct, b.sample.power_w, b.sample.encoder_fps), (None, None, Some(0)));
        assert_eq!(b.sample.encoder_pct, Some(0));
    }

    #[test]
    fn a_short_or_garbled_line_is_skipped() {
        assert!(parse("No devices were found\n", 0).is_empty());
        assert!(parse("0, RTX, 550\n", 0).is_empty());
        assert!(parse("", 0).is_empty());
    }
}
