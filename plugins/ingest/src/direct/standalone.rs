//! The direct host on its own, with no station: `gmx-ingest --direct
//! <table.json> [--seconds N]`. For measuring, and for the scale tests.
//!
//! The file is the table as the station hands it over, `{"direct": [...]}`
//! or the bare array. It is read again whenever it changes, so a test can
//! add and remove shows while the host runs. Every few seconds one line on
//! stderr sums up what runs: shows, inputs and outputs live, the rates in
//! and out, GOPs dropped, and the process's own CPU and memory.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use serde_json::{json, Value};

use super::{Emit, Host, Relay};
use crate::hub::Hub;

const EVERY: Duration = Duration::from_secs(5);

fn read(path: &str) -> Result<Value, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))?;
    let v: Value = serde_json::from_str(&text).map_err(|e| format!("{path} is not JSON: {e}"))?;
    Ok(if v.is_array() { json!({"direct": v}) } else { v })
}

fn modified(path: &str) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Run until `seconds` pass, or for ever. Answers the process's exit code.
pub fn run(path: &str, seconds: Option<u64>) -> i32 {
    let table = match read(path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    let latest = Arc::new(Mutex::new(Value::Null));
    let l = latest.clone();
    let emit: Emit = Arc::new(move |name, params| match name {
        "direct.stats" => *l.lock().unwrap_or_else(|e| e.into_inner()) = params,
        _ => eprintln!("{name} {params}"),
    });
    let relay: Relay = Arc::new(String::new);
    let host = Host::new(Hub::new(), emit, relay);
    for why in host.apply(&table) {
        eprintln!("{why}");
    }
    let (started, mut seen) = (Instant::now(), modified(path));
    let mut cpu = Cpu::now();
    while seconds.is_none_or(|s| started.elapsed() < Duration::from_secs(s)) {
        std::thread::sleep(EVERY);
        if modified(path) != seen {
            seen = modified(path);
            match read(path) {
                Ok(t) => host.apply(&t).iter().for_each(|w| eprintln!("{w}")),
                Err(e) => eprintln!("{e}"),
            }
        }
        let stats = latest.lock().unwrap_or_else(|e| e.into_inner()).clone();
        eprintln!("t={}s {} {}", started.elapsed().as_secs(), summary(&stats), cpu.since());
    }
    0
}

/// One line about every show at once.
pub fn summary(stats: &Value) -> String {
    let shows = stats["shows"].as_array().cloned().unwrap_or_default();
    let n = |v: &Value| v.as_u64().unwrap_or(0);
    let live_in = shows.iter().filter(|s| n(&s.get("input").and_then(|i| i.get("last_frame_ms")).cloned().unwrap_or(json!(u64::MAX))) < 3000).count();
    let outs: Vec<&Value> = shows.iter().flat_map(|s| s["outputs"].as_array().into_iter().flatten()).collect();
    let live_out = outs.iter().filter(|o| o["state"] == "live").count();
    let kbps_in: u64 = shows.iter().map(|s| n(&s["input"]["kbps"])).sum();
    let kbps_out: u64 = outs.iter().map(|o| n(&o["kbps"])).sum();
    let dropped: u64 = shows.iter().map(|s| n(&s["dropped_gops"])).sum();
    let cc: u64 = shows.iter().map(|s| n(&s["input"]["cc_errors"])).sum();
    format!(
        "shows={} inputs_live={live_in} outputs_live={live_out}/{} in_kbps={kbps_in} out_kbps={kbps_out} dropped_gops={dropped} cc_errors={cc}",
        shows.len(),
        outs.len()
    )
}

/// This process's CPU time and memory, from getrusage and the task info.
struct Cpu {
    at: Instant,
    used: Duration,
}

impl Cpu {
    fn now() -> Cpu {
        Cpu { at: Instant::now(), used: used() }
    }

    /// CPU as a share of one core since the last call, and resident memory.
    fn since(&mut self) -> String {
        let (now, at) = (used(), Instant::now());
        let share = (now.saturating_sub(self.used)).as_secs_f64() / at.duration_since(self.at).as_secs_f64().max(0.001);
        (self.used, self.at) = (now, at);
        format!("cpu={:.1}% rss_mb={:.1}", share * 100.0, rss_mb())
    }
}

#[cfg(unix)]
fn used() -> Duration {
    // SAFETY: getrusage writes one rusage into memory we own.
    let mut u: libc::rusage = unsafe { std::mem::zeroed() };
    unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut u) };
    let t = |tv: libc::timeval| Duration::from_secs(tv.tv_sec as u64) + Duration::from_micros(tv.tv_usec as u64);
    t(u.ru_utime) + t(u.ru_stime)
}

#[cfg(not(unix))]
fn used() -> Duration {
    Duration::ZERO
}

/// Resident memory now, from `ps`, which every Unix has.
fn rss_mb() -> f64 {
    let out = std::process::Command::new("ps").args(["-o", "rss=", "-p", &std::process::id().to_string()]).output();
    out.ok().and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse::<f64>().ok()).map_or(0.0, |kb| kb / 1024.0)
}
