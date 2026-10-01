//! `gmx-scale sample`: once a second, the CPU and memory of a station and of
//! every process under it, by role, and one `show.stats` read for every show.

use super::roles::{self, Roles};
use super::rpc::Client;
use super::stats::Stats;
use crate::args::Args;
use crate::procs;
use serde_json::json;
use std::collections::HashMap;
use std::io::Write;
use std::time::{Duration, Instant};

const HELP: &str = "
gmx-scale sample --pid PID [--station ADDR] [options]

  --pid PID          the station's process; everything under it is followed
  --station ADDR     its control address, for show.stats (127.0.0.1:8080)
  --token T          a bearer token, when the station has one (or GODWINMIX_TOKEN)
  --seconds S        how long (60)
  --watch NAME=PID   also sample another process, e.g. feeds=4242; repeat
  --direct WORD      a process whose command has WORD is the direct host (gmx-ingest, the
                     ingest plugin, which also serves the channels)
  --csv FILE         every second, every role: t,role,count,cpu_percent,rss_mib
  --json FILE        the summary
";

pub fn main(a: Args) -> Result<(), String> {
    if a.help(HELP) {
        return Ok(());
    }
    let root: u32 = a.need("pid")?.parse().map_err(|_| "--pid takes a process id".to_string())?;
    let mut watch: Vec<(String, u32)> = Vec::new();
    for w in a.all("watch") {
        let (name, pid) = w.split_once('=').ok_or(format!("--watch {w}: write it as name=pid"))?;
        watch.push((name.into(), pid.parse().map_err(|_| format!("--watch {w}: {pid} is not a process id"))?));
    }
    let token = a.str("token").map(String::from).or_else(|| std::env::var("GODWINMIX_TOKEN").ok());
    let client = Client::connect(a.str("station").unwrap_or("127.0.0.1:8080"), token).ok();
    let mut csv = match a.str("csv") {
        Some(p) => Some(std::fs::File::create(p).map_err(|e| format!("could not write {p}: {e}"))?),
        None => None,
    };
    if let Some(f) = csv.as_mut() {
        let _ = writeln!(f, "t,role,count,cpu_percent,rss_mib");
    }
    let seconds = a.num("seconds", 60.0f64)?;
    let direct = a.str("direct").unwrap_or("gmx-ingest").to_string();
    let mut roles = Roles::default();
    let mut stats = Stats::default();
    let mut last: HashMap<u32, f64> = HashMap::new();
    let mut last_at = Instant::now();
    let start = Instant::now();
    let mut tick = 0u64;
    while start.elapsed().as_secs_f64() < seconds {
        let all = procs::all();
        let now = Instant::now();
        let dt = (now - last_at).as_secs_f64();
        let seen = roles::classify(&procs::tree(&all, root), root, &direct);
        let extra: Vec<_> = watch.iter().filter_map(|(name, pid)| all.iter().find(|p| p.pid == *pid).map(|p| (name.clone(), p.clone()))).collect();
        let rows = roles.add(&seen, &extra, &last, dt, tick > 0);
        last = seen.iter().chain(&extra).map(|(_, p)| (p.pid, p.cpu_seconds)).collect();
        last_at = now;
        if let (Some(f), true) = (csv.as_mut(), tick > 0) {
            for (role, count, cpu, rss) in rows {
                let _ = writeln!(f, "{:.0},{role},{count},{cpu:.1},{rss:.1}", start.elapsed().as_secs_f64());
            }
        }
        if let Some(c) = client.as_ref() {
            stats.read(c);
        }
        tick += 1;
        let next = start + Duration::from_secs(tick);
        std::thread::sleep(next.saturating_duration_since(Instant::now()));
    }
    let out = json!({"seconds": (start.elapsed().as_secs_f64()).round(), "roles": roles.summary(), "stats": stats.summary()});
    println!("{out}");
    if let Some(path) = a.str("json") {
        std::fs::write(path, format!("{out}\n")).map_err(|e| format!("could not write {path}: {e}"))?;
    }
    Ok(())
}
