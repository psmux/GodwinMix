//! Turn each scenario's `BENCH` lines into one row of a Markdown table.

use std::collections::HashMap;

use crate::matrix::{Mech, Scenario};

pub struct Row {
    label: String,
    owner_cpu: Option<f64>,
    owner_frames: Option<u64>,
    owner_dropped: Option<u64>,
    reader_cpu: Vec<f64>,
    frames: Vec<u64>,
    skipped: Vec<u64>,
    lat_ms: Option<(f64, f64, f64)>,
    stalled: Option<(u64, u64)>,
}

pub fn label(s: &Scenario) -> String {
    let what = match s.mech {
        Mech::Bus if s.readers == 0 => return "frame bus, owner alone".into(),
        Mech::Bus => "frame bus",
        Mech::Unixfd => "unixfdsink",
        Mech::Decode => "each decodes itself",
    };
    let stall = if s.stalled { ", one stalled" } else { "" };
    format!("{what}, {} reader{}{stall}", s.readers, if s.readers == 1 { "" } else { "s" })
}

fn f(m: &HashMap<String, String>, k: &str) -> f64 {
    m.get(k).and_then(|v| v.parse().ok()).unwrap_or(0.0)
}

impl Row {
    pub fn from(_s: &Scenario, label: String, lines: Vec<HashMap<String, String>>) -> Row {
        let owner = lines.iter().find(|m| m.get("role").is_some_and(|r| r == "owner"));
        let readers: Vec<_> = lines.iter().filter(|m| m.get("role").is_some_and(|r| r != "owner")).collect();
        let normal: Vec<_> = readers.iter().filter(|m| m.get("stalled").is_none_or(|v| v != "true")).collect();
        let stalled = readers.iter().find(|m| m.get("stalled").is_some_and(|v| v == "true"));
        let lat = normal.iter().filter(|m| m.contains_key("p50_us")).fold(None, |acc: Option<(f64, f64, f64)>, m| {
            let (a, b, c) = (f(m, "p50_us") / 1000.0, f(m, "p99_us") / 1000.0, f(m, "max_us") / 1000.0);
            Some(acc.map_or((a, b, c), |(x, y, z)| (x.max(a), y.max(b), z.max(c))))
        });
        Row {
            label,
            owner_cpu: owner.map(|m| f(m, "cpu")),
            owner_frames: owner.map(|m| f(m, "frames") as u64),
            owner_dropped: owner.map(|m| f(m, "dropped") as u64),
            reader_cpu: readers.iter().map(|m| f(m, "cpu")).collect(),
            frames: normal.iter().map(|m| f(m, "frames") as u64).collect(),
            skipped: normal.iter().map(|m| f(m, "skipped") as u64).collect(),
            lat_ms: lat,
            stalled: stalled.map(|m| (f(m, "frames") as u64, f(m, "skipped") as u64)),
        }
    }
}

impl Row {
    /// Owner and every reader together, percent of one core.
    pub fn total(&self) -> f64 {
        self.owner_cpu.unwrap_or(0.0) + self.reader_cpu.iter().sum::<f64>()
    }
}

fn opt<T: std::fmt::Display>(v: Option<T>) -> String {
    v.map_or("".into(), |v| v.to_string())
}

pub fn render(rows: &[Row], secs: u64, decoder: &str, format: &str, repeat: u64) -> String {
    let mut out = format!(
        "1080p30 H.264 decoded with {decoder}, frames {format}, measured over {secs} s ({} frames), median of {repeat} run(s) by total CPU.\n\
         CPU is percent of one core. Latency is from the owner being handed a decoded frame to a reader holding it.\n\n\
         | Scenario | Owner CPU | Reader CPU, mean / max | Total CPU | Owner frames, dropped | Frames per reader, min | Skipped per reader, max | Latency p50 / p99 / max ms | Stalled reader got / skipped |\n\
         |---|---|---|---|---|---|---|---|---|\n",
        secs * 30
    );
    for r in rows {
        let n = r.reader_cpu.len().max(1) as f64;
        let mean = r.reader_cpu.iter().sum::<f64>() / n;
        let max = r.reader_cpu.iter().cloned().fold(0.0, f64::max);
        let total = r.total();
        let readers = if r.reader_cpu.is_empty() { String::new() } else { format!("{mean:.1} / {max:.1}") };
        let owner_frames = r.owner_frames.map_or(String::new(), |f| format!("{f}, {}", opt(r.owner_dropped)));
        let lat = r.lat_ms.map_or(String::new(), |(a, b, c)| format!("{a:.2} / {b:.2} / {c:.2}"));
        let stalled = r.stalled.map_or(String::new(), |(g, s)| format!("{g} / {s}"));
        out += &format!(
            "| {} | {} | {readers} | {total:.1} | {owner_frames} | {} | {} | {lat} | {stalled} |\n",
            r.label,
            r.owner_cpu.map_or(String::new(), |c| format!("{c:.1}")),
            opt(r.frames.iter().min()),
            opt(r.skipped.iter().max()),
        );
    }
    out
}
