//! Stand in for the core and run udp/source for a while, then report.
//!
//! Starts the staged plugin binary the way the core does (the GMX_
//! environment, the handshake on stdin and stderr), starts the source, and
//! every few seconds asks it for `stats` and reads its CPU and memory with
//! `ps`. The MPEG-TS it hands over on stdout goes to a file or to nowhere.
//!
//! ```sh
//! cargo run -p gmx-udp --example drive -- --uri udp://@239.1.1.1:19471 --seconds 60
//! cargo run -p gmx-udp --example drive -- --uri udp://@239.1.1.1:19471 --program 2 --out got.ts
//! ```
//!
//! Prints one line per sample and a JSON summary last.

#[path = "drive/plugin.rs"]
mod plugin;

use plugin::Plugin;
use serde_json::{json, Value};
use std::process::ExitCode;
use std::time::{Duration, Instant};

struct Args {
    uri: String,
    program: u64,
    seconds: u64,
    every: u64,
    out: Option<String>,
    binary: String,
}

fn parse(mut it: impl Iterator<Item = String>) -> Result<Args, String> {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/bin/gmx-udp").to_string();
    let mut a = Args { uri: String::new(), program: 0, seconds: 60, every: 5, out: None, binary: root };
    while let Some(flag) = it.next() {
        let value = it.next().ok_or(format!("{flag} needs a value"))?;
        let num = || value.parse::<u64>().map_err(|_| format!("{flag} takes a number, not {value}"));
        match flag.as_str() {
            "--uri" => a.uri = value.clone(),
            "--program" => a.program = num()?,
            "--seconds" => a.seconds = num()?,
            "--every" => a.every = num()?,
            "--out" => a.out = Some(value.clone()),
            "--binary" => a.binary = value.clone(),
            other => return Err(format!("drive does not know {other}")),
        }
    }
    if a.uri.is_empty() {
        return Err("usage: drive --uri URI [--program n] [--seconds s] [--every s] [--out file] [--binary path]".into());
    }
    Ok(a)
}

/// `%CPU` and RSS in MB, from `ps`.
fn cpu_rss(pid: u32) -> (f64, u64) {
    let out = std::process::Command::new("ps").args(["-o", "%cpu=,rss=", "-p", &pid.to_string()]).output();
    let text = out.map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
    match text.split_whitespace().collect::<Vec<_>>()[..] {
        [cpu, rss] => (cpu.parse().unwrap_or(0.0), rss.parse::<u64>().unwrap_or(0) / 1024),
        _ => (0.0, 0),
    }
}

fn run() -> Result<(), String> {
    let a = parse(std::env::args().skip(1))?;
    let mut p = Plugin::spawn(&a.binary, a.out.as_deref())?;
    p.handshake(json!({ "uri": a.uri, "program": a.program }))?;
    let (mut samples, t0) = (Vec::new(), Instant::now());
    while t0.elapsed() < Duration::from_secs(a.seconds) {
        std::thread::sleep(Duration::from_secs(a.every));
        let stats = p.call("stats", json!({}))["result"]["stats"].clone();
        let health = p.call("health", json!({}))["result"].clone();
        let (cpu, rss) = cpu_rss(p.pid());
        let t = t0.elapsed().as_secs_f64().round() as u64;
        println!("{t:>4}s cpu {cpu:5.1}% rss {rss} MB  {}: {}", show(&health["state"]), show(&health["detail"]));
        let mut s = json!({ "t": t, "cpu": cpu, "rss_mb": rss });
        if let (Some(m), Some(st)) = (s.as_object_mut(), stats.as_object()) {
            m.extend(st.clone());
        }
        samples.push(s);
    }
    let programs = p.call("programs", json!({}))["result"].clone();
    p.call("stop", json!({}));
    p.call("shutdown", json!({ "reason": "drive is done" }));
    p.wait();
    println!("{}", summary(&a, &samples, programs));
    Ok(())
}

/// Python's `str()` of a JSON value, which is how the line always read.
fn show(v: &Value) -> String {
    match v {
        Value::Null => "None".into(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn summary(a: &Args, samples: &[Value], programs: Value) -> String {
    let last = samples.last().cloned().unwrap_or(json!({}));
    let mean = samples.iter().filter_map(|s| s["cpu"].as_f64()).sum::<f64>() / samples.len().max(1) as f64;
    let rss = samples.iter().filter_map(|s| s["rss_mb"].as_u64()).max().unwrap_or(0);
    let mut fields = vec![
        ("seconds", json!(a.seconds)),
        ("uri", json!(a.uri)),
        ("cpu_percent_mean", json!((mean * 100.0).round() / 100.0)),
        ("rss_mb_max", json!(rss)),
    ];
    for k in ["datagrams", "bytes_in", "ts_packets_lost", "rtp_packets_lost", "null_packets_dropped", "resumed"] {
        fields.push((k, last[k].clone()));
    }
    fields.push(("programs", programs));
    plugin::ordered_json(&fields)
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
