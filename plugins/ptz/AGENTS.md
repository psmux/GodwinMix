# AGENTS.md

For a coding agent changing this plugin. Read this before touching anything.

## What this is

A GodwinMix `service` plugin in Rust: no media of its own. It drives PTZ
cameras with the drivers in `crates/godwinmix-ptz`, and while asked it reads
a camera's picture and runs `crates/godwinmix-track`'s Follower to steer it.
Agents reach every tool through the core's `tool.call` as `ptz/<tool>`.

## Build and test

```sh
cargo test -p gmx-ptz          # tools, the joystick watchdog, presets, the follow loop, the transcript
cargo test -p godwinmix-ptz    # the drivers against their fakes
cargo bench -p gmx-ptz
cargo clippy --all-targets -p gmx-ptz -p godwinmix-ptz
```

## Where things are

| Path | What it is |
|---|---|
| `src/worker.rs` | one thread per camera that owns its driver; the joystick watchdog |
| `src/rig.rs` | every camera, from the settings and from `camera_add` |
| `src/follow/` | the follow loop's thread and what it answers |
| `src/frames/` | pictures for the tracker: the frame bus (unix) or the core's MJPEG |
| `src/presets.rs` | presets' names and thumbnails on disk |
| `src/tools/` | one module per group of tools |
| `src/service.rs` | the `Service` impl, and the stdin mode |
| `tests/common/` | a fake mixer serving a still and an MJPEG scene of a walking person |
| `tests/transcript.jsonl` | a recorded conversation, replayed by `tests/offline.rs` |

## The rules that matter

1. **A camera never keeps moving on its own.** `move` lasts the hold time and
   the worker stops the camera when no call renews it. Do not add a way to
   move a camera that does not end by itself, except `zoom` and the
   tracking, which have explicit off switches.
2. **The operator wins.** While the joystick is held, the follow loop's
   speeds are dropped in the worker. Keep that check there, not in the loop.
3. **Nothing runs unless asked.** The follow loop and its picture stream
   exist only while tracking is on or someone observed in the last 15 s.
4. **Every call to a camera goes through its worker.** Two threads talking to
   one camera's socket interleave replies.
5. **Tool names are underscores.** The plan's `track/observe` is
   `track_observe`; the core's `tool.call` adds `ptz/`.
6. Adding a tool: a function in `src/tools/`, its name in `tools::NAMES`, an
   entry in `gmx-plugin.toml` and two schemas in `schemas/tools/`.
   `tests/offline.rs` fails until all four agree.

## What not to do

* Do not put a driver call on the plugin's main thread; tool calls are
  answered one at a time and a slow camera would hold up every other.
* Do not edit `tests/transcript.jsonl` to make a failing check pass.
