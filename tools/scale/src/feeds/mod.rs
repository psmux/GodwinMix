//! `gmx-scale feeds`: N live MPEG-TS feeds from a few files, in real time.

pub mod clip;
pub mod plan;
pub mod send;

use crate::args::Args;
use crate::{net, procs};
use clip::Clip;
use send::Totals;
use serde_json::json;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::Arc;
use std::time::{Duration, Instant};

const HELP: &str = "
gmx-scale feeds --clip FILE [--clip FILE ...] [options]

Sends --count feeds, feed i looping clip i mod (number of clips) to the i-th
group after --to (or the i-th port, for a unicast address), paced by PCR.

  --clip FILE        an MPEG-TS file; repeat for more. A file with two programs
                     is listed with each program in turn in the CSV
  --count N          how many feeds (200)
  --to ADDR          the first address, udp://239.77.0.1:5000 by default
  --port-step        multicast: the next port as well as the next group per feed
  --iface IP         the interface multicast leaves by (127.0.0.1, so nothing
                     reaches the network)
  --ttl N            multicast TTL (1)
  --seconds S        how long to send (60)
  --threads N        sending threads (2)
  --tick-ms N        how often each thread wakes (1)
  --loss P           percent of datagrams every feed drops (0)
  --jitter-ms J      up to J ms late, every feed (0)
  --impair R:K=V     loss or jitter for some feeds, e.g. 0-9:loss=1,jitter=20
  --csv FILE         write the feeds list as name,input,program,output,format
  --out ADDR         the first output address in the CSV (udp://127.0.0.1:30000)
  --format F         the CSV's format column: copy, or a rendition preset (copy)
  --json FILE        write the summary there as well as to stdout
";

pub fn main(a: Args) -> Result<(), String> {
    if a.help(HELP) {
        return Ok(());
    }
    let mut paths: Vec<&str> = a.all("clip");
    if paths.is_empty() {
        paths = a.need("clips")?.split(',').collect();
    }
    let clips = paths.iter().map(|p| Clip::load(p).map(Arc::new)).collect::<Result<Vec<_>, _>>()?;
    for c in &clips {
        eprintln!("{}: {:.0} kbit/s, {:.2} s a loop, programs {:?}", c.name, c.kbps(), c.loop_ns as f64 / 1e9, c.programs);
    }
    let count = a.num("count", 200usize)?;
    let to = net::parse(a.str("to").unwrap_or("udp://239.77.0.1:5000"))?;
    let base = plan::Impair { loss: a.num("loss", 0.0)?, jitter_ms: a.num("jitter-ms", 0.0)? };
    let special = a.all("impair").into_iter().map(plan::impairment).collect::<Result<Vec<_>, _>>()?;
    let plan = plan::build(&clips, count, (to, a.flag("port-step")), base, &special);
    if let Some(path) = a.str("csv") {
        let out = net::parse(a.str("out").unwrap_or("udp://127.0.0.1:30000"))?;
        std::fs::write(path, plan::csv(&plan.rows, out, a.str("format").unwrap_or("copy"))).map_err(|e| format!("could not write {path}: {e}"))?;
    }
    let kbps: f64 = plan.feeds.iter().map(|f| f.clip.kbps()).sum();
    let summary = run(&a, plan.feeds)?;
    let mut summary = summary;
    summary["feeds"] = json!(count);
    summary["offered_mbps"] = json!((kbps / 10.0).round() / 100.0);
    println!("{summary}");
    if let Some(path) = a.str("json") {
        std::fs::write(path, format!("{summary}\n")).map_err(|e| format!("could not write {path}: {e}"))?;
    }
    Ok(())
}

fn run(a: &Args, feeds: Vec<send::Feed>) -> Result<serde_json::Value, String> {
    let iface = a.str("iface").unwrap_or("127.0.0.1").parse().map_err(|_| "--iface takes an IPv4 address".to_string())?;
    let (ttl, threads) = (a.num("ttl", 1u32)?, a.num("threads", 2usize)?.max(1));
    let seconds = a.num("seconds", 60.0f64)?;
    let tick = Duration::from_micros((a.num("tick-ms", 1.0f64)? * 1000.0) as u64);
    let totals = Arc::new(Totals::default());
    let start = Instant::now();
    let until = start + Duration::from_secs_f64(seconds);
    let mut groups: Vec<Vec<send::Feed>> = (0..threads).map(|_| Vec::new()).collect();
    for (i, f) in feeds.into_iter().enumerate() {
        groups[i % threads].push(f);
    }
    let cpu0 = procs::own_cpu_seconds();
    stop_on_signal();
    let mut handles = Vec::new();
    for g in groups {
        let (sock, t) = (net::sender(iface, ttl)?, Arc::clone(&totals));
        handles.push(std::thread::spawn(move || send::run(g, sock, start, until, tick, &t)));
    }
    progress(&totals, start, until);
    for h in handles {
        let _ = h.join();
    }
    let wall = start.elapsed().as_secs_f64();
    let cpu = (procs::own_cpu_seconds() - cpu0) / wall * 100.0;
    let t = &totals;
    Ok(json!({
        "seconds": (wall * 10.0).round() / 10.0,
        "datagrams": t.datagrams.load(Relaxed),
        "sent_mbps": ((t.bytes.load(Relaxed) as f64 * 8.0 / wall / 1e4).round()) / 100.0,
        "dropped_on_purpose": t.dropped.load(Relaxed),
        "send_errors": t.errors.load(Relaxed),
        "late_max_ms": t.late_max_us.load(Relaxed) as f64 / 1000.0,
        "cpu_percent_of_one_core": (cpu * 10.0).round() / 10.0,
        "threads": a.num("threads", 2usize)?,
        "rss_mib": procs::own_rss_mib(),
    }))
}

/// One line every five seconds on stderr while the feeds run.
fn progress(t: &Totals, start: Instant, until: Instant) {
    let mut last = (0u64, procs::own_cpu_seconds(), Instant::now());
    while Instant::now() + Duration::from_secs(5) < until && !send::STOP.load(Relaxed) {
        for _ in 0..50 {
            if !send::STOP.load(Relaxed) {
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        let (bytes, cpu, now) = (t.bytes.load(Relaxed), procs::own_cpu_seconds(), Instant::now());
        let dt = (now - last.2).as_secs_f64();
        eprintln!(
            "{:>5.0} s  {:>7.1} Mbit/s  cpu {:>5.1}% of one core  late max {:.1} ms  errors {}",
            (now - start).as_secs_f64(),
            (bytes - last.0) as f64 * 8.0 / dt / 1e6,
            (cpu - last.1) / dt * 100.0,
            t.late_max_us.load(Relaxed) as f64 / 1000.0,
            t.errors.load(Relaxed)
        );
        last = (bytes, cpu, now);
    }
}

#[cfg(unix)]
extern "C" fn on_signal(_: libc::c_int) {
    send::STOP.store(true, Relaxed);
}

/// Ctrl+C or a kill stops the feeds cleanly, with the summary still printed.
fn stop_on_signal() {
    #[cfg(unix)]
    // SAFETY: the handler only stores to an atomic, which is signal safe.
    unsafe {
        libc::signal(libc::SIGINT, on_signal as *const () as libc::sighandler_t);
        libc::signal(libc::SIGTERM, on_signal as *const () as libc::sighandler_t);
    }
}
