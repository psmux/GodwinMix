//! The tables on a report page.

use serde_json::Value;
use std::fmt::Write as _;

fn role(r: &Value) -> String {
    if r.is_null() {
        return "no process of this kind ran".into();
    }
    let n = r["processes"].as_u64().unwrap_or(0);
    let per = if n > 1 {
        format!(", {:.2}% and {:.1} MiB each", r["cpu_avg_percent"].as_f64().unwrap_or(0.0) / n as f64, r["rss_avg_mib"].as_f64().unwrap_or(0.0) / n as f64)
    } else {
        String::new()
    };
    format!("{}% of one core on average, {}% at peak, {} MiB ({n} process{}){per}", r["cpu_avg_percent"], r["cpu_peak_percent"], r["rss_avg_mib"], if n == 1 { "" } else { "es" })
}

fn received(c: &Value) -> String {
    let t = &c["total"];
    if t.is_null() {
        return "not checked".into();
    }
    format!(
        "{} of {} streams arrived, {} Mbit/s; {} CC errors ({} packets lost), {} PCR jumps, PCR jitter up to {} ms; {} keyframes, {} GOPs dropped in {} streams; longest silence {} ms. The checker used {}% of one core",
        t["streams"].as_u64().unwrap_or(0) - t["silent_streams"].as_u64().unwrap_or(0),
        t["streams"], t["mbps"], t["cc_errors"], t["packets_lost"], t["pcr_jumps"], t["pcr_jitter_max_ms"], t["keyframes"], t["gops_dropped"], t["streams_with_gops_dropped"], t["silence_max_ms"], t["checker_cpu_percent"]
    )
}

fn generator(f: &Value) -> String {
    format!(
        "{} feeds, {} Mbit/s sent, {}% of one core, {} MiB, {} send errors, latest datagram {} ms late",
        f["feeds"], f["sent_mbps"], f["cpu_percent_of_one_core"], f["rss_mib"], f["send_errors"], f["late_max_ms"]
    )
}

pub fn headline(feeds: &[Value; 2], add: &Value, sample: &Value, check: &Value, check_in: &Value) -> String {
    let r = &sample["roles"];
    let st = &sample["stats"];
    let mut rows: Vec<(String, String)> = Vec::new();
    if !add.is_null() {
        rows.push(("Shows added".into(), format!("{} of {} with {} in {} s", add["added"], add["requested"], add["method"].as_str().unwrap_or("?"), add["seconds"])));
        if !add["plan"].is_null() {
            rows.push(("Dry run plan".into(), format!("`{}`", add["plan"])));
        }
    }
    if !r.is_null() {
        rows.push(("Station".into(), role(&r["station"])));
        rows.push(("Direct host".into(), role(&r["direct host"])));
        rows.push(("Show processes".into(), role(&r["shows"])));
        for (name, v) in r.as_object().into_iter().flatten().filter(|(k, _)| k.starts_with("plugin ")) {
            rows.push((format!("Plugin `{}`", &name[7..]), role(v)));
        }
        rows.push(("Station and everything under it".into(), role(&r["total"])));
    }
    for (f, label) in feeds.iter().zip(["Feeds generator, alone with the checker", "Feeds generator, during the run"]) {
        if !f.is_null() {
            rows.push((label.into(), generator(f)));
        }
    }
    if !check_in.is_null() {
        rows.push(("Feeds as sent (checked at the generator)".into(), received(check_in)));
    }
    if !check.is_null() {
        rows.push(("Outputs received".into(), received(check)));
    }
    if !st["method"].is_null() && st["reads"].as_u64().unwrap_or(0) > 0 {
        rows.push((format!("`{}`, once a second", st["method"].as_str().unwrap_or("")), format!("{} reads, {} ms on average, {} ms at most, {} shows", st["reads"], st["read_ms_avg"], st["read_ms_max"], st["shows"])));
        rows.push(("Health at the end".into(), format!("shows {}, alarms at peak {}, outputs {}", st["shows_by_state"], st["alarms_peak"], st["outputs_by_state"])));
        rows.push(("Input as the station counted it".into(), format!("{} kbit/s, {} CC errors, {} packets lost", st["input_kbps"], st["input_cc_errors"], st["input_packets_lost"])));
    }
    let mut s = String::from("| Measure | Result |\n|---|---|\n");
    for (k, v) in rows {
        let _ = writeln!(s, "| {k} | {} |", v.replace('|', "/"));
    }
    s
}

pub fn roles(r: &Value) -> String {
    let mut s = String::from("| Role | Processes | CPU avg % | CPU peak % | RSS avg MiB | RSS peak MiB |\n|---|---|---|---|---|---|\n");
    for (name, v) in r.as_object().into_iter().flatten() {
        let _ = writeln!(s, "| {name} | {} | {} | {} | {} | {} |", v["processes"], v["cpu_avg_percent"], v["cpu_peak_percent"], v["rss_avg_mib"], v["rss_peak_mib"]);
    }
    s
}

/// The `n` streams with the most trouble: GOPs dropped, then CC errors, then silence.
pub fn worst(streams: &Value, n: usize) -> String {
    let mut v: Vec<&Value> = streams.as_array().into_iter().flatten().collect();
    let key = |s: &Value| (s["gops_dropped"].as_u64().unwrap_or(0), s["cc_errors"].as_u64().unwrap_or(0), s["silence_max_ms"].as_f64().unwrap_or(0.0) as u64);
    v.sort_by_key(|s| std::cmp::Reverse(key(s)));
    let mut s = String::from("| Stream | kbit/s | CC errors | Packets lost | PCR jumps | PCR gap max ms | Keyframes | GOP ms | GOPs dropped | Silence max ms |\n|---|---|---|---|---|---|---|---|---|---|\n");
    for x in v.into_iter().take(n) {
        let _ = writeln!(
            s,
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            x["stream"].as_str().unwrap_or(""), x["kbps"], x["cc_errors"], x["packets_lost"], x["pcr_jumps"], x["pcr_gap_max_ms"], x["keyframes"], x["gop_ms"], x["gops_dropped"], x["silence_max_ms"]
        );
    }
    s
}
