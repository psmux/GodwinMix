//! Check what `dev/harness/restream-fanout.sh` recorded.
//!
//! For every receiver: how many video frames and keyframes arrived, the
//! largest gap between two frames, and whether each recording starts on a
//! keyframe. A receiver that was never killed must have every frame the
//! publisher sent, with no gap longer than a frame and a half. The one that
//! was killed has two recordings; the second must start on a keyframe.
//!
//! ```sh
//! cargo run --release -p gmx-ingest --example restream_check -- <dir> <port> [<port> ...]
//! ```
//!
//! Reads each recording's packets with `ffprobe`, so it needs ffprobe on the
//! path. Exits 0 on PASS and 1 on FAIL.

use serde_json::Value;
use std::path::Path;
use std::process::{Command, ExitCode};

/// One video packet: its decode time in seconds, and whether it is a keyframe.
type Frame = (f64, bool);

fn frames(path: &Path) -> Result<Vec<Frame>, String> {
    let out = Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "v:0", "-show_entries", "packet=dts_time,flags", "-of", "json"])
        .arg(path)
        .output()
        .map_err(|e| format!("could not run ffprobe on {}: {e}", path.display()))?;
    if !out.status.success() {
        return Err(format!("ffprobe could not read {}: {}", path.display(), String::from_utf8_lossy(&out.stderr).trim()));
    }
    let json: Value = serde_json::from_slice(&out.stdout).map_err(|e| format!("ffprobe said something that is not JSON: {e}"))?;
    Ok(packets(&json))
}

fn packets(json: &Value) -> Vec<Frame> {
    let list = json["packets"].as_array().cloned().unwrap_or_default();
    list.iter()
        .filter_map(|p| {
            let dts = p["dts_time"].as_str()?.parse::<f64>().ok()?;
            Some((dts, p["flags"].as_str().unwrap_or("").contains('K')))
        })
        .collect()
}

/// Print one line about a recording and return its largest gap in seconds.
fn describe(name: &str, f: &[Frame]) -> f64 {
    let worst = f.windows(2).map(|w| w[1].0 - w[0].0).fold(0.0, f64::max);
    let keys = f.iter().filter(|(_, k)| *k).count();
    let first = if f.first().is_some_and(|(_, k)| *k) { "keyframe" } else { "NOT a keyframe" };
    println!("{name}: {} frames, {keys} keyframes, largest gap {:.0} ms, starts on {first}", f.len(), worst * 1000.0);
    worst
}

fn check(folder: &Path, ports: &[String]) -> Result<bool, String> {
    let source = frames(&folder.join("source.flv"))?;
    describe("publisher", &source);
    let mut ok = true;
    for port in ports {
        let second = folder.join(format!("recv-{port}-2.flv"));
        let f = frames(&folder.join(format!("recv-{port}-1.flv")))?;
        let worst = describe(&format!("receiver {port}"), &f);
        if second.exists() {
            let g = frames(&second)?;
            describe(&format!("receiver {port} after it came back"), &g);
            ok &= g.first().is_some_and(|(_, k)| *k);
            continue;
        }
        // Every frame, and none more than a frame and a half after the last.
        let whole = f.len() == source.len() && worst < 0.050 && f.first().is_some_and(|(_, k)| *k);
        let verdict = if whole { "every frame arrived, no GOP lost" } else { "FRAMES WERE LOST" };
        println!("  {verdict} ({} short of the publisher)", source.len() as i64 - f.len() as i64);
        ok &= whole;
    }
    Ok(ok)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some((folder, ports)) = args.split_first() else {
        eprintln!("usage: restream_check <dir> <port> [<port> ...]");
        return ExitCode::from(2);
    };
    match check(Path::new(folder), ports) {
        Ok(ok) => {
            println!("{}", if ok { "PASS" } else { "FAIL" });
            if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE }
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packets_are_read_from_ffprobes_json() {
        let json = serde_json::json!({ "packets": [
            { "dts_time": "0.000000", "flags": "K__" },
            { "dts_time": "0.033333", "flags": "___" },
            { "dts_time": "0.100000", "flags": "___" },
            { "flags": "___" }
        ]});
        let f = packets(&json);
        assert_eq!(f.len(), 3, "a packet with no time is left out");
        assert!(f[0].1 && !f[1].1);
        assert!((describe("t", &f) - 0.066667).abs() < 1e-6);
    }
}
