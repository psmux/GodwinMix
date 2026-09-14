# AGENTS.md

For a coding agent changing this plugin.

## What this is

A GodwinMix source plugin in Rust that captures a monitor. One process per
instance, JSON-RPC 2.0 on stdin and stderr, media on the transport the
handshake chose. A workspace member of the GodwinMix repository; `./build`
stages the binary into `bin/` where the manifest says it is.

## Build and test

```sh
./check                              # everything that needs no core
./build                              # stage bin/gmx-screen
gmx plugin test ./plugins/screen     # needs screen recording permission on macOS
gmx plugin test ./plugins/screen --offline
```

`./check` and the offline transcript force `element = "videotestsrc"`, because
a real capture needs a permission a CI runner cannot grant.

## Where things are

| Path | What it is |
|---|---|
| `src/settings.rs` | the schema as a struct, including the region parser |
| `src/pipeline.rs` | the element chain, the platform candidates, the four spellings of a crop |
| `src/source.rs` | the `source` provide |
| `src/discover.rs` | the `devices` provide |
| `src/tools.rs` | `list_screens`, including the per platform permission note |
| `../capture-common/` | what the four capture plugins share |

## The rules that matter

1. **stdout is media.** Log through the `Reporter`, never `println!`.
2. **Desktop duplication only.** No capture hooks, no injected DLLs, no
   per game work. If someone asks for fullscreen game capture, the answer is
   borderless windowed mode, and the README says so.
3. **Say what permission is missing.** On macOS and Wayland a refused capture
   is a black picture and no error. `list_screens` carries a `note` for every
   platform, and `health` says the capture is producing nothing rather than
   reporting ok. Keep both true when you add a platform.
4. **Every setting except the label reopens the capture.** One behaviour on
   every platform is worth more than one saved freeze frame, because `show-cursor`
   is live on some elements and not on others.
5. **Set properties through `elements::set_*`.** Every capture element spells
   the same idea differently and setting one an element does not have aborts
   the process.
6. **No audio.** What comes out of the speakers is an `audio-device/source`.

## Changing it

* The portal handshake on Wayland: this is the big missing piece. It means
  D-Bus to `org.freedesktop.portal.ScreenCast`, a session held open for the
  life of the capture, and a PipeWire file descriptor received over that
  socket. Do not half do it: a partial handshake that leaves a session dangling
  is worse than the `node_id` setting that is there now.
* A new platform element: add it to `pipeline::CANDIDATES`, add its crop
  property names to `crop()`, add it to the `element` enum in the schema, and
  give it a line in `tools::note`.
* Window capture: the platforms disagree too much about what a window handle
  is. `region` over a maximised window is the portable answer, and it is what
  the SKILL says.

## What not to do

* Do not edit `tests/transcript.jsonl` to make a failing check pass.
* Do not point the transcript at a real screen. It runs in CI.
