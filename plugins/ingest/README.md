# ingest

GodwinMix as a server.

Everywhere else the mixer dials out. Here it waits, and a phone, an OBS on the
other laptop, a hardware encoder in the rack or a guest in a browser dials in.
That is how a church or a small studio actually gets its pictures, and today it
means running a separate mediamtx beside the mixer.

| Provide | What it is |
|---|---|
| `ingest/rtmp` | an RTMP listener, in Rust, with no GStreamer element involved |
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
codec byte, the same AVC or AAC packet type. So turning a published stream into
something the core can open is a nine byte file header and an eleven byte header
per message. Nothing is parsed, nothing is re-timed, nothing is decoded here.

The core's container transport is `fdsrc ! decodebin`, which typefinds FLV and
picks `flvdemux`, so a stream that arrives here is decoded by exactly the code
that decodes one `rtmp/source` dialled out for. There is a test that captures a
real publisher's stream and runs `decodebin` over it, because a correct header
is not the same as an openable stream.

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

## What the core cannot do yet

`ingest/rtmp` and `ingest/whip` work: they are `source` provides, and sources go
through the plugin loader. `ingest/discover` and the `add_publishers` tool are
written to the contract and dormant, because the core is missing four things:

1. `crates/godwinmix-core/src/plugin/loader.rs` interns only provides whose
   `kind` is `"source"`, so a `device` provide registers nothing. `SidecarDevice`
   exists in `crates/godwinmix-core/src/plugin/host/service.rs` and is
   constructed nowhere.
2. Nothing calls `discover`. There is no `device.discover` in the method table,
   no CLI command and no timer.
3. A plugin's `event` notification is parsed by
   `crates/godwinmix-core/src/plugin/host/process.rs` into a bounded ring buffer
   that has no reader, so nothing can act on a publisher arriving.
4. `tool.call` is not a registered method, so even the MCP bridge's
   `POST /api/v1/tool/call` cannot reach a plugin's tool.

Until those are wired, the working path is an `ingest/rtmp` source that owns its
own port. Add it once, and every publisher who arrives is live within five
seconds with nothing else to configure, which is the acceptance line either way.
`docs/how-to/receive-a-phone-or-obs-stream.md` leads with that and says why.

## Settings

### `ingest/rtmp`

| Key | Default | What it does |
|---|---|---|
| `port` | `1935` | the TCP port. 1935 is what every encoder fills in by itself |
| `bind` | `0.0.0.0` | every interface. Give one address to listen on that one only |
| `app` | empty | the first part of the publish path. Empty takes any |
| `stream_key` | empty | the rest of it. Empty takes any; setting one is the closest RTMP has to a password |
| `relay` | empty | filled in by `ingest/discover` when it owns the port |

One source is one picture, so a second publisher is refused while the first is
live, with a message the publisher's own error box shows. For two at once, add a
second source on another port.

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
| `app` | empty | accept publishers on this application name only |

This device and an `ingest/rtmp` source cannot both hold 1935. Run one or the
other; the bind error names the other when they clash.

## Testing it

```sh
cargo test -p gmx-ingest                     # includes a real gst-launch publisher
gmx plugin test plugins/ingest --offline     # replay tests/transcript.jsonl
dev/harness/publish.sh rtmp 1935 &           # something to receive
gmx plugin test plugins/ingest               # the full harness
```

The harness's frame counting checks need a publisher, like any listener. With
nothing publishing they fail honestly.

## Where the rules come from

`docs/reference/plugin-manifest.md`, `docs/reference/plugin-protocol.md` and
`docs/reference/plugin-lifecycle.md`. The how to page is
`docs/how-to/receive-a-phone-or-obs-stream.md`.
