//! A device opened once and read by every source that names it, in this
//! process and in another one, with real plugin processes and real GStreamer.
//!
//! The plugin is a shell script that declares `share` and writes a line to a
//! log each time it opens its "device", so the tests can count opens. A
//! second mixer is this test binary started again as a child, which is what a
//! show will be: another process on the same machine with the same registry.
//!
//! These need a shell and `gst-launch-1.0`, and skip where those are missing.

#![cfg(unix)]

mod shared;

use godwinmix_core::plugin::loader;
use gstreamer as gst;
use shared::*;
use std::path::PathBuf;
use std::time::Duration;

const CAMERA: Fixture = Fixture {
    script: r#"#!/bin/sh
say() { printf '%s\n' "$1" >&2; }
say '{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"plugin":"sharebars","version":"0.1.0","api":1,"transports":["container"],"provides":[]}}'
read -r _ready
say '{"jsonrpc":"2.0","method":"initialized"}'
started=0
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  case "$line" in
    *'"method":"start"'*)
      if [ "$started" = 0 ]; then
        echo "open $$" >> "$SHAREBARS_LOG"
        gst-launch-1.0 -q videotestsrc is-live=true pattern=ball \
          ! video/x-raw,format=I420,width=640,height=360,framerate=30/1 \
          ! matroskamux streamable=true ! fdsink fd=1 &
        started=1
      fi
      say "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"latency_ms\":0}}" ;;
    *'"method":"health"'*) say "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"state\":\"ok\"}}" ;;
    *'"method":"configure"'*) say "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"applied\":true}}" ;;
    *'"method":"stop"'*) say "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{}}" ;;
    *'"method":"shutdown"'*) kill %1 2>/dev/null; exit 0 ;;
  esac
done
kill %1 2>/dev/null
"#,
    manifest: r#"
[plugin]
name = "sharebars"
version = "0.1.0"
api = 1
description = "A pretend camera from a shell script, for the shared source tests."
license = "Apache-2.0"
platforms = ["linux-x86_64", "linux-aarch64", "macos-aarch64", "macos-x86_64"]
placements = ["sidecar"]
process = "per-instance"

[run]
shell = "run.sh"

[[provides]]
kind = "source"
id = "source"
media = { video = "container", audio = "none", alpha = false, thumb = true }
transports = ["container"]
capabilities = ["restart-in-place", "health"]
latency_ms = 0
settings = "settings.json"
share = { bus = "camera", params = ["device"] }
"#,
};

const TYPE: &str = "sharebars/source";

fn camera(id: &str) -> Running {
    open(TYPE, id, &[("device", "facetime")])
}

/// The second mixer: open the device, say so, count frames until killed or
/// until its stdin closes.
#[test]
fn child_entry() {
    let Ok(root) = std::env::var("SHARE_CHILD_ROOT") else { return };
    let root = PathBuf::from(root);
    let _ = gst::init();
    setup(&root, &CAMERA);
    let mut cam = camera("cam-b");
    let owner = until(Duration::from_secs(10), || cam.share()["owner"] == true);
    println!("RESULT ready=1 owner={}", u8::from(owner));
    let mut line = String::new();
    let _ = std::io::stdin().read_line(&mut line);
    println!("RESULT frames={}", cam.frames());
    cam.close();
    std::process::exit(0);
}

#[test]
fn two_sources_in_one_process_open_the_device_once_and_the_second_takes_over() {
    let _lock = exclusive();
    if skip() {
        return;
    }
    let _ = gst::init();
    let root = temp("one");
    env_for(&root);
    setup(&root, &CAMERA);

    let mut a = camera("cam-a");
    assert!(until(Duration::from_secs(10), || a.frames() > 10), "the first source shows a picture");
    let mut b = camera("cam-a2");
    assert!(until(Duration::from_secs(5), || b.frames() > 30), "the second reads the same picture");
    assert_eq!(opens(&root), 1, "the device was opened once for two sources");
    assert_eq!(a.share()["owner"], true);
    assert_eq!(b.share()["owner"], false);
    let (owner, reader) = (a.share(), b.share());
    println!("owner publishes in {}, reader holds it {} later", owner["publish_ms"], reader["hop_ms"]);
    // The owner's normaliser holds no frame: publishing takes well under one.
    let publish = owner["publish_ms"]["p50"].as_f64().expect("the owner timed its frames");
    assert!(publish < 10.0, "the feed held frames: {}", owner["publish_ms"]);
    assert!(reader["hop_ms"]["p50"].as_f64().unwrap() < 10.0, "{}", reader["hop_ms"]);

    b.video.gap_reset();
    let before = b.frames();
    a.close();
    assert!(
        until(Duration::from_secs(10), || b.frames() > before + 30),
        "the second source kept a picture after the first went"
    );
    assert_eq!(b.share()["owner"], true, "the reader took the device over");
    assert_eq!(opens(&root), 2);
    println!("handover gap in one process: {:?}", b.video.longest());
    assert!(b.video.longest() < Duration::from_secs(3), "gap {:?}", b.video.longest());
    b.close();
    loader::uninstall("sharebars").ok();
}

#[test]
fn a_mixer_killed_with_sigkill_hands_its_camera_to_the_one_reading_it() {
    let _lock = exclusive();
    if skip() {
        return;
    }
    let _ = gst::init();
    let root = temp("two");
    env_for(&root);
    let (mut child, mut lines) = spawn_child("child_entry", &root);
    assert_eq!(result(&mut lines), "ready=1 owner=1", "the first mixer owns the device");

    // This process is the second mixer. It must read, not open.
    setup(&root, &CAMERA);
    let mut b = camera("cam-b");
    assert!(until(Duration::from_secs(10), || b.frames() > 30), "the second mixer has a picture");
    assert_eq!(opens(&root), 1, "one open for two mixers");
    assert_eq!(b.share()["owner"], false);
    let hop = b.share()["hop_ms"].clone();
    println!("from the owner's publish to this process holding the frame: {hop}");
    assert!(hop["p50"].as_f64().unwrap() < 10.0, "{hop}");

    b.video.gap_reset();
    let before = b.frames();
    kill9(&mut child);
    assert!(
        until(Duration::from_secs(10), || b.frames() > before + 30),
        "the picture came back after the owner was killed"
    );
    let gap = b.video.longest();
    println!("handover gap across processes: {gap:?}, took over in {} ms", b.share()["last_start_ms"]);
    assert_eq!(b.share()["owner"], true);
    assert_eq!(opens(&root), 2);
    assert!(gap < Duration::from_secs(3), "gap {gap:?}");
    b.close();
    loader::uninstall("sharebars").ok();
}
