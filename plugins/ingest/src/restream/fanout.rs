//! The fan out harness `dev/harness/restream-fanout.sh` runs: one RTMP
//! listener taking a publisher, its tags handed to several destinations the
//! way the channel hub will hand them, and a line of stats every ten seconds.
//! Ignored by `cargo test`; the script starts it by name.
//!
//! `GMX_FANOUT_IN` is the port to listen on, `GMX_FANOUT_OUT` a comma list of
//! addresses to send to, `GMX_FANOUT_SECS` how long to run.

use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::{start, Target};
use crate::media_tag::MediaTag;
use super::test_gate::listen;

fn env(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

#[test]
#[ignore = "a two minute run against real receivers; dev/harness/restream-fanout.sh starts it"]
fn fanout() {
    let port: u16 = env("GMX_FANOUT_IN", "19355").parse().unwrap();
    let secs: u64 = env("GMX_FANOUT_SECS", "120").parse().unwrap();
    let outs = env("GMX_FANOUT_OUT", "rtmp://127.0.0.1:19352/live/a");
    let mut senders = Vec::new();
    let mut handles = Vec::new();
    for (n, url) in outs.split(',').enumerate() {
        let (tx, rx) = mpsc::channel::<MediaTag>();
        senders.push(tx);
        handles.push((url.to_string(), start(Target::new(&format!("out-{n}"), "custom", url), rx)));
    }
    let senders = Arc::new(Mutex::new(senders));
    let fan = Arc::clone(&senders);
    // The stream ending ends the destinations' inputs with it, so each one
    // finishes what it holds and says goodbye to its receiver.
    let _server = listen(port, None, Arc::new(move |t| match t {
        Some(tag) => {
            for tx in fan.lock().unwrap().iter() {
                let _ = tx.send(tag.clone());
            }
        }
        None => fan.lock().unwrap().clear(),
    }));
    eprintln!("fanout: listening on {port}, {} destinations, {secs} s", handles.len());
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(secs) {
        std::thread::sleep(Duration::from_secs(10));
        for (url, h) in &handles {
            let s = h.stats();
            eprintln!(
                "fanout t={:>3}s {url} state={:?} kbps={} reconnects={} dropped_gops={} dropped_tags={} bytes={} error={:?}",
                started.elapsed().as_secs(), s.live.state, s.live.kbps, s.live.reconnects,
                s.dropped.gops, s.dropped.tags, s.bytes, s.live.error
            );
        }
    }
}
