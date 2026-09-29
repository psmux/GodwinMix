# ingest

GodwinMix as a server.

Everywhere else the mixer dials out. Here it waits, and a phone, an OBS on the
other laptop, a hardware encoder in the rack or a guest in a browser dials in.
That is how a church or a small studio actually gets its pictures, and today it
means running a separate mediamtx beside the mixer.

| Provide | What it is |
|---|---|
| `ingest/rtmp` | an RTMP listener written in Rust |
| `ingest/whip` | a WHIP endpoint, so a browser needs nothing but the URL |
| `ingest/discover` | one RTMP port for many publishers, each reported as a source ready to add |

## In four minutes

```sh
dev/harness/stage-plugins.sh
gmx plugin add ./plugins/ingest
gmx source add phone --type ingest/rtmp
```

Then give somebody:

```
Server:     rtmp://<the mixer's address>:1935/live
Stream key: anything
```

In OBS that is Settings, Stream, Custom, those two boxes. The picture is live
within a second or two of their first keyframe, and nothing else is configured
at either end. `gmx take phone`.

`dev/harness/publish.sh rtmp 1935` publishes a test pattern to it from this
machine if you want to see it work without anybody else involved.

## Why a Rust RTMP server and not mediamtx

The roadmap allows either. `rml_rtmp` 0.8.0 (MIT) is sans-io: it parses chunks
and raises events and never touches a socket, so all the networking here is
`std::net::TcpListener` and a thread per connection. It is about 6,000 lines of
library code and it brings `byteorder`, `rml_amf0` and a second copy of the
`sha2` and `hmac` stack for the Flash Player 9 handshake. Eight small pure Rust
crates, no C, and nothing to supervise.

`mediamtx` is a 30 MB Go binary per platform, a second process, a YAML
configuration to keep in step, its own ports, and a download and signing story
for five platforms. It is also the thing this plugin exists to remove. On a
project whose first constraint is running on a Raspberry Pi, bundling it would
have made the mixer heavier than the tool it replaces.

The one thing it gives that this does not is Enhanced RTMP, which is HEVC and
AV1 over RTMP. If that becomes what people need, `rtmpx` is the crate to look at
again; today it wants Rust 1.97 and this workspace is on 1.82.

## How the stream reaches the core

An RTMP audio or video message carries exactly the body of an FLV tag: the same
codec byte, the same AVC or AAC packet type. So the RTMP half of this plugin
turns a published stream into FLV with a nine byte file header and eleven bytes
per message, and parses nothing.

That FLV is then remuxed to Matroska before it crosses to the core:

```
appsrc(FLV) ──► flvdemux ──┬─► h264parse ─┐
                           └─► aacparse ──┴─► matroskamux ──► fdsink fd=1
```

Two parsers and a muxer. No decode, no encode, no copy of a picture.

Handing the core the FLV directly would have been cheaper still, and it is what
this plugin did first. It does not work on macOS: `flvdemux` feeding the core's
`decodebin` makes it autoplug Apple's `vtdec_hw`, which negotiates GL backed
memory, and the core's normaliser works in system memory. The pipeline fails
with `not-negotiated`, the source never produces a frame, and nothing in the log
names the cause. The same stream in Matroska or MPEG-TS decodes on the same
machine with the same hardware decoder, which was measured with
`gst-launch-1.0` and no GodwinMix code involved. `src/remux.rs` carries the
whole finding.

The core could fix it instead, by putting the catalogue's `download` element
between `decodebin` and the normaliser on the sidecar container path, the way
the built in `rtmp/source` already does for its own decoder. That is the better
long term answer and it belongs in
`crates/godwinmix-core/src/plugin/host/source.rs`. Until it lands, every sidecar
plugin sending encoded video over the container transport wants Matroska or
MPEG-TS rather than FLV, and that is worth knowing.

There is a test that captures a real publisher's stream and runs `decodebin`
over what comes out, because a correct header is not the same as an openable
stream.

Bytes are held back until the first keyframe, so a decoder is never handed a run
of inter frames with nothing to decode them against.

## What is not here

**No SRT listener.** `srt/source` is one already, and `listener` is its default
mode. Two copies would mean two places to fix a bug:

```sh
gmx source add feed --type srt/source --params '{"port":9000}'
```

**No RTSP server.** `gstreamer-rtsp-server` needs `libgstrtspserver-1.0` at link
time on every platform, so adding it would make this whole plugin fail to build
where that library is absent, for people who only wanted RTMP. When the RTSP
server lands it should be its own plugin with its own platform list. Pulling
*from* an RTSP camera already works today with a bare `rtsp://` URL, and
pushing to somebody's RTSP server is `rtspclientsink`.

## The channel server

`ingest/discover` holds the mixer's RTMP port for every channel. The core runs
it as a singleton, hands it the channel table in its settings, hears its
`event/channel.*` notifications and asks it for the `streams` tool; a live
stream becomes an `ingest/rtmp` source reading the hub over loopback on the
same port. [The channel methods](../../docs/reference/channels.md) are the
core's side of it, and [Take streams from several encoders on one
port](../../docs/how-to/rtmp-channels.md) is the operator's.

With no channels it takes any publisher, as it always has, and the supervisor
makes each a source from its `event/ingest.publisher`.

## Settings

### `ingest/rtmp`

| Key | Default | What it does |
|---|---|---|
| `port` | `1935` | the TCP port. 1935 is what every encoder fills in by itself |
| `bind` | `0.0.0.0` | every interface. Give one address to listen on that one only |
| `app` | empty | the first part of the publish path. Empty takes any |
| `stream_key` | empty | the rest of it. Empty takes any; setting one is the closest RTMP has to a password |
| `relay` | empty | the channel server's address. Filled in when a channel's stream becomes a source |
| `stream` | empty | `<channel>/<stream>` to read from it. Filled in with `relay` |

One source on its own port is one picture, so a second publisher is refused
while the first is live, with a message the publisher's own error box shows.
For two at once, make a channel.

### `ingest/whip`

| Key | Default | What it does |
|---|---|---|
| `port` | `8889` | the port the endpoint's HTTP server listens on. What mediamtx used |
| `bind` | `0.0.0.0` | every interface |
| `path` | `/whip` | the part of the URL after the host |
| `stun_server` | empty | `stun://host:port`. Not needed on a local network |

### `ingest/discover`

| Key | Default | What it does |
|---|---|---|
| `rtmp_port` | `1935` | the port every publisher uses |
| `bind` | `0.0.0.0` | every interface |
| `app` | empty | with no channels, accept publishers on this application name only |

The channel table arrives from the core under `channels` and is never written
to the config. This device and an `ingest/rtmp` source cannot both hold one
port: give a source that owns its port another one.

## Testing it

```sh
cargo test -p gmx-ingest                     # includes a real gst-launch publisher
gmx plugin test plugins/ingest --offline     # replay tests/transcript.jsonl
dev/harness/publish.sh rtmp 1935 &           # something to receive
gmx plugin test plugins/ingest               # the full harness
```

The harness's frame counting checks need a publisher, like any listener. With
nothing publishing they fail honestly.

Two things to know about the full harness here.

It needs a publisher connected while the harness is playing, and the listener
only exists once the harness has started, so the publisher has to be retrying:

```sh
while :; do dev/harness/publish.sh rtmp 1935; sleep 0.3; done &
gmx plugin test plugins/ingest
```

With that, every check passes except check 6, the kill test. It measures the
programme's frame interval across a deliberate kill and wants it under 34 ms,
one frame; a listener cannot make that, because after the process restarts the
publisher has to connect again. The gap measured here is about 70 ms. The
documented remedy is to drop `restart-in-place` from the manifest, and it is the
wrong one: the capability does work, and dropping it would have the supervisor
rebuild the source from nothing instead, which is slower. `--quick` skips the
check.


## Where the rules come from

`docs/reference/plugin-manifest.md`, `docs/reference/plugin-protocol.md` and
`docs/reference/plugin-lifecycle.md`. The how to page is
`docs/how-to/receive-a-phone-or-obs-stream.md`.
