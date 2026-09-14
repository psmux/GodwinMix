# AGENTS.md

For a coding agent changing this plugin.

## What this is

A GodwinMix source plugin in Rust that carries sound and no picture. One
process per instance, started by the core, JSON-RPC 2.0 on stdin and stderr,
media on whichever transport the handshake chose. It is a workspace member of
the GodwinMix repository, so `./build` stages the binary into `bin/` where the
manifest says it is.

## Build and test

```sh
./check                                       # everything that needs no core
./build                                       # stage bin/gmx-audio-device
gmx plugin test ./plugins/audio-device        # the core's conformance harness
```

`./check` needs no core, no network and no sound card: every path it exercises
forces `element = "audiotestsrc"`.

## Where things are

| Path | What it is |
|---|---|
| `src/settings.rs` | the schema as a struct, the decibel to linear conversion, and what a change costs |
| `src/pipeline.rs` | the element chain, the platform candidates, the ten millisecond buffers |
| `src/source.rs` | the `source` provide, including `audio.set` |
| `src/discover.rs` | the `devices` provide |
| `src/tools.rs` | `list_audio_inputs` |
| `../capture-common/` | what the four capture plugins share. Change it there |

## The rules that matter

1. **stdout is media.** Log through the `Reporter`, never `println!`.
2. **Gain and mute never reopen the device.** They are properties of a running
   `volume` element. A lectern microphone that went dead because someone typed
   a number is the failure this rule exists to prevent.
3. **`audio.set` leaves out what it was not given.** A mute must not move the
   fader, and a gain must not unmute.
4. **Ten milliseconds a buffer.** `latency-time` on the source asks the driver;
   `audiobuffersplit` makes it true when the driver will not. Do not remove
   either without measuring what the other alone produces.
5. **No meters.** Levels are the core's job, measured where the sound reaches
   the mix. A plugin measuring its own output tells an operator nothing.
6. **`configure` gets the full validated object.** The harness sends one
   property at a time, so every field falls back to its default.

## Changing it

* A new platform element: add it to `pipeline::CANDIDATES` behind the right
  `cfg!`, add it to the `element` enum in `schemas/source.json`, and say in the
  README which bug it is the fallback for.
* Layers: do not. `audio.set` refuses `layers` on purpose; page and media
  levels belong to a source with a document in it.
* Video: do not. A webcam's picture is a `camera/source`.

## What not to do

* Do not edit `tests/transcript.jsonl` to make a failing check pass.
* Do not point the transcript at a real sound card. It runs in CI.
