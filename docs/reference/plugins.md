# The first party plugins

Everything in `plugins/` in this repository: one directory per plugin, each a
Rust crate on `godwinmix-sdk`, each installable with
`gmx plugin add ./plugins/<name>`.

They use only the public sidecar contract. Nothing here is reachable by this
repository's code that is not reachable by yours, which is the rule that keeps
the ecosystem honest ([why the UI is a client](../explanation/why-the-ui-is-a-client.md)
makes the same argument about the UI).

## The table

| Plugin | Provides | Platforms | Transport | Status |
|---|---|---|---|---|
| [camera](../../plugins/camera/README.md) | `camera/source`, `camera/devices`, tool `list_cameras` | Linux, macOS, Windows | `unixfd`, else container | conformant; verified on macOS |
| [audio-device](../../plugins/audio-device/README.md) | `audio-device/source`, `audio-device/devices`, tool `list_audio_inputs` | Linux, macOS, Windows | container | conformant; verified on macOS |
| [screen](../../plugins/screen/README.md) | `screen/source`, `screen/devices`, tool `list_screens` | Linux, macOS, Windows | `unixfd`, else container | conformant; verified on macOS. The Wayland portal handshake is not implemented |
| [file-record](../../plugins/file-record/README.md) | `file-record/output`, tool `list_recordings` | Linux, macOS | container, on a FIFO | conformant; a ten second recording verified on macOS. Not Windows |

"Conformant" means `gmx plugin test ./plugins/<name>` passes every check it
runs on that machine. See [test a plugin](../how-to/test-a-plugin.md) for what
each check measures.

## What each one is for

### camera

A USB camera, a built in laptop camera or a capture card. `v4l2src` on Linux,
`avfvideosrc` on macOS, `mfvideosrc` on Windows with `ksvideosrc` behind it
because `mfvideosrc` has an open startup bug where some cameras never produce a
first frame.

Devices are opened through GStreamer's own device provider, so the plugin never
has to know whether this platform's element wants `device`, `device-index` or
`device-path`. Picture only: a camera's microphone is a separate
`audio-device/source`, which is what lets an operator take the picture from one
place and the sound from another.

### audio-device

A microphone, a line input, a mixing desk feed. `pipewiresrc` then `pulsesrc`
then `alsasrc` on Linux, `osxaudiosrc` on macOS, `wasapi2src` on Windows with
`directsoundsrc` behind it for the open stutter bugs. Gain and mute move
through `audio.set` on a running `volume` element, so a fader never reopens the
device.

It declares the container transport and not `unixfd`, on purpose: the socket
transport passes one file descriptor per buffer, which is what makes it free
for 93 MB/s of video and wrong for 384 kB/s of sound in ten millisecond pieces.

Meters are the core's, measured where the sound reaches the mix. The plugin
computes none.

### screen

A monitor or a rectangle of one. The PipeWire portal's node id or `ximagesrc`
on Linux, `avfvideosrc` with `capture-screen` on macOS,
`d3d11screencapturesrc` on Windows. Desktop duplication only: an exclusive
fullscreen game shows as black and the answer is borderless windowed mode.

`list_screens` says which capture element this machine would use and what
permission is outstanding, because on macOS and Wayland a refused capture is a
black picture with no error.

### file-record

The programme on the disk. A remux rather than a second encode, so it costs
about as much CPU as copying a file. Fragmented MP4 by default so a crash
leaves a file that still plays, Matroska as the alternative, a path pattern
with the date and time, splitting by the clock, and a disk space check that
reports degraded and keeps recording.

Linux and macOS only. An output plugin receives the programme on a FIFO and
Windows has none; the core refuses a sidecar output there. See
[the plugin lifecycle](plugin-lifecycle.md).

## What the core does not do with them yet

Three gaps, none of them in the plugins. They are worth knowing before you
build on the same contract.

| Gap | What it means | Where |
|---|---|---|
| Only `source` provides are registered | a `device` or `output` provide in a manifest registers nothing, so `camera/devices` and `file-record/output` cannot be reached through the core today | `crates/godwinmix-core/src/plugin/loader.rs`, `intern_all` |
| `discover` has no caller | the `device` provides answer it correctly and nothing asks | `crates/godwinmix-core/src/plugin/host/service.rs` |
| `tool.call` has no route | the MCP server resolves `gmx_<plugin>_<tool>` and posts to `/api/v1/tool/call`, which is not a registered method | `crates/godwinmix/src/mcp.rs` |
| Only a `source` can be placed on a node | `place = "node:<name>"` builds a remote source; an output, a filter or a service on a node parses and is validated and has no host behind it yet | `crates/godwinmix-core/src/plugin/host/bridged.rs` |

Every plugin here implements its side of all three, so they work the day the
core's side lands. `gmx plugin test --offline` exercises `tool.call` against
each of them now.

## Running one on another machine

Every plugin here declares `placements = ["sidecar", "node"]`, which means it
can run on this machine or on a node. Nothing in the plugin changes: install it
on the node, and place the source there.

```toml
[[sources]]
id = "cam1"
type = "srt/source"
place = "node:studio-b"
latency_ms = 150
```

`type` names a plugin installed on the node, not on the core. The core reads
its manifest and its settings schema out of the node's hello, so the settings
form, the tools and the health are the ones the plugin declares, and
`plugin.list` shows it with `root` naming the node it is on.

A plugin whose manifest does not declare `node` is refused that placement with
error `-32005`, listing the placements it did declare. See
[Nodes](nodes.md) and
[Add a second machine](../how-to/add-a-node.md).

## Building and installing

```sh
./plugins/<name>/build           # compile and stage bin/<binary>
gmx plugin add ./plugins/<name>  # copy and register, live
gmx plugin list
gmx plugin remove <name>
```

`./build` is needed because these are members of the repository's cargo
workspace, so their binaries land in the workspace target directory, and
`gmx plugin add` copies the plugin directory while skipping anything called
`target`. The manifest points at `bin/<binary>`, and `./build` is what puts it
there. `gmx plugin add` does not run `[build]` itself today.

Remove a source before removing the plugin that provides it: removing a plugin
does not stop instances that are using it.

## See also

* [Install a plugin](../how-to/install-a-plugin.md)
* [Test a plugin](../how-to/test-a-plugin.md)
* [The plugin manifest](plugin-manifest.md)
* [The plugin lifecycle](plugin-lifecycle.md)
* [Write a source plugin in Rust](../how-to/write-a-source-plugin.md)
