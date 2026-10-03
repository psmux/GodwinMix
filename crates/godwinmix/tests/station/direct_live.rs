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
pub fn staged_ingest(dir: &Path) -> PathBuf {
    let release = BIN.contains("/release/");
    let mut cargo = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
    cargo.current_dir(REPO).args(["build", "-q", "-p", "gmx-ingest"]);
    if release {
        cargo.arg("--release");
    }
    assert!(cargo.status().unwrap().success(), "gmx-ingest did not build");
    let built = Path::new(BIN).with_file_name(format!("gmx-ingest{}", std::env::consts::EXE_SUFFIX));
    let to = dir.join("ingest-src");
    let from = Path::new(REPO).join("plugins/ingest");
    std::fs::create_dir_all(to.join("bin")).unwrap();
    for f in ["gmx-plugin.toml", "README.md"] {
        std::fs::copy(from.join(f), to.join(f)).unwrap();
    }
    for d in ["schemas", "skills", "designer"] {
        copy_dir(&from.join(d), &to.join(d));
    }
    std::fs::copy(built, to.join(format!("bin/gmx-ingest{}", std::env::consts::EXE_SUFFIX))).unwrap();
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
    feed_with(port, "avenc_aac ! aacparse")
}

/// The same picture, with the sound encoded by `sound`.
pub fn feed_with(port: u16, sound: &str) -> gstreamer::Pipeline {
    gstreamer::init().unwrap();
    let text = format!(
        "videotestsrc is-live=true ! video/x-raw,format=I420,width=320,height=180,framerate=30/1 ! x264enc tune=zerolatency key-int-max=30 bitrate=600 ! h264parse ! mux. \
         audiotestsrc is-live=true ! audioconvert ! audioresample ! audio/x-raw,rate=48000 ! {sound} ! mux. \
         mpegtsmux name=mux alignment=7 ! udpsink host=127.0.0.1 port={port} sync=false"
    );
    let p = gstreamer::parse::launch(&text).expect("x264enc, avenc_aac and mpegtsmux are installed").downcast::<gstreamer::Pipeline>().unwrap();
    p.set_state(gstreamer::State::Playing).unwrap();
    p
}

/// Bytes that reach `socket` within `wait`, stopping once there are `enough`.
pub fn received(socket: &UdpSocket, wait: Duration, enough: usize) -> usize {
    socket.set_read_timeout(Some(Duration::from_millis(200))).unwrap();
    let (mut n, mut buf, until) = (0, vec![0u8; 65536], Instant::now() + wait);
    while Instant::now() < until && n < enough {
        if let Ok(got) = socket.recv(&mut buf) {
            n += got;
        }
    }
    n
}

pub fn free_udp() -> u16 {
    UdpSocket::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

async fn output_state(ws: &mut Ws, id: u64) -> Value {
    let s = call(ws, id, "show.stats", json!({"ids": ["feed"]})).await;
    s["result"]["shows"][0].clone()
}

/// The ingest plugin's process, the station's child, as `ps` reads it:
/// its pid and its CPU in thousandths of a core.
#[cfg(unix)]
fn ingest_process(station: u32) -> Option<(u32, u64)> {
    let out = Command::new("pgrep").args(["-P", &station.to_string(), "gmx-ingest"]).output().ok()?;
    let pid: u32 = String::from_utf8_lossy(&out.stdout).lines().next()?.trim().parse().ok()?;
    let out = Command::new("ps").args(["-o", "pcpu=", "-p", &pid.to_string()]).output().ok()?;
    let pcpu: f64 = String::from_utf8_lossy(&out.stdout).trim().parse().ok()?;
    Some((pid, (pcpu * 10.0).round() as u64))
}

/// The wall's header CPU counts the direct host. Before, `governor.status`
/// saw the station's own process and the reports of shows holding a ticket,
/// and the ingest plugin, where every direct show runs, holds none: 200
/// direct shows taking five cores read as nothing.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn governor_status_counts_the_direct_host_the_station_started() {
    let (dir, port) = folder("direct-host-cpu");
    let source = staged_ingest(&dir);
    let st = start(dir.clone(), port, &[]).await;
    let mut ws = rpc(&st, "").await;
    let input = free_udp();
    // Sixteen copies of one feed give the host work enough to measure.
    let sinks: Vec<UdpSocket> = (0..16).map(|_| UdpSocket::bind("127.0.0.1:0").unwrap()).collect();
    let outputs: Vec<Value> = sinks.iter().enumerate().map(|(n, s)| json!({"id": format!("out-{n}"), "uri": format!("udp://127.0.0.1:{}", s.local_addr().unwrap().port())})).collect();
    let pipeline = feed(input);
    let added = call(&mut ws, 1, "show.add", json!({"name": "Feed", "compositing": false, "input": {"uri": format!("udp://127.0.0.1:{input}")}, "outputs": outputs})).await;
    assert!(added.get("error").is_none(), "{added}");
    let asked = call(&mut ws, 2, "plugin.add", json!({"source": source.to_string_lossy()})).await;
    assert!(asked.get("error").is_none(), "{asked}");
    assert!(received(&sinks[0], Duration::from_secs(60), 100_000) >= 100_000, "the feed never reached an output");

    // A first start calibrates in the station's own process, which swamps
    // everything; its share decays over a few seconds once that is done.
    let settled = Instant::now() + Duration::from_secs(20);
    let limit = Instant::now() + Duration::from_secs(120);
    loop {
        let g = call(&mut ws, 3, "governor.status", json!({})).await;
        if g["result"]["calibrating"] == false && Instant::now() > settled {
            break;
        }
        assert!(Instant::now() < limit, "still calibrating: {g}");
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    let (pid, host) = ingest_process(st.pid()).expect("the station runs gmx-ingest as its child");
    let g = call(&mut ws, 4, "governor.status", json!({})).await;
    let used = g["result"]["cpu"]["measured_millicores"].as_u64().unwrap_or(0);
    eprintln!("gmx-ingest {pid} at {host} millicores, governor.status measured {used}");
    assert!(host >= 20, "sixteen outputs should cost the host something; ps read {host} millicores");
    assert!(used >= host * 3 / 4, "governor.status measures {used} millicores, under the direct host's {host}: {g}");
    pipeline.set_state(gstreamer::State::Null).unwrap();
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
    assert_eq!(stats["work"], "copy", "{stats}");

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
