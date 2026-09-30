//! A stream with sound and pictures decoded once and read by every source that
//! names it, the way a channel stream is, with real processes.
//!
//! The plugin is a shell script shaped like `ingest/rtmp` reading a channel:
//! it declares `share = { bus = "channel", params = ["stream"], scope =
//! ["relay"] }` and sends Matroska with a picture and a tone. Each start is
//! logged, so the tests can count how many times the stream was decoded.

#![cfg(unix)]

mod shared;

use godwinmix_core::plugin::loader;
use gstreamer as gst;
use shared::*;
use std::path::PathBuf;
use std::sync::atomic::Ordering::Relaxed;
use std::time::Duration;

const CHANNEL: Fixture = Fixture {
    script: r#"#!/bin/sh
say() { printf '%s\n' "$1" >&2; }
say '{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"plugin":"sharechan","version":"0.1.0","api":1,"transports":["container"],"provides":[]}}'
read -r _ready
say '{"jsonrpc":"2.0","method":"initialized"}'
started=0
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  case "$line" in
    *'"method":"start"'*)
      if [ "$started" = 0 ]; then
        echo "open $$" >> "$SHAREBARS_LOG"
        gst-launch-1.0 -q matroskamux name=mux streamable=true ! fdsink fd=1 \
          videotestsrc is-live=true pattern=ball \
          ! video/x-raw,format=I420,width=640,height=360,framerate=30/1 ! queue ! mux. \
          audiotestsrc is-live=true wave=sine \
          ! audio/x-raw,format=S16LE,rate=48000,channels=2 ! queue ! mux. &
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
name = "sharechan"
version = "0.1.0"
api = 1
description = "A pretend channel stream from a shell script, for the shared source tests."
license = "Apache-2.0"
platforms = ["linux-x86_64", "linux-aarch64", "macos-aarch64", "macos-x86_64"]
placements = ["sidecar"]
process = "per-instance"

[run]
shell = "run.sh"

[[provides]]
kind = "source"
id = "source"
media = { video = "container", audio = "container", alpha = false, thumb = true }
transports = ["container"]
capabilities = ["restart-in-place", "health"]
latency_ms = 0
settings = "settings.json"
share = { bus = "channel", params = ["stream"], scope = ["relay"] }
"#,
};

const TYPE: &str = "sharechan/source";

fn stream(id: &str, relay: &str) -> Running {
    open(TYPE, id, &[("stream", "sunday/main"), ("relay", relay)])
}

/// Both tracks flowing, and on one timeline: the newest picture and the newest
/// sound are within a few frames of each other, not seconds apart.
fn in_step(r: &Running) -> bool {
    let (v, a) = (r.video.last_pts.load(Relaxed), r.audio.last_pts.load(Relaxed));
    r.video.frames() > 30 && r.audio.frames() > 30 && v.abs_diff(a) < 150_000_000
}

#[test]
fn channel_child_entry() {
    let Ok(root) = std::env::var("SHARE_CHILD_ROOT") else { return };
    let root = PathBuf::from(root);
    let _ = gst::init();
    setup(&root, &CHANNEL);
    let mut s = stream("show-b", "127.0.0.1:1935");
    let owner = until(Duration::from_secs(10), || s.share()["owner"] == true);
    println!("RESULT ready=1 owner={}", u8::from(owner));
    let mut line = String::new();
    let _ = std::io::stdin().read_line(&mut line);
    s.close();
    std::process::exit(0);
}

#[test]
fn two_sources_of_one_stream_decode_it_once_with_sound_and_pictures_together() {
    let _lock = exclusive();
    if skip() {
        return;
    }
    let _ = gst::init();
    let root = temp("chan-one");
    env_for(&root);
    setup(&root, &CHANNEL);

    let a = stream("show-a", "127.0.0.1:1935");
    let mut b = stream("show-a2", "127.0.0.1:1935");
    assert!(until(Duration::from_secs(10), || in_step(&a) && in_step(&b)), "both read both tracks");
    assert_eq!(opens(&root), 1, "one decode for two sources");
    assert_eq!(b.share()["bus"], "channel:sunday/main");
    let (v, s) = (b.video.last_pts.load(Relaxed), b.audio.last_pts.load(Relaxed));
    println!("the reader's newest picture and sound are {} ms apart", v.abs_diff(s) / 1_000_000);

    // Another channel server's `sunday/main` is another stream.
    let c = stream("show-c", "127.0.0.1:1936");
    assert!(until(Duration::from_secs(10), || in_step(&c)), "the other server's stream plays");
    assert_eq!(opens(&root), 2, "a different relay is a different stream");

    for r in [a, b, c] {
        r.close();
    }
    loader::uninstall("sharechan").ok();
}

#[test]
fn a_reader_of_a_stream_keeps_sound_and_pictures_when_the_owner_is_killed() {
    let _lock = exclusive();
    if skip() {
        return;
    }
    let _ = gst::init();
    let root = temp("chan-two");
    env_for(&root);
    let (mut child, mut lines) = spawn_child("channel_child_entry", &root);
    assert_eq!(result(&mut lines), "ready=1 owner=1");

    setup(&root, &CHANNEL);
    let mut b = stream("show-b", "127.0.0.1:1935");
    assert!(until(Duration::from_secs(10), || in_step(&b)), "the reader has both tracks");
    assert_eq!(opens(&root), 1);

    b.video.gap_reset();
    b.audio.gap_reset();
    let (v0, a0) = (b.video.frames(), b.audio.frames());
    kill9(&mut child);
    let back = until(Duration::from_secs(10), || {
        b.video.frames() > v0 + 30 && b.audio.frames() > a0 + 30 && in_step(&b)
    });
    assert!(back, "both tracks came back together after the owner was killed");
    println!(
        "handover gap: pictures {:?}, sound {:?}",
        b.video.longest(),
        b.audio.longest()
    );
    assert_eq!(b.share()["owner"], true);
    assert_eq!(opens(&root), 2);
    b.close();
    loader::uninstall("sharechan").ok();
}
