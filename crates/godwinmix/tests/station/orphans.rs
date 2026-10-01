//! No show outlives its station: stopped with SIGTERM or SIGHUP, or killed
//! outright with SIGKILL, every show process is gone within seconds.

use super::support::*;
use serde_json::json;
use std::time::{Duration, Instant};

/// The pids of this station's shows, read from `ps`: a show is this binary
/// with `--station` and this station's folder in its command line.
fn shows_of(dir: &std::path::Path) -> Vec<u32> {
    let out = std::process::Command::new("ps").args(["-axo", "pid=,command="]).output().unwrap();
    let marker = dir.to_string_lossy().to_string();
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| l.contains("--station") && l.contains(&marker))
        .filter_map(|l| l.split_whitespace().next()?.parse().ok())
        .collect()
}

async fn with_shows(name: &str) -> Running {
    let (dir, port) = folder(name);
    let st = start(dir.clone(), port, &[]).await;
    let mut ws = rpc(&st, "").await;
    for (i, n) in ["Second", "Third"].iter().enumerate() {
        let added = call(&mut ws, i as u64 + 1, "show.add", json!({"name": n})).await;
        assert!(added.get("result").is_some(), "{added}");
    }
    let started = Instant::now();
    while shows_of(&dir).len() < 3 {
        assert!(started.elapsed() < Duration::from_secs(30), "three shows did not start: {:?}", shows_of(&dir));
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    st
}

async fn all_gone(dir: &std::path::Path, within: Duration) -> Duration {
    let started = Instant::now();
    loop {
        if shows_of(dir).is_empty() {
            return started.elapsed();
        }
        assert!(started.elapsed() < within, "shows still running {:?} after the station went: {:?}", within, shows_of(dir));
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_show_outlives_its_station_whichever_way_it_goes() {
    for (name, signal) in [("orphan-kill", libc::SIGKILL), ("orphan-term", libc::SIGTERM), ("orphan-hup", libc::SIGHUP)] {
        let st = with_shows(name).await;
        let dir = st.dir.clone();
        unsafe {
            libc::kill(st.pid() as i32, signal);
        }
        let took = all_gone(&dir, Duration::from_secs(15)).await;
        eprintln!("{name}: every show gone {} ms after the signal", took.as_millis());
    }
}
