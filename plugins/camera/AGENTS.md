# AGENTS.md

For a coding agent changing this plugin. Read this before touching anything.

## What this is

A GodwinMix source plugin in Rust. One process per instance, started by the
core. Control is JSON-RPC 2.0 on stdin and stderr, one object per line, and
`godwinmix-sdk` runs that loop. Media is a GStreamer pipeline that ends at
whichever transport the handshake chose.

It is a workspace member of the GodwinMix repository, so `cargo build` at the
repository root builds it and its binary lands in the workspace target
directory, not here. `./build` copies it to `bin/gmx-camera`, which is where
`gmx-plugin.toml` says it is.

## Build and test

```sh
./check                                 # fmt, clippy, unit tests, manifest, offline replay, staged binary
./build                                 # stage bin/gmx-camera
gmx plugin test ./plugins/camera        # the core's conformance harness
gmx plugin test ./plugins/camera --offline
```

`./check` needs no core, no network and no camera: every path it exercises
forces `element = "videotestsrc"`.

## Where things are

| Path | What it is |
|---|---|
| `src/main.rs` | picks the handler from `GMX_PROVIDE` and runs the SDK loop |
| `src/settings.rs` | `schemas/source.json` as a struct, and what a change costs |
| `src/pipeline.rs` | the element chain, the platform candidates, opening the device |
| `src/source.rs` | the `source` provide |
| `src/discover.rs` | the `devices` provide |
| `src/tools.rs` | `list_cameras` |
| `../capture-common/` | what the four capture plugins share. Change it there, not here |
| `gmx-plugin.toml` | what this registers and how the core starts it |
| `schemas/` | the settings UI and the tool schemas. Every surface renders these |
| `skills/source/SKILL.md` | what an agent operating the mixer reads |
| `tests/transcript.jsonl` | a recorded conversation, replayed with no core |

## The rules that matter

1. **stdout is media.** A `println!` anywhere in this process corrupts the
   video stream when the container transport is in use. Log through the
   `Reporter` the SDK hands to `initialize`, which writes to stderr.
2. **Never block a streaming thread.** The bus watch has its own thread and the
   frame counter is one relaxed atomic add. Nothing else may run on a GStreamer
   thread.
3. **Say hello before opening the camera.** The handshake has five seconds and
   a cold camera can take one. `main` runs the SDK loop first and opens nothing
   until `start`.
4. **`health` must answer fast.** It reads two atomics and a mutex that is held
   for the length of a clone. Keep it that way.
5. **`configure` gets the full validated object, not a diff.** The harness
   sends one property at a time, so every field must fall back to its default.
6. **Do not set `device`, `device-index` or `device-path` by hand.** Ask
   `capture_common::devices::find` and let GStreamer's device provider
   configure the element. `elements::point_at` is the fallback and exists only
   for a forced `element`.
7. **Every error names the next step.** `RpcError::new(codes::…, message)` with
   a message that says what is wrong and what to do.

## Changing it

* A new setting: add it to `schemas/source.json` with a `description`, a
  `default` and at least one `examples` entry, read it in `Settings::from`, and
  decide in `needs_restart` whether it can change while the camera runs.
* A new platform element: add it to `pipeline::CANDIDATES` behind the right
  `cfg!`, and say in the README why it is there and which it falls back from.
* Audio: do not. A camera's microphone is an `audio-device/source`. Adding
  audio here would tie the two together for every operator who wants them
  apart.
* A new tool: add a `[[tools]]` block with input and output schemas and a
  description carrying one example, then handle the name in `tools::call_tool`.

## What not to do

* Do not edit `tests/transcript.jsonl` to make a failing check pass. If the
  behaviour changed on purpose, re-record it and say so in the commit message.
* Do not point the transcript at a real camera. It runs in CI.
* Do not add a dependency without a reason you can defend in one sentence.
