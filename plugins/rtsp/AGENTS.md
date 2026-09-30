# AGENTS.md

For a coding agent changing this plugin. Read this before editing anything.

## What this is

`rtsp/output`: the programme served over RTSP. One binary, one provide.
Built on `godwinmix-sdk`, `plugins/netkit` for the pipeline and bus watch,
`plugins/capture-common` for the programme FIFO, and gst-rtsp-server.

| File | What |
|---|---|
| `src/ingest.rs` | the feeder: FIFO, `matroskademux`, a parser per stream, an appsink |
| `src/feed.rs` | hands each encoded buffer to every live media's appsrc, retimed onto its clock |
| `src/server.rs` | the RTSP server on its own GLib main loop, the mount and the launch line |
| `src/output.rs` | the `Output` methods |
| `src/settings.rs` | port, path, bind |

## Rules

* Do not add an encoder. The programme's encode is the only one.
* Nothing may wait on a player. A media's appsrc is non blocking and drops
  when full; the feeder's appsinks drop rather than hold the demuxer.
* The media is shared (`set_shared(true)`): one packetiser for every player.

## Build and test

```sh
cargo test -p gmx-rtsp            # needs gst-launch-1.0 and ffmpeg for the pull tests
cargo clippy -p gmx-rtsp --all-targets
```
