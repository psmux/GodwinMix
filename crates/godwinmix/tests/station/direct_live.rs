//! A direct show end to end, through the station: a real MPEG-TS feed over
//! UDP in, the ingest plugin installed while the station runs (so the
//! station has to start it and hand it the table), and the bytes counted on
//! the UDP copy output. Then a restart, after which the plugin must be
//! handed the table again before it starts.

use super::support::*;
use gstreamer::prelude::*;
use serde_json::{json, Value};
use std::net::UdpSocket;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

const REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

/// The ingest plugin as a folder `plugin.add` takes: its manifest and what
/// it reads, and its binary built in the same profile as the station's.
fn staged_ingest(dir: &Path) -> PathBuf {
    let release = BIN.contains("/release/");
    let mut cargo = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
    cargo.current_dir(REPO).args(["build", "-q", "-p", "gmx-ingest"]);
    if release {
        cargo.arg("--release");
    }
    assert!(cargo.status().unwrap().success(), "gmx-ingest did not build");
    let built = Path::new(BIN).with_file_name("gmx-ingest");
    let to = dir.join("ingest-src");
    let from = Path::new(REPO).join("plugins/ingest");
    std::fs::create_dir_all(to.join("bin")).unwrap();
    for f in ["gmx-plugin.toml", "README.md"] {
        std::fs::copy(from.join(f), to.join(f)).unwrap();
    }
    for d in ["schemas", "skills", "designer"] {
        copy_dir(&from.join(d), &to.join(d));
    }
    std::fs::copy(built, to.join("bin/gmx-ingest")).unwrap();
    to
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let path = e.path();
        if path.is_dir() {
            copy_dir(&path, &to.join(e.file_name()));
        } else {
            std::fs::copy(&path, to.join(e.file_name())).unwrap();
        }
    }
}

/// 320x180 H.264 and AAC in MPEG-TS, sent live to `port`.
fn feed(port: u16) -> gstreamer::Pipeline {
    gstreamer::init().unwrap();
    let text = format!(
        "videotestsrc is-live=true ! video/x-raw,width=320,height=180,framerate=30/1 ! x264enc tune=zerolatency key-int-max=30 bitrate=600 ! h264parse ! mux. \
         audiotestsrc is-live=true ! audioconvert ! avenc_aac ! aacparse ! mux. \
         mpegtsmux name=mux alignment=7 ! udpsink host=127.0.0.1 port={port} sync=false"
    );
    let p = gstreamer::parse::launch(&text).expect("x264enc, avenc_aac and mpegtsmux are installed").downcast::<gstreamer::Pipeline>().unwrap();
    p.set_state(gstreamer::State::Playing).unwrap();
    p
}

/// Bytes that reach `socket` within `wait`, stopping once there are `enough`.
fn received(socket: &UdpSocket, wait: Duration, enough: usize) -> usize {
    socket.set_read_timeout(Some(Duration::from_millis(200))).unwrap();
    let (mut n, mut buf, until) = (0, vec![0u8; 65536], Instant::now() + wait);
    while Instant::now() < until && n < enough {
        if let Ok(got) = socket.recv(&mut buf) {
            n += got;
        }
    }
    n
}

fn free_udp() -> u16 {
    UdpSocket::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

async fn output_state(ws: &mut Ws, id: u64) -> Value {
    let s = call(ws, id, "show.stats", json!({"ids": ["feed"]})).await;
    s["result"]["shows"][0].clone()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_direct_show_carries_a_real_udp_feed_to_a_udp_copy_output_through_the_station() {
    let (dir, port) = folder("direct-live");
    let source = staged_ingest(&dir);
    let st = start(dir.clone(), port, &[]).await;
    let mut ws = rpc(&st, "").await;
    let _ = call(&mut ws, 1, "channel.remove", json!({"id": "live"})).await;

    let (input, receiver) = (free_udp(), UdpSocket::bind("127.0.0.1:0").unwrap());
    let out = receiver.local_addr().unwrap().port();
    let pipeline = feed(input);
    let added = call(&mut ws, 2, "show.add", json!({"name": "Feed", "compositing": false,
        "input": {"uri": format!("udp://127.0.0.1:{input}")}, "outputs": [{"id": "out", "uri": format!("udp://127.0.0.1:{out}")}]})).await;
    assert!(added.get("error").is_none(), "{added}");

    // Installed while the station runs: a show answers plugin.add, and the
    // station must start the plugin itself, with the direct table.
    let asked = call(&mut ws, 3, "plugin.add", json!({"source": source.to_string_lossy()})).await;
    assert!(asked.get("error").is_none(), "{asked}");
    let got = received(&receiver, Duration::from_secs(60), 200_000);
    let stats = output_state(&mut ws, 4).await;
    assert!(got >= 200_000, "only {got} bytes reached the copy output in 60 s: {stats}; see {}", dir.join("log.jsonl").display());
    assert_eq!(stats["outputs"][0]["state"], "live", "{stats}");
    assert!(stats["input"]["kbps"].as_u64().unwrap_or(0) > 0, "{stats}");

    drop(ws);
    drop(st);
    // What the first run sent and nobody has read yet is not the second's.
    received(&receiver, Duration::from_secs(1), usize::MAX);
    let st = start(dir.clone(), port, &[]).await;
    let got = received(&receiver, Duration::from_secs(60), 200_000);
    let mut ws = rpc(&st, "").await;
    let stats = output_state(&mut ws, 5).await;
    assert!(got >= 200_000, "after a restart only {got} bytes in 60 s: {stats}");
    pipeline.set_state(gstreamer::State::Null).unwrap();
}
