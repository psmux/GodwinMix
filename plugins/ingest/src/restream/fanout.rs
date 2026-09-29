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
use crate::media_tag::{MediaTag, TagKind};
use crate::rtmp::{Event, Filter, Server, Sink};

/// One FLV tag, as `rtmp.rs` hands them over, back into a `MediaTag`. The
/// payload is made shared once and every destination gets the same one.
fn to_tag(flv: &[u8]) -> Option<MediaTag> {
    if flv.len() < 11 + 2 || flv.starts_with(b"FLV") {
        return None;
    }
    let size = u32::from_be_bytes([0, flv[1], flv[2], flv[3]]) as usize;
    let ts = u32::from_be_bytes([flv[7], flv[4], flv[5], flv[6]]);
    let body = flv.get(11..11 + size)?;
    let (kind, keyframe, header) = match flv[0] {
        9 => (TagKind::Video, body[0] >> 4 == 1, body.get(1) == Some(&0)),
        8 => (TagKind::Audio, false, body[0] >> 4 == 10 && body.get(1) == Some(&0)),
        _ => (TagKind::Script, false, false),
    };
    Some(MediaTag { kind, timestamp_ms: ts, keyframe, sequence_header: header, payload: Arc::from(body) })
}

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
    let sink: Sink = Arc::new(move |e| match e {
        Event::Bytes(b) => {
            if let Some(tag) = to_tag(&b) {
                for tx in fan.lock().unwrap().iter() {
                    let _ = tx.send(tag.clone());
                }
            }
        }
        // The stream ended: the destinations' inputs end with it, so each
        // one finishes what it holds and says goodbye to its receiver.
        Event::Left { .. } => fan.lock().unwrap().clear(),
        Event::Note(n) => eprintln!("listener: {n}"),
        other => eprintln!("listener: {other:?}"),
    });
    let _server = Server::bind("127.0.0.1", port, Filter::default(), sink).expect("bind the input");
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
