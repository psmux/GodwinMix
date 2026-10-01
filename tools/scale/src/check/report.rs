//! The checker's answer: one object per stream and a total, as JSON and as a
//! table on stdout.

use super::stream::Stream;
use serde_json::{json, Value};

fn round(v: f64, places: i32) -> f64 {
    let f = 10f64.powi(places);
    (v * f).round() / f
}

pub fn one(name: &str, s: &Stream, seconds: f64) -> Value {
    let (gop_ms, gops_dropped) = s.gops();
    json!({
        "stream": name,
        "kbps": round(s.bytes as f64 * 8.0 / seconds.max(0.001) / 1000.0, 0),
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
    let max = |k: &str| each.iter().map(|v| v[k].as_f64().unwrap_or(0.0)).fold(0.0, f64::max);
    let silent = each.iter().filter(|v| v["datagrams"].as_u64() == Some(0)).count();
    let total = json!({
        "streams": each.len(),
        "silent_streams": silent,
        "seconds": round(seconds, 1),
        "mbps": round(sum("kbps") / 1000.0, 1),
        "datagrams": sum("datagrams"),
        "packets": sum("packets"),
        "cc_errors": sum("cc_errors"),
        "packets_lost": sum("packets_lost"),
        "pcr_jumps": sum("pcr_jumps"),
        "pcr_gap_max_ms": max("pcr_gap_max_ms"),
        "pcr_jitter_max_ms": max("pcr_jitter_ms"),
        "keyframes": sum("keyframes"),
        "gops_dropped": sum("gops_dropped"),
        "streams_with_gops_dropped": each.iter().filter(|v| v["gops_dropped"].as_u64().unwrap_or(0) > 0).count(),
        "silence_max_ms": max("silence_max_ms"),
    });
    json!({ "total": total, "streams": each })
}

pub fn print(out: &Value, quiet: bool) {
    let cols = ["kbps", "packets", "cc_errors", "packets_lost", "pcr_jumps", "pcr_gap_max_ms", "pcr_jitter_ms", "keyframes", "gop_ms", "gops_dropped", "silence_max_ms"];
    if !quiet {
        println!("{:<22} {}", "stream", cols.iter().map(|c| format!("{c:>14}")).collect::<String>());
        for s in out["streams"].as_array().into_iter().flatten() {
            let row: String = cols.iter().map(|c| format!("{:>14}", s[*c])).collect();
            println!("{:<22} {row}", s["stream"].as_str().unwrap_or(""));
        }
    }
    println!("total {}", out["total"]);
}
