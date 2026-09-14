# The network plugins

Four first party plugins carry media over a network: `srt`, `whip`, `ingest` and
`ndi`. Every setting they have is here.

If `docs/reference/plugins.md` exists in your checkout, it is the index of every
first party plugin and these rows belong in it; this page is where they live
until then.

## Every provide

| Id | Kind | Plugin | What it does | Works today |
|---|---|---|---|---|
| `srt/source` | source | `srt` | receive SRT, caller or listener | yes |
| `srt/output` | output | built into the core, not a plugin | send MPEG-TS over SRT | yes |
| `whip/output` | output | `whip` | send the programme to a WHIP endpoint | needs the core to load plugin outputs |
| `whip/whep` | source | `whip` | receive a stream over WHEP | yes |
| `ingest/rtmp` | source | `ingest` | listen for an RTMP publisher | yes |
| `ingest/whip` | source | `ingest` | run a WHIP endpoint publishers send to | yes |
| `ingest/discover` | device | `ingest` | one RTMP port for many publishers | needs the core to load device provides |
| `ndi/source` | source | `ndi` | receive an NDI sender | yes, with the NDI runtime |
| `ndi/output` | output | `ndi` | announce the programme as an NDI sender | needs the core to load plugin outputs |
| `ndi/discover` | device | `ndi` | list NDI senders, `list_senders` | needs the core to load device provides |

### What "needs the core" means, precisely

* **Plugin outputs.** `crates/godwinmix-core/src/plugin/output.rs` resolves an
  output type against a static list of the built in ones and never asks the
  loader. `crates/godwinmix-core/src/plugin/source.rs` already does the
  equivalent lookup for sources; the same `.or_else(|| loader::…)` in the output
  registry is what is missing.
* **Device provides.** `crates/godwinmix-core/src/plugin/loader.rs` interns only
  provides whose `kind` is `"source"`. `SidecarDevice` exists in
  `crates/godwinmix-core/src/plugin/host/service.rs` and is constructed nowhere,
  and no RPC method, CLI command or timer calls `discover`.
* **Plugin tools.** `tool.call` is not a registered method, so neither the MCP
  bridge's `POST /api/v1/tool/call` nor a direct RPC call reaches a plugin's
  tool.
* **Auto add from an event.** A plugin's `event` notification is parsed by
  `crates/godwinmix-core/src/plugin/host/process.rs` into a bounded ring buffer
  with no reader. Nothing turns a notification into `source.add`.

## `srt/source`

Receive SRT. Sending is `srt/output`, built into the core.

| Key | Type | Default | Meaning |
|---|---|---|---|
| `mode` | `listener`, `caller`, `rendezvous` | `listener` | wait, dial out, or both at once through a firewall |
| `host` | string | `0.0.0.0` | the interface to wait on, or the sender to dial |
| `port` | integer 0 to 65535 | `9000` | the **UDP** port |
| `uri` | string | empty | a whole `srt://` address; wins over `host` and `port`, and anything already in its query string is left alone |
| `latency_ms` | integer 0 to 10000 | `125` | the delay budget for retransmission |
| `passphrase` | string, `format: secret` | empty | 10 to 79 characters, the same at both ends |
| `stream_id` | string | empty | the name the sender publishes under |
| `auto_reconnect` | boolean | `true` | dial again by itself when the link drops |

`capabilities`: `restart-in-place`, `health`, `latency-report`.
`uri_schemes`: `srt://` at rank 240, above `hls/source`'s 200.
Media: `container` both ways, so the MPEG-TS crosses as it arrived and the core
decodes it once.

`health` carries the link numbers once packets flow: `rtt 14.2 ms, 31 lost,
4 retransmitted`. A `stats` call returns the same as JSON. No
`keyframe-request`: SRT has no back channel a receiver can ask on.

## `whip/output` and `whip/whep`

Both read the same keys, because WHIP and WHEP are the same protocol pointed in
opposite directions.

| Key | Type | Default | Meaning |
|---|---|---|---|
| `endpoint` | string | empty | the WHIP or WHEP URL. Required before `start`, not before `initialize` |
| `token` | string, `format: secret` | empty | sent as `Authorization: Bearer` |
| `stun_server` | string | empty | `stun://host:port` |
| `turn_server` | string, `format: secret` | empty | `turn://user:password@host:port`, or `turns://` |
| `timeout_secs` | integer 0 to 300 | `15` | how long to wait for the POST to be answered |
| `reconnect_first_ms` | integer 100 to 60000 | `500` | output only: the first wait before dialling again |
| `reconnect_max_ms` | integer 100 to 300000 | `15000` | output only: where the doubling stops |

The output passes the programme's H.264 through untouched and decodes only the
audio, because WebRTC has no AAC. A programme in another video codec is refused
with a message naming `codecs.toml` rather than being re-encoded.

`whip/whep` prefers `whepsrc` over its replacement `whepclientsrc`: the first
hands over encoded streams, the second decodes inside itself and costs a decode
here plus a much fatter pipe. GStreamer 1.28 deprecates the first; the plugin
falls back automatically when it is gone.

## `ingest/rtmp`

The mixer listens and the publisher dials in. Written in Rust over `rml_rtmp`;
no GStreamer element is involved.

| Key | Type | Default | Meaning |
|---|---|---|---|
| `port` | integer 0 to 65535 | `1935` | the TCP port |
| `bind` | string | `0.0.0.0` | the interface |
| `app` | string | empty | the first part of the publish path. Empty takes any |
| `stream_key` | string, `format: secret` | empty | the rest of it. Empty takes any |
| `relay` | string | empty | filled in by `ingest/discover` when it owns the port |

One source is one picture, so a second publisher is refused while the first is
live and the refusal reaches the publisher's own error box. Bytes are held back
until the first keyframe.

The stream crosses to the core as FLV: an RTMP audio or video message carries
exactly the body of an FLV tag, so it costs a nine byte file header and eleven
bytes per message and nothing else.

## `ingest/whip`

The mixer runs the WHIP server, so a browser publishes with nothing but a URL.

| Key | Type | Default | Meaning |
|---|---|---|---|
| `port` | integer 0 to 65535 | `8889` | the port the endpoint's HTTP server listens on |
| `bind` | string | `0.0.0.0` | the interface |
| `path` | string | `/whip` | the part of the URL after the host, beginning with a slash |
| `stun_server` | string | empty | `stun://host:port` |

Needs `whipserversrc`, from the GStreamer 1.28 rs webrtc set. Without it the
source refuses at `initialize` with a message naming the package and pointing at
`ingest/rtmp`.

## `ingest/discover`

| Key | Type | Default | Meaning |
|---|---|---|---|
| `rtmp_port` | integer 0 to 65535 | `1935` | the port every publisher uses |
| `bind` | string | `0.0.0.0` | the interface |
| `app` | string | empty | accept publishers on this application name only |

`discovery = { mdns = ["_rtmp._tcp"] }`. One publisher becomes one candidate of
type `ingest/rtmp` whose params carry a loopback `relay` address. It also pushes
`event/ingest.publisher`:

```json
{"action": "connected", "id": "live-phone", "type": "ingest/rtmp",
 "name": "live/phone", "peer": "10.0.0.31:51666", "params": {"relay": "127.0.0.1:54321"}}
```

and `{"action": "left", "id": "live-phone", "name": "live/phone"}`.

### Tool: `add_publishers`

| Argument | Type | Default | Meaning |
|---|---|---|---|
| `dry_run` | boolean | `false` | answer with the plan and change nothing |

Adds an `ingest/rtmp` source for every publisher that has none, removes the ones
it added whose publisher has gone, and never touches a source somebody else
made. It calls the core's own REST layer (`POST /api/v1/sources`,
`DELETE /api/v1/sources/{id}`) with `GMX_TOKEN`, which therefore has to carry
the `operate` scope; the error says so when it does not. Plain HTTP only: a
plugin carrying a TLS stack to reach a process on the same machine is not worth
the weight, and the refusal says that too.

Annotations: `readOnlyHint = false`, `destructiveHint = false`,
`idempotentHint = true`, `openWorldHint = true`.

## `ndi/source`, `ndi/output` and `ndi/discover`

The NDI runtime is `dlopen`ed and never linked. Its licence forbids
redistribution; NDI is a registered trademark of Vizrt Group and GodwinMix is
not affiliated with or endorsed by Vizrt. Download the runtime from
<https://ndi.video/for-developers/ndi-sdk/>.

`ndi/source`:

| Key | Type | Default | Meaning |
|---|---|---|---|
| `name` | string | empty | the NDI name as announced, `STUDIO (CAM 1)` |
| `address` | string | empty | `host:port`, for a sender mDNS cannot reach |
| `bandwidth` | `high`, `low`, `audio-only` | `high` | `low` is a much smaller picture at a fraction of the bandwidth |
| `timestamp_mode` | see below | `receive-time-vs-timecode` | which clock frames are stamped with |

`timestamp_mode` is one of `receive-time-vs-timecode`,
`receive-time-vs-timestamp`, `timecode`, `timestamp`, `receive-time`.

`ndi/output`:

| Key | Type | Default | Meaning |
|---|---|---|---|
| `name` | string | `GodwinMix` | what the rest of the network sees this mixer called |

`ndi/discover` has no settings. Discovery is GStreamer's own NDI device
provider, listening for `_ndi._tcp`, rather than an mDNS client of this
plugin's: one implementation, and no second responder fighting Avahi or Bonjour
for the port.

### Tool: `list_senders`

| Argument | Type | Default | Meaning |
|---|---|---|---|
| `timeout_ms` | integer 100 to 10000 | `1500` | how long to listen for announcements |

Answers `{"senders": [{"id", "name", "address", "width", "height", "fps"}]}`,
with anything the provider did not say left out rather than reported as zero.
`id` is the slug a source would sensibly be added under.

Annotations: `readOnlyHint = true`, `destructiveHint = false`,
`idempotentHint = true`, `openWorldHint = true`.

Where the runtime is absent the tool is an error naming the download page, and
`discover` answers with an empty list rather than an error, because a machine
with no NDI on it is not broken.

## Where they look for their elements

| Element | Used by | Where it comes from |
|---|---|---|
| `srtsrc`, `srtsink` | `srt`, `srt/output` | `gstreamer1.0-plugins-bad` |
| `whipclientsink`, `whipserversrc` | `whip`, `ingest` | `gstreamer1.0-plugins-rs`, GStreamer 1.28 or newer |
| `whepsrc`, `whepclientsrc` | `whip` | `gstreamer1.0-plugins-rs` |
| `ndisrc`, `ndisink` | `ndi` | `gstreamer1.0-plugins-rs`, plus the NDI runtime |

Nothing in `ingest`'s RTMP path needs an element at all.

Every one of these plugins refuses at `initialize` with one sentence naming the
element and the package for the platform it is running on, rather than a missing
element error from deep in a pipeline.

## Where to go next

* [Receive a phone or an OBS stream](../how-to/receive-a-phone-or-obs-stream.md)
* [Receive and send SRT](../how-to/srt.md)
* [Send the programme to a WHIP endpoint](../how-to/send-to-whip.md)
* [Use NDI](../how-to/use-ndi.md)
* [The plugin manifest](plugin-manifest.md)
* [The plugin lifecycle](plugin-lifecycle.md)
