# AGENTS.md

For a coding agent extending this plugin. Read this before changing anything.

## What this is

A GodwinMix source plugin in Rust, built on `godwinmix-sdk`. One process,
started by the core. Control is JSON-RPC 2.0 on stdin and stderr, one object
per line, and the SDK runs that loop. Media goes out over whichever transport
the core chose in the handshake; the default is a streamable Matroska stream on
stdout, which works everywhere.

## Build and test

```sh
./check            # fmt, clippy, build, unit tests, manifest, offline replay, your test
./check --quick    # the same without the test you have not written yet
cargo build --release
```

`./check` needs no core, no network and no GStreamer, and it finishes in under
30 seconds on a warm target directory. `gmx plugin test .` runs the core's
conformance harness on top of it once `gmx` is installed: real caps at the media
end, frame counts, a kill mid stream, and the footprint this plugin costs.

## Where things are

| Path | What it is |
|---|---|
| `src/main.rs` | the whole plugin. `draw()` is the part you change |
| `src/bin/check-manifest.rs` | runs the SDK's manifest validator over `gmx-plugin.toml` |
| `gmx-plugin.toml` | the manifest: what this plugin provides and how the core starts it |
| `schemas/source.json` | the settings, JSON Schema draft 2020-12. Every surface renders this |
| `skills/source/SKILL.md` | what an agent operating the mixer needs to know about this source |
| `tests/transcript.jsonl` | a recorded conversation with the core, replayed offline |
| `tests/offline.rs` | the replayer. Do not edit it to make a test pass |
| `tests/your_picture.rs` | the failing test. Replace it |
| `check` | everything above, in order |

## The rules that matter

1. **stdout is media.** A `println!` anywhere in this process corrupts the video
   stream. Log with the `Reporter` the SDK hands to `initialize`, which writes
   to stderr.
2. **`draw()` gets a buffer that is already the right size.** Fill it; do not
   resize it. An I420 frame is a Y plane of `width * height` bytes, then U and V
   planes of `ceil(width/2) * ceil(height/2)` each.
3. **`draw()` runs on the media thread.** No network calls, no file reads, no
   locks held for long, no allocation you can avoid. Do that work elsewhere and
   hand the result over through a channel or an `Arc<Mutex<_>>` you lock briefly.
4. **`health()` must answer fast.** The SDK answers it from the reader thread
   while `start` is still running, and that only works if the method itself does
   not block.
5. **`configure` gets the full validated object, not a diff.** The core has
   already checked it against `schemas/source.json`. Store it and use it.
6. **Return `Configure::restart_required(reason)` rather than lying.** A setting
   you cannot apply live is not a failure; saying it applied when it did not is.
7. **Every error names the next step.** Use `RpcError::new(codes::…, message)`
   with a message that says what is wrong and what to do, and attach `data` a
   caller can act on.

## Changing it

* A different picture: edit `draw()`. Nothing else.
* New settings: add them to `schemas/source.json` with a `description`, a
  `default` and at least one example, then read them in `Settings::from`.
  Do not build a settings UI; the schema is the UI.
* Audio as well as video: change `Streams::video_only` to
  `Streams::video_and_audio`, set `audio = "raw"` in the manifest's `media`
  table, and use `VideoLoop::spawn_with_audio`. Buffers are 10 ms of interleaved
  F32LE at 48 kHz, which is 3,840 bytes each.
* Alpha: declare `alpha = true` and the `alpha` capability in the manifest, and
  send `VideoFormat::Ayuv` instead of `I420`.
* Seeking: implement `seek` and `position` and add `"seek"` to `capabilities`.
  The SDK answers -32601 with a message naming the capability until you do.
* A cheaper transport: build with `--features gst` and add `"unixfd"` to
  `transports`. Unix only; the core falls back to the container elsewhere.
* A tool an agent can call: add a `[[tools]]` block to the manifest with an
  input schema and a description carrying one example call, then handle
  `tool.call` in the `call` method.

## What not to do

* Do not edit `tests/offline.rs` or `tests/transcript.jsonl` to make a failing
  check pass. If the behaviour changed on purpose, re-record the transcript and
  say so in the commit message.
* Do not write to stdout. See rule 1.
* Do not add a dependency without a reason you can defend in one sentence. The
  SDK itself depends on serde, serde_json and toml, and nothing else unless the
  `gst` feature is on.
