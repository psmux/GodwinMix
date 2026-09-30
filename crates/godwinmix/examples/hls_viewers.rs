//! Simulated viewers for an HLS output, and what they cost the core.
//!
//! Each viewer is a task that behaves like a player: it reads the
//! multivariant playlist, takes one rung (spread over the ladder), and then
//! either follows it with LL-HLS blocking reloads, fetching every new part, or
//! polls the playlist and fetches every new segment. Meanwhile the core's CPU
//! and resident memory are sampled with `ps`.
//!
//! ```sh
//! cargo run --release -p godwinmix --example hls_viewers -- \
//!     'http://127.0.0.1:8080/hls/viewers/master.m3u8?key=...' \
//!     --viewers 50 --seconds 60 --pid "$(pgrep -f 'godwinmix -c')"
//! ```

#[path = "hls_viewers/player.rs"]
mod player;
#[path = "hls_viewers/usage.rs"]
mod usage;

use player::Totals;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

struct Args {
    master: String,
    viewers: usize,
    seconds: u64,
    pid: Option<u32>,
}

fn parse(mut it: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut a = Args { master: String::new(), viewers: 50, seconds: 60, pid: None };
    while let Some(arg) = it.next() {
        let mut num = |name: &str| -> Result<u64, String> {
            it.next().and_then(|v| v.parse().ok()).ok_or(format!("{name} takes a number"))
        };
        match arg.as_str() {
            "--viewers" => a.viewers = num("--viewers")? as usize,
            "--seconds" => a.seconds = num("--seconds")?,
            "--pid" => a.pid = Some(num("--pid")? as u32),
            _ if a.master.is_empty() => a.master = arg,
            other => return Err(format!("hls_viewers does not know {other}")),
        }
    }
    if a.master.is_empty() {
        return Err("usage: hls_viewers MASTER_URL [--viewers n] [--seconds s] [--pid pid]".into());
    }
    Ok(a)
}

#[tokio::main]
async fn main() -> Result<(), String> {
    let a = parse(std::env::args().skip(1))?;
    let (totals, stop) = (Arc::new(Totals::default()), Arc::new(AtomicBool::new(false)));
    let client = reqwest::Client::builder().timeout(Duration::from_secs(15)).build().map_err(|e| e.to_string())?;
    let rss = a.pid.map(|pid| usage::sample_rss(pid, stop.clone()));
    let cpu0 = a.pid.map(|pid| (usage::cpu_seconds(pid), Instant::now()));
    let mut tasks = Vec::new();
    for n in 0..a.viewers {
        tasks.push(tokio::spawn(player::viewer(client.clone(), a.master.clone(), n, totals.clone(), stop.clone())));
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    tokio::time::sleep(Duration::from_secs(a.seconds)).await;
    let cpu = a.pid.zip(cpu0).map(|(pid, (c0, t0))| 100.0 * (usage::cpu_seconds(pid) - c0) / t0.elapsed().as_secs_f64());
    stop.store(true, Ordering::Relaxed);
    for t in tasks {
        let _ = tokio::time::timeout(Duration::from_secs(20), t).await;
    }
    let samples = match rss {
        Some(h) => h.join().unwrap_or_default(),
        None => Vec::new(),
    };
    report(&a, &totals, cpu, &samples);
    Ok(())
}

fn report(a: &Args, t: &Totals, cpu: Option<f64>, rss: &[f64]) {
    let (requests, bytes, errors) = t.counts();
    let mut media = t.media();
    media.sort_by(f64::total_cmp);
    if media.is_empty() {
        media.push(0.0);
    }
    println!(
        "viewers {} for {} s: {requests} requests, {:.1} MB, {errors} errors, {:.1} Mbit/s out",
        a.viewers,
        a.seconds,
        bytes as f64 / 1e6,
        bytes as f64 * 8.0 / a.seconds as f64 / 1e6
    );
    let p95 = media[(media.len() as f64 * 0.95) as usize];
    println!(
        "media fetch ms: median {:.1}, p95 {:.1}, max {:.1}",
        median(&media) * 1000.0,
        p95 * 1000.0,
        media[media.len() - 1] * 1000.0
    );
    if let (Some(cpu), Some(first)) = (cpu, rss.first()) {
        let max = rss.iter().copied().fold(0.0, f64::max);
        println!(
            "core cpu {cpu:.1}% of one core over the run (from its CPU time); rss MiB: start {first:.0}, end {:.0}, max {max:.0}",
            rss[rss.len() - 1]
        );
    }
}

/// The middle of a sorted list, or the mean of the two middles.
fn median(sorted: &[f64]) -> f64 {
    let n = sorted.len();
    if n % 2 == 1 { sorted[n / 2] } else { (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0 }
}
