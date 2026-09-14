# AGENTS.md

For a coding agent changing this plugin. Read this before editing anything.

## What this is

Three provides in one process image. `GMX_PROVIDE` says which this process is,
and `main` picks the handler from it:

| Provide | Kind | Module |
|---|---|---|
| `ingest/rtmp` | source | `src/source.rs` over `src/rtmp.rs` and `src/flv.rs` |
| `ingest/whip` | source | `src/whip_in.rs` |
| `ingest/discover` | device | `src/device.rs` over `src/rtmp.rs`, `src/relay.rs`, `src/rest.rs` |

The RTMP half is pure Rust: `rml_rtmp` parses chunks and raises events, this
plugin owns the sockets, and the published messages become FLV tags with a
twenty byte header each. No GStreamer element is involved in RTMP at all. The
WHIP half is one GStreamer element, `whipserversrc`.

## Build and test

```sh
cargo test -p gmx-ingest                     # includes a real gst-launch publisher
cargo clippy -p gmx-ingest --all-targets     # add no warning that was not there
dev/harness/stage-plugins.sh                 # build and stage plugins/ingest/bin/
gmx plugin test plugins/ingest --offline     # replay tests/transcript.jsonl
```

## The rules that matter

1. **stdout is media.** A `println!` anywhere in this process corrupts the
   stream. Log through the `Reporter`, which writes to stderr. `src/source.rs`
   takes the stdout handle for exactly this reason.
2. **Do not parse the media.** An RTMP message body is an FLV tag body. It goes
   through unchanged, and the core decodes it once. Adding a parser or a
   decoder here would pay for the decode twice and is the one change that would
   make this plugin expensive.
3. **A connection thread must not block on anything but its own socket.** The
   `Sink` closure is called from it. Writing to the pipe is what it does;
   waiting on a lock somebody else holds for long is not.
4. **A relay reader that is not keeping up is dropped, not waited for.** Nothing
   a plugin does may stall the media path. `Relay::send` retains only the
   clients whose write succeeded.
5. **Hold bytes back until the first keyframe.** `flv::is_keyframe` decides.
   Handing a decoder inter frames with no keyframe in front produces a grey
   picture and a lot of log noise, and it is the failure everyone blames on the
   encoder.
6. **Every refusal reaches the publisher.** A wrong application name or stream
   key is answered with `reject_request` and a sentence, because the publisher's
   own error box is the only place the person sending will look.
7. **No dependency without a reason written down.** This crate is the SDK,
   netkit, gstreamer, serde_json and `rml_rtmp`. The comparison against mediamtx
   is at the top of `src/rtmp.rs` and in the README; keep it true if it changes.
   `src/rest.rs` is a hand written HTTP client precisely so that a plugin does
   not carry a TLS stack and a runtime to call a process on the same machine.

## What the core cannot do yet

`ingest/discover` and `add_publishers` are dormant. The four gaps are listed at
the top of `src/device.rs` with the file that would change for each. Do not
delete the dormant code to make the warnings go away: it is what the core will
call when those land, and it is tested.

## What is deliberately not here

* No SRT listener: `srt/source` is one, and `listener` is its default mode.
* No RTSP server: `gstreamer-rtsp-server` needs `libgstrtspserver-1.0` at link
  time on every platform, which would break this plugin's build for people who
  only wanted RTMP. It belongs in its own plugin with its own platform list.
* No `onMetaData` script tag. `flvdemux` works from the AVC and AAC sequence
  headers, and rebuilding the AMF0 object would be work for nothing. If a
  downstream tool ever needs the metadata, `ServerSessionEvent::StreamMetadataChanged`
  is where it arrives.
* No Enhanced RTMP. `rml_rtmp` does not do HEVC or AV1 over RTMP, and the crate
  that does wants a newer Rust than this workspace.
