//! The checker's answer: one object per stream and a total, as JSON and as a
//! table on stdout.

use super::stream::Stream;
use serde_json::{json, Value};

fn round(v: f64, places: i32) -> f64 {
    let f = 10f64.powi(places);
    (v * f).round() / f
}

/// One stream. Its rate is over the time it was arriving, not the whole window,
/// so a feed that started late is not counted slow.
pub fn one(name: &str, s: &Stream, seconds: f64) -> Value {
    let (gop_ms, by_pts) = s.gops();
    let span = s.first_ms.map_or(seconds * 1000.0, |f| s.last_ms - f).max(1.0) / 1000.0;
    // A stream that runs slow shows fewer keyframes than its time allows even
    // when each one is a GOP after the last by its own clock.
    let by_time = if gop_ms > 0.0 { ((span * 1000.0 / gop_ms).floor() as u64).saturating_sub(s.keyframes + 1) } else { 0 };
    let gops_dropped = by_pts.max(by_time);
    json!({
        "stream": name,
        "kbps": round(s.bytes as f64 * 8.0 / span / 1000.0, 0),
        "datagrams": s.datagrams,
        "packets": s.packets,
        "cc_errors": s.cc_errors,
        "packets_lost": s.cc_lost,
        "pcr_jumps": s.pcr_jumps,
        "pcr_gap_max_ms": round(s.pcr_gap_max_ms, 1),
        "pcr_jitter_ms": round(s.jitter_ms(), 1),
        "keyframes": s.keyframes,
        "gop_ms": round(gop_ms, 0),
        "gops_dropped": gops_dropped,
        "silence_max_ms": round(s.silence_max_ms, 0),
        "received": s.datagrams > 0,
    })
}

pub fn json(names: &[String], streams: &[Stream], seconds: f64) -> Value {
    let each: Vec<Value> = names.iter().zip(streams).map(|(n, s)| one(n, s, seconds)).collect();
    let sum = |k: &str| each.iter().map(|v| v[k].as_f64().unwrap_or(0.0)).sum::<f64>();
    let count = |k: &str| each.iter().map(|v| v[k].as_u64().unwrap_or(0)).sum::<u64>();
    let max = |k: &str| each.iter().map(|v| v[k].as_f64().unwrap_or(0.0)).fold(0.0, f64::max);
    let silent = each.iter().filter(|v| v["datagrams"].as_u64() == Some(0)).count();
    let total = json!({
        "streams": each.len(),
        "silent_streams": silent,
        "seconds": round(seconds, 1),
        "mbps": round(sum("kbps") / 1000.0, 1),
        "datagrams": count("datagrams"),
        "packets": count("packets"),
        "cc_errors": count("cc_errors"),
        "packets_lost": count("packets_lost"),
        "pcr_jumps": count("pcr_jumps"),
        "pcr_gap_max_ms": max("pcr_gap_max_ms"),
        "pcr_jitter_max_ms": max("pcr_jitter_ms"),
        "keyframes": count("keyframes"),
        "gops_dropped": count("gops_dropped"),
        "streams_with_gops_dropped": each.iter().filter(|v| v["gops_dropped"].as_u64().unwrap_or(0) > 0).count(),
        "silence_max_ms": max("silence_max_ms"),
    });
    json!({ "total": total, "streams": each })
}

pub fn print(out: &Value, quiet: bool) {
    let cols = ["kbps", "packets", "cc_errors", "packets_lost", "pcr_jumps", "pcr_gap_max_ms", "pcr_jitter_ms", "keyframes", "gop_ms", "gops_dropped", "silence_max_ms"];
    if !quiet {
        println!("{:<22} {}", "stream", cols.iter().map(|c| format!(" {c:>14}")).collect::<String>());
        for s in out["streams"].as_array().into_iter().flatten() {
            let row: String = cols.iter().map(|c| format!(" {:>14}", s[*c].to_string())).collect();
            println!("{:<22} {row}", s["stream"].as_str().unwrap_or(""));
        }
    }
    println!("total {}", out["total"]);
}
