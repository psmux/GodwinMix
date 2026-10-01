//! `gmx shows stats` as a table a person reads in a terminal.
//!
//! One line per show, the worst first, so on a screen of 200 the ones that
//! need a look are at the top. A tally line above says how many are in each
//! state. Everything is read from one `show.stats` answer.

use serde_json::Value;
use std::fmt::Write;

/// How bad a health state is, for the sort: alarm, warning, off, ok.
fn rank(state: &str) -> u8 {
    match state {
        "alarm" => 0,
        "warning" => 1,
        "off" => 2,
        "ok" => 3,
        _ => 4,
    }
}

fn state_of(show: &Value) -> &str {
    show["health"]["state"].as_str().unwrap_or("-")
}

/// The whole table, from one `show.stats` answer.
pub fn render(stats: &Value) -> String {
    let mut shows: Vec<&Value> = stats["shows"].as_array().map(|a| a.iter().collect()).unwrap_or_default();
    shows.sort_by(|a, b| {
        rank(state_of(a)).cmp(&rank(state_of(b))).then_with(|| a["id"].as_str().cmp(&b["id"].as_str()))
    });
    let mut out = tally(&shows);
    let _ = writeln!(
        out,
        "{:<22} {:<7} {:>7} {:>5} {:>9} {:<11} {:>5} {:>7} {:>8}  ALARMS",
        "SHOW", "HEALTH", "IN KBPS", "FPS", "SIZE", "CODECS", "CC", "OUTPUTS", "OUT KBPS"
    );
    for show in shows {
        let _ = writeln!(out, "{}", line(show));
    }
    out
}

fn tally(shows: &[&Value]) -> String {
    let count = |s: &str| shows.iter().filter(|v| state_of(v) == s).count();
    format!(
        "{} shows: {} ok, {} warning, {} alarm, {} off\n",
        shows.len(),
        count("ok"),
        count("warning"),
        count("alarm"),
        count("off")
    )
}

fn line(show: &Value) -> String {
    let input = &show["input"];
    let num = |v: &Value| v.as_u64().map(|n| n.to_string()).unwrap_or_else(|| "-".into());
    let fps = input["fps"].as_f64().map(|f| format!("{f:.0}")).unwrap_or_else(|| "-".into());
    let size = match (input["width"].as_u64(), input["height"].as_u64()) {
        (Some(w), Some(h)) if w > 0 => format!("{w}x{h}"),
        _ => "-".into(),
    };
    let codecs = format!(
        "{}/{}",
        input["video_codec"].as_str().unwrap_or("-"),
        input["audio_codec"].as_str().unwrap_or("-")
    );
    let outputs = show["outputs"].as_array().cloned().unwrap_or_default();
    let sending = outputs.iter().filter(|o| matches!(o["state"].as_str(), Some("live" | "sending" | "on" | "ok"))).count();
    let out_kbps: u64 = outputs.iter().filter_map(|o| o["kbps"].as_u64()).sum();
    format!(
        "{:<22} {:<7} {:>7} {:>5} {:>9} {:<11} {:>5} {:>7} {:>8}  {}",
        clip(show["id"].as_str().unwrap_or("-"), 22),
        state_of(show),
        num(&input["kbps"]),
        fps,
        size,
        clip(&codecs, 11),
        num(&input["cc_errors"]),
        format!("{sending}/{}", outputs.len()),
        out_kbps,
        alarms(show)
    )
}

/// `black 12s, cc-errors 3m`: each alarm and how long it has been up.
fn alarms(show: &Value) -> String {
    let list = show["health"]["alarms"].as_array().cloned().unwrap_or_default();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    list.iter()
        .map(|a| {
            let kind = a["kind"].as_str().unwrap_or("alarm");
            match a["since_ms"].as_u64() {
                // A wall clock time, as against an age.
                Some(t) if t > 1_000_000_000_000 => format!("{kind} {}", age(now.saturating_sub(t))),
                Some(t) => format!("{kind} {}", age(t)),
                None => kind.to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn age(ms: u64) -> String {
    let s = ms / 1000;
    match s {
        0..=59 => format!("{s}s"),
        60..=3599 => format!("{}m", s / 60),
        _ => format!("{}h", s / 3600),
    }
}

fn clip(s: &str, width: usize) -> String {
    if s.chars().count() <= width {
        return s.to_string();
    }
    let mut out: String = s.chars().take(width - 1).collect();
    out.push('~');
    out
}

#[cfg(test)]
#[path = "table_tests.rs"]
mod tests;
