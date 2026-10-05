# AGENTS.md

For a coding agent changing this plugin. Read this before editing anything.

## What this is

Three provides in one process image. `GMX_PROVIDE` says which this process is,
and `main` picks the handler from it:

| Provide | Kind | Module |
|---|---|---|
| `ingest/rtmp` | source | `src/source.rs` over `src/rtmp.rs`, `src/flv.rs` and `src/remux.rs` |
| `ingest/whip` | source | `src/whip_in.rs` |
| `ingest/discover` | device | `src/device.rs` over `src/listeners.rs`, `src/rtmp.rs`, `src/srt.rs`, `src/whip.rs`, `src/gate.rs`, `src/channels.rs`, `src/hub.rs`, `src/relay.rs`, `src/rest.rs`, and the direct host for shows with compositing off, `src/direct/` (`docs/explanation/direct-host.md`) |

The RTMP half is pure Rust: `rml_rtmp` parses chunks and raises events, this
plugin owns the sockets, and the published messages become FLV tags with a
twenty byte header each. That FLV is then remuxed to Matroska by two parsers and
a muxer, with nothing decoded; `src/remux.rs` says at length why the FLV cannot
go to the core as it is. The WHIP half is one GStreamer element,
`whipserversrc`.

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
2. **Do not decode the media.** An RTMP message body is an FLV tag body and
   stays one all the way to `matroskamux`. `h264parse` and `aacparse` are there
   because a muxer needs framing, not because anything is being examined.
   Adding a decoder here would pay for the decode twice and is the one change
   that would make this plugin expensive. One exception is `src/transcode/`,
   which decodes a channel stream only while a destination has asked for a
   rendition the stream is not, once per stream however many destinations
   convert it, and builds exactly the nodes the core's plan hands over in the
   channel table. A destination with no rendition never goes near it. Another
   is `src/whip/session/vp8.rs`, which decodes VP8 from a WHIP publisher
   that offers no H.264 and encodes it as H.264, for that publisher alone;
   H.264 comes first in every answer, so a browser that can send it does.
   The last is `src/direct/vitals/`, which decodes keyframes alone, at most one
   a second per show, and three sound frames a second, through two shared
   worker threads, and only while a show's alarms are on or somebody is
   looking at it (`docs/reference/show-health.md`).
3. **A connection thread must not block on anything but its own socket.** The
   `Gate` and each `Inlet` are called from it. Handing a tag on is what it
   does; waiting on a lock somebody else holds for long is not. No lock is
   held across I/O anywhere in the hub.
4. **A reader that is not keeping up loses GOPs, it is never waited for.**
   Nothing a plugin does may stall the media path. The hub's queues are
   bounded and drop whole GOPs from the front; the loss is counted in
   `dropped_gops`. `hub::tests::a_blocked_reader_does_not_slow_the_publisher_or_the_other_reader`
   holds that to a number.
5. **Hold bytes back until the first keyframe.** `codec::is_keyframe` decides.
   Handing a decoder inter frames with no keyframe in front produces a grey
   picture and a lot of log noise, and it is the failure everyone blames on the
   encoder.
6. **Every refusal reaches the publisher.** A wrong application name or stream
   key is answered with `reject_request` and a sentence, because the publisher's
   own error box is the only place the person sending will look.
7. **No dependency without a reason written down.** This crate is the SDK,
   netkit, gstreamer, serde_json and `rml_rtmp`, with `libloading` for libsrt
   and `gstreamer-webrtc` and `gstreamer-sdp` for WHIP; `Cargo.toml` says why. The comparison against mediamtx
   is at the top of `src/rtmp.rs` and in the README; keep it true if it changes.
   `src/rest.rs` is a hand written HTTP client precisely so that a plugin does
   not carry a TLS stack and a runtime to call a process on the same machine.

## The channel server

`ingest/discover` is the channel server: one port per protocol, many
channels, many streams on each (`dev/plans/channels-contract.md`,
`dev/plans/shows-and-renditions.md`). No port opens until a channel that is
switched on uses it; `src/listeners.rs` opens and closes them each time the
table arrives. RTMP and RTMPS are `src/rtmp.rs` (TLS in `src/rtmp/io.rs`),
SRT is libsrt loaded at run time (`src/srt.rs`), and WHIP sessions are
`webrtcbin` (`src/whip.rs`), their offers handed over by the core from its
control port. SRT and WHIP media become the same `MediaTag`s RTMP does
(`src/tagger.rs`), parsed and never decoded, except WHIP's Opus sound, which
is turned into AAC. The core owns channels and
hands this process its table as `channels` in the settings; `src/channels.rs`
decides who is let in, `src/gate.rs` tells the core, and `src/hub.rs` carries
each stream to its readers:

* a publisher pushes each tag once, as a `MediaTag` whose payload is an
  `Arc<[u8]>`, and never waits on a reader;
* every reader has a bounded queue (`src/hub/queue.rs`); one that falls behind
  loses whole GOPs from the front, counted, and starts again at a keyframe
  with the headers in front;
* a late joiner gets `onMetaData` and the two sequence headers before its
  first keyframe.

`src/media_tag.rs` is shared with the restreamer in `src/restream/` and its
content is fixed by the contract. Change it there first.

A mixer source reads its stream from the hub over loopback on the RTMP port
itself (`src/relay.rs`). The relayed source exits when its publisher leaves,
on purpose: `src/source/relayed.rs` says why.

## What is deliberately not here

* No SRT source for one feed without a channel: `srt/source` is that.
* No RTSP server: `gstreamer-rtsp-server` needs `libgstrtspserver-1.0` at link
  time on every platform, which would break this plugin's build for people who
  only wanted RTMP. It belongs in its own plugin with its own platform list.
* No copy of the publisher's own `onMetaData` bytes. `rml_rtmp` parses them
  into a struct and keeps nothing else, so `flv::metadata_body` writes the
  object again with the AMF0 encoder `rml_rtmp` already carries, for a late
  reader and for a restream.
* No Enhanced RTMP. `rml_rtmp` does not do HEVC or AV1 over RTMP, and the crate
  that does wants a newer Rust than this workspace.
