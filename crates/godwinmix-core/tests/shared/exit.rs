//! The plugin `plugin_exit.rs` kills: the shell plugin the other sidecar
//! tests use, and the helpers that watch its process group.

use super::{setup, Fixture};
use std::path::Path;
use std::time::{Duration, Instant};

const SCRIPT: &str = r#"#!/bin/sh
say() { printf '%s\n' "$1" >&2; }
say '{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"plugin":"NAME","version":"0.1.0","api":1,"transports":["container"],"provides":[]}}'
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
    *'"method":"stop"'*) say "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{}}" ;;
    *'"method":"shutdown"'*) kill %1 2>/dev/null; exit 0 ;;
  esac
done
"#;

fn manifest(name: &str, share: bool) -> String {
    format!(
        r#"
[plugin]
name = "{name}"
version = "0.1.0"
api = 1
description = "A shell plugin killed from outside, for the exit tests."
license = "Apache-2.0"
platforms = ["linux-x86_64", "linux-aarch64", "macos-aarch64", "macos-x86_64"]
placements = ["sidecar"]
process = "per-instance"

[run]
shell = "run.sh"

[[provides]]
kind = "source"
id = "source"
media = {{ video = "container", audio = "none", alpha = false, thumb = true }}
transports = ["container"]
capabilities = ["restart-in-place", "health"]
latency_ms = 0
settings = "settings.json"
{}
"#,
        if share { r#"share = { bus = "camera", params = ["device"] }"# } else { "" }
    )
}

pub fn install(root: &Path, name: &str, share: bool) {
    let manifest: &'static str = Box::leak(manifest(name, share).into_boxed_str());
    let script: &'static str = Box::leak(SCRIPT.replace("NAME", name).into_boxed_str());
    setup(root, &Fixture { manifest, script });
}

/// Whether any process is left in the group `pgid` led.
pub fn group_alive(pgid: u32) -> bool {
    // SAFETY: signal 0 only asks whether the group exists.
    unsafe { libc::kill(-(pgid as i32), 0) == 0 }
}

/// Whether the group is empty within the teardown's own grace: a polite
/// SIGTERM first, so the helper may take a moment.
pub fn gone_within(pgid: u32) -> bool {
    let end = Instant::now() + Duration::from_secs(5);
    while group_alive(pgid) && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(20));
    }
    !group_alive(pgid)
}

/// SIGKILL to the plugin's own process and nothing else.
pub fn kill_plugin(pid: u32) {
    // SAFETY: a signal to a process this test's mixer started.
    unsafe { libc::kill(pid as i32, libc::SIGKILL) };
}
