//! A sidecar output whose plugin stops reading the programme.
//!
//! Caught by the scale harness on 2026-10-01: a udp/output plugin stopped
//! reading its FIFO, the core's `filesink` sat in a write that never returned,
//! the overflow watchdog's forced reconnect then sat in `set_state(Null)` on
//! the old pipeline for good, and the output said `live` for the rest of the
//! run while nothing went out. The plugin here is a shell script that reads
//! the FIFO until a marker file appears and then never reads again, which is
//! that fault without the reason for it. The output has to stop saying `live`
//! once bytes stop, restart the plugin, and send again.
//!
//! Needs a shell and GStreamer's x264enc and avenc_aac, and says so and skips
//! where they are missing.

#![cfg(unix)]

use godwinmix_core::config::OutputConfig;
use godwinmix_core::gstutil::BusEvent;
use godwinmix_core::output::OutputSlot;
use godwinmix_core::plugin::loader;
use godwinmix_core::state::OutputState;
use gstreamer as gst;
use gstreamer::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// An output plugin in shell. It reads the FIFO in the background and, once
/// the marker exists, stops reading for good, as the deadlocked sender did.
/// Asked to stop, it takes its reader with it, as a plugin in one process
/// does by exiting.
fn plugin(stall: &Path) -> String {
    format!(
        r#"#!/bin/sh
say() {{ printf '%s\n' "$1" >&2; }}
say '{{"jsonrpc":"2.0","id":0,"method":"initialize","params":{{"plugin":"stallout","version":"0.1.0","api":1,"transports":["container"],"provides":[]}}}}'
read -r _ready
say '{{"jsonrpc":"2.0","method":"initialized"}}'
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  case "$line" in
    *'"method":"start"'*)
      ( exec < "$GMX_MEDIA"
        while :; do
          if [ -e '{stall}' ]; then exec sleep 3600; fi
          n=$(head -c 65536 | wc -c)
          [ "$n" -eq 0 ] && exit 0
        done ) &
      reader=$!
      say "{{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{{}}}}" ;;
    *'"method":"health"'*) say "{{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{{\"state\":\"ok\"}}}}" ;;
    *'"method":"configure"'*) say "{{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{{\"applied\":true}}}}" ;;
    *'"method":"stop"'*) kill "$reader" 2>/dev/null; say "{{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{{}}}}" ;;
    *'"method":"shutdown"'*) kill "$reader" 2>/dev/null; exit 0 ;;
  esac
done
"#,
        stall = stall.display()
    )
}

const MANIFEST: &str = r#"
[plugin]
name = "stallout"
version = "0.1.0"
api = 1
description = "An output that stops reading, for the core's own tests."
license = "Apache-2.0"
platforms = ["linux-x86_64", "linux-aarch64", "macos-aarch64", "macos-x86_64"]
placements = ["sidecar"]
process = "per-instance"

[run]
shell = "run.sh"

[[provides]]
kind = "output"
id = "output"
uri_schemes = ["stallout://"]
rank = 210
media = { video = "container", audio = "container", alpha = false, thumb = false }
capabilities = ["health"]
settings = "settings.json"
"#;

fn write_plugin(at: &Path, stall: &Path) {
    std::fs::create_dir_all(at).unwrap();
    std::fs::write(at.join("gmx-plugin.toml"), MANIFEST).unwrap();
    std::fs::write(at.join("settings.json"), r#"{"type":"object","properties":{}}"#).unwrap();
    let entry = at.join("run.sh");
    std::fs::write(&entry, plugin(stall)).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&entry, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn temp(tag: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("gmx-stall-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    path
}

/// A programme the way the mixer makes one: live H.264 and AAC on two tees.
fn programme() -> Option<(gst::Pipeline, gst::Element, gst::Element)> {
    let line = "videotestsrc is-live=true pattern=ball ! video/x-raw,width=320,height=240,framerate=30/1 \
        ! x264enc tune=zerolatency key-int-max=30 bitrate=800 ! h264parse ! tee name=vtee allow-not-linked=true \
        audiotestsrc is-live=true ! avenc_aac ! aacparse ! tee name=atee allow-not-linked=true";
    let p = gst::parse::launch(line).ok()?.downcast::<gst::Pipeline>().ok()?;
    let (v, a) = (p.by_name("vtee")?, p.by_name("atee")?);
    Some((p, v, a))
}

fn bytes_out(slot: &OutputSlot) -> u64 {
    slot.status().extra.get("bytes_out").and_then(|v| v.as_u64()).unwrap_or(0)
}

/// What the mixer does every 500 ms for an output: read its state, and force
/// a reconnect, off its own thread, when the feed has been full too long.
fn tick(slot: &Arc<OutputSlot>) {
    slot.refresh_connected();
    if slot.tick_overflow_watchdog(6) && slot.claim_reconnect() {
        let s = slot.clone();
        std::thread::spawn(move || {
            if let Err(e) = s.reconnect() {
                eprintln!("a reconnect failed, and the next tick may try again: {e:#}");
            }
        });
    }
}

/// Tick until `done` says so, or panic with `what` at the deadline.
fn until(slot: &Arc<OutputSlot>, within: Duration, what: &str, mut done: impl FnMut(&OutputSlot) -> bool) {
    let deadline = Instant::now() + within;
    loop {
        tick(slot);
        if done(slot) {
            return;
        }
        assert!(Instant::now() < deadline, "{what}: state {:?}, {} bytes out", slot.state(), bytes_out(slot));
        std::thread::sleep(Duration::from_millis(500));
    }
}

#[test]
fn a_sidecar_output_that_stops_reading_is_restarted_and_never_called_live_while_silent() {
    let _ = gst::init();
    let Some((program, vtee, atee)) = programme() else {
        println!("skipping: needs x264enc and avenc_aac");
        return;
    };
    let root = temp("restart");
    let stall = root.join("stall");
    write_plugin(&root.join("checkout"), &stall);
    std::fs::create_dir_all(root.join("plugins")).unwrap();
    loader::set_dir(root.join("plugins"));
    loader::set_runtime_dir(root.join("run"));
    loader::install_from_path(&root.join("checkout")).expect("it installs");
    program.set_state(gst::State::Playing).unwrap();

    let mut cfg = OutputConfig::bare("stuck", "stallout://anywhere");
    cfg.type_id = Some("stallout/output".into());
    cfg.queue_secs = 2.0;
    let (bus_tx, bus_rx) = tokio::sync::mpsc::channel::<BusEvent>(512);
    let slot = OutputSlot::attach(&program, &vtee, &atee, &cfg, bus_tx).expect("the output attaches");

    until(&slot, Duration::from_secs(20), "the output never went live", |s| s.state() == OutputState::Live);
    let first = bytes_out(&slot);

    // The plugin stops reading. Within a few seconds the output must stop
    // saying it is live, whatever the plugin process says about itself.
    std::fs::write(&stall, b"").unwrap();
    until(&slot, Duration::from_secs(10), "still live after the plugin stopped reading", |s| {
        s.state() != OutputState::Live
    });
    // The process that stopped reading never reads again; a fresh one will.
    std::fs::remove_file(&stall).unwrap();

    // And come back: a reconnect that finishes, a new plugin, bytes again.
    // Throughout, live only ever while the count is moving.
    let mut last = bytes_out(&slot);
    until(&slot, Duration::from_secs(40), "the output never came back", |s| {
        let now = bytes_out(s);
        if s.state() == OutputState::Live {
            assert!(now > last, "said live with no new bytes since the last look ({now})");
        }
        last = now;
        s.state() == OutputState::Live && s.status().reconnects >= 1
    });
    let before = bytes_out(&slot);
    until(&slot, Duration::from_secs(10), "bytes did not keep moving after the restart", |s| bytes_out(s) > before + 10_000);
    assert!(bytes_out(&slot) > first);

    slot.detach(&program);
    let _ = program.set_state(gst::State::Null);
    drop(bus_rx);
    loader::uninstall("stallout").ok();
    let _ = std::fs::remove_dir_all(&root);
}
