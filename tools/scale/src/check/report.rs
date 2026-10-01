//! The checker's answer: one object per stream and a total, as JSON and as a
//! table on stdout.

use super::stream::Stream;
use serde_json::{json, Value};

/// A GOP counts as dropped when the keyframes either side of it are this many
/// typical GOPs apart, or more.
const GOP_GAP: f64 = 1.5;

/// The GOP most keyframes are apart, and how many went missing between them.
fn gops(s: &Stream) -> (f64, u64) {
    let mut g: Vec<f64> = s.key_gaps_ms.iter().copied().filter(|v| *v > 0.0).collect();
    if g.is_empty() {
        return (0.0, 0);
    }
    g.sort_by(f64::total_cmp);
    let typical = g[g.len() / 2];
    let dropped = g.iter().filter(|v| **v >= typical * GOP_GAP).map(|v| (v / typical).round() as u64 - 1).sum();
    (typical, dropped)
}

fn round(v: f64, places: i32) -> f64 {
    let f = 10f64.powi(places);
    (v * f).round() / f
}

/// One stream. Its rate is over the whole window, and a stream that stops or
/// starts late has the time it was missing counted as silence, so a stream
/// that dies part way through cannot look healthy.
pub fn one(name: &str, s: &Stream, seconds: f64) -> Value {
    let (gop_ms, by_pts) = gops(s);
    let span = seconds.max(0.001);
    let edges = s.first_ms.map_or(seconds * 1000.0, |f| f.max(seconds * 1000.0 - s.last_ms));
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
        "silence_max_ms": round(s.silence_max_ms.max(edges), 0),
        "received_seconds": round(s.first_ms.map_or(0.0, |f| s.last_ms - f) / 1000.0, 1),
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
        "streams_silent_over_1s": each.iter().filter(|v| v["silence_max_ms"].as_f64().unwrap_or(0.0) >= 1000.0).count(),
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
