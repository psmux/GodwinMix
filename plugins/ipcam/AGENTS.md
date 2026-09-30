# AGENTS.md

For a coding agent changing this plugin. Read this before editing anything.

## What this is

`ipcam/source` and `ipcam/discover`. One binary; `GMX_PROVIDE` says which
provide a process serves.

| File | What |
|---|---|
| `src/camera.rs` | the MJPEG pipeline and the shared muxer tail |
| `src/snapshot.rs`, `src/fetch.rs` | a JPEG fetched every 1/fps through `souphttpsrc` |
| `src/source.rs` | the `Source` methods |
| `src/discover.rs` | the `Device` methods: ONVIF streams as `hls/source` candidates |
| `src/onvif/` | WS-Discovery, a SOAP POST, and the three ONVIF calls with WS-Security |

## Rules

* Do not decode. JPEGs cross to the core in Matroska as they came.
* An RTSP camera is the core's job (`hls/source`); discovery hands it the
  address and nothing else.
* Every network wait has a deadline.

## Build and test

```sh
cargo test -p gmx-ipcam
cargo clippy -p gmx-ipcam --all-targets
```
