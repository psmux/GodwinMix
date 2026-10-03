# The network plugins

Five first party plugins carry media over a network: `srt`, `whip`, `ingest`,
`ndi` and `udp`. Every setting they have is here, but for `udp`, which has
its own page, [udp.md](udp.md).

If `docs/reference/plugins.md` exists in your checkout, it is the index of every
first party plugin and these rows belong in it; this page is where they live
until then.

## Every provide

| Id | Kind | Plugin | What it does | Works today |
|---|---|---|---|---|
| `srt/source` | source | `srt` | receive SRT, caller or listener | yes |
| `srt/output` | output | built into the core, not a plugin | send MPEG-TS over SRT | yes |
| `rist/output` | output | built into the core, not a plugin | send MPEG-TS over RIST (Simple Profile) | yes |
| `whip/output` | output | `whip` | send the programme to a WHIP endpoint | needs the core to load plugin outputs |
| `whip/whep` | source | `whip` | receive a stream over WHEP | yes |
| `ingest/rtmp` | source | `ingest` | listen for an RTMP publisher | yes |
| `ingest/whip` | source | `ingest` | run a WHIP endpoint publishers send to | yes |
| `ingest/discover` | device | `ingest` | one RTMP port for many publishers | yes |
| `ndi/source` | source | `ndi` | receive an NDI sender | yes, with the NDI runtime |
| `ndi/output` | output | `ndi` | announce the programme as an NDI sender | needs the core to load plugin outputs |
| `ndi/discover` | device | `ndi` | list NDI senders, `list_senders` | yes, with the NDI runtime |
| `udp/source` | source | `udp` | MPEG-TS over UDP or RTP, unicast or multicast, one program chosen | yes; every setting is in [udp.md](udp.md) |
| `udp/output` | output | `udp` | the programme as MPEG-TS over UDP or RTP | yes, Linux and macOS |
| `rtsp/output` | output | `rtsp` | serve the programme over RTSP for players, decoders and NVRs that pull | yes, Linux and macOS |
| `ipcam/source` | source | `ipcam` | an IP camera's MJPEG stream or snapshot picture over HTTP | yes |
| `ipcam/discover` | device | `ipcam` | ONVIF cameras on the LAN, each profile as an RTSP `hls/source` | yes |
| `icecast/output` | output | `icecast` | the programme's sound to an Icecast or SHOUTcast 2 mount | yes, Linux and macOS |
| `icecast/source` | source | `icecast` | an internet radio station or any audio stream over HTTP, live | yes |

### What "needs the core" means, precisely

* **Plugin outputs.** `crates/godwinmix-core/src/plugin/output.rs` resolves an
  output type against a static list of the built in ones and never asks the
  loader. `crates/godwinmix-core/src/plugin/source.rs` already does the
  equivalent lookup for sources; the same `.or_else(|| loader::…)` in the output
  registry is what is missing.
* **Device provides** are loaded: the supervisor starts a device as a
  singleton beside a service, `device.discover` asks every device and merges
  the answers, and the smoke test drives both. The two rows above that say
  "needs the core to load device provides" are older than that and are wrong;
  they load.
* **Plugin tools** reach the plugin: `tool.call` is registered
  (`crates/godwinmix/src/control/methods/plugins.rs`), `POST /api/v1/tool/call`
  is its route, and the answer comes back from whichever running instance of
  the plugin owns the tool.
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

The mixer listens and the publisher dials in. The protocol is written in Rust
over `rml_rtmp`; GStreamer is used only to remux for the core.

| Key | Type | Default | Meaning |
|---|---|---|---|
| `port` | integer 0 to 65535 | `1935` | the TCP port |
| `bind` | string | `0.0.0.0` | the interface |
| `app` | string | empty | the first part of the publish path. Empty takes any |
| `stream_key` | string, `format: secret` | empty | the rest of it. Empty takes any |
| `relay` | string | empty | the channel server's address, `127.0.0.1:<rtmp port>`. Filled in with `stream` when a channel's stream becomes a source |
| `stream` | string | empty | `<channel>/<stream>` to read from the channel server |

With `relay` empty the source owns its port. One source is one picture, so a
second publisher is refused while the first is live and the refusal reaches
the publisher's own error box. Bytes are held back until the first keyframe.

With `relay` and `stream` set it reads that stream from `ingest/discover` over
loopback, waiting quietly while the stream is not live, and asking again once
a second while the channel server itself is not up yet. When the publisher
leaves, the process ends and the core starts it again in place behind the
freeze frame, so the next publisher starts on a clean pipe.

An RTMP audio or video message carries exactly the body of an FLV tag, so the
listener writes FLV for nine bytes plus eleven per message and parses nothing.
That FLV is remuxed to Matroska before it crosses to the core, by two parsers
and a muxer with nothing decoded, because FLV into the core's `decodebin`
autoplugs a GL backed decoder on macOS that the normaliser cannot take.
`plugins/ingest/src/remux.rs` carries the measurement and names the one core
change that would remove the need for it.

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
| `app` | string | empty | with no channels, accept publishers on this application name only |
| `channels` | array | none | the channel table. Laid over these settings by the core from its channel registry; never written to the config |

The channel server: one listener, one thread per connection, and a hub
(`plugins/ingest/src/hub.rs`) every reader of a stream goes through with a
bounded queue of its own. [The channel methods](channels.md) are the core's
side.

With channels, a publisher is let in by its channel's table and each stream
raises `event/channel.stream` (`state` `live`, again when its codecs are
first known, and `idle` when it leaves) and a refusal `event/channel.refused`.
A new table through `configure` applies at once, and cuts off a publisher
whose key was taken back or whose channel was switched off or removed.

With none, it takes any publisher. `discovery = { mdns = ["_rtmp._tcp"] }`.
One publisher becomes one candidate of type `ingest/rtmp` whose params name
the relay and the stream. It also pushes `event/ingest.publisher`, which the
supervisor adds as a source:

```json
{"action": "connected", "id": "live-phone", "type": "ingest/rtmp",
 "name": "live/phone", "peer": "10.0.0.31:51666",
 "params": {"relay": "127.0.0.1:1935", "stream": "live/phone"}}
```

and `{"action": "left", "id": "live-phone", "name": "live/phone"}`.

A reader on this machine that sends `GMXHUB <app>/<stream>` and a newline on
the RTMP port, instead of a handshake, gets that stream as FLV: the file header, then `onMetaData` and the two codec headers, then tags
from the next keyframe. That is how an `ingest/rtmp` source in another
process reads it.

### Tool: `streams`

No arguments. Answers `{port, relay, streams}`, one row per live stream with
`app`, `stream`, `since_ms`, `from`, `key`, `video` (`codec`, `width`,
`height`, `fps`, `kbps`), `audio` (`codec`, `channels`, `sample_rate`,
`kbps`), `readers`, `dropped_gops` and `bytes`. Reads numbers the listener
keeps anyway and changes nothing.

Annotations: `readOnlyHint = true`, `destructiveHint = false`,
`idempotentHint = true`, `openWorldHint = false`.

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

The NDI® runtime is `dlopen`ed and never linked. The Windows installer carries
it beside the plugin under the NDI end user terms; elsewhere, download it from
<https://ndi.video/for-developers/ndi-sdk/>. NDI® is a registered trademark of
Vizrt NDI AB, and GodwinMix is not affiliated with or endorsed by Vizrt. See
<https://ndi.video/>.

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

## `icecast/output` and `icecast/source`

`icecast/output` sends the programme's sound to an Icecast 2 or SHOUTcast 2
server (the Icecast HTTP source protocol). The sound is decoded and encoded
once; the picture is dropped at the demuxer.

| Key | Type | Default | Meaning |
|---|---|---|---|
| `uri` | string | empty | `icecast://<user>:<password>@<host>:<port>/<mount>`; fills every field below |
| `host` | string | required | the server |
| `port` | integer 1 to 65535 | `8000` | |
| `mount` | string | `live.mp3` | what listeners open after the port |
| `user` | string | `source` | |
| `password` | string, `format: secret` | required | the source password |
| `format` | `mp3`, `vorbis`, `opus` | `mp3` | Ogg for the last two |
| `bitrate_kbps` | integer 32 to 320 | `128` | |
| `name` | string | `GodwinMix` | the station name players show |
| `public` | boolean | `false` | list the mount in the server's directory |

Song title updates are not sent. Health says how much has been sent, or why
the server refused.

`icecast/source` plays an audio stream over HTTP or HTTPS (`uri`) as a live
source: `souphttpsrc` in ICY mode, `icydemux`, `parsebin`, and the sound as it
came (MP3, AAC, Vorbis, Opus) to the core in Matroska. Health carries the last
song title the station sent; `stats` answers `{address, title}`.

## `ipcam/source` and `ipcam/discover`

`ipcam/source` reads what a camera serves over HTTP or HTTPS, as JPEGs that
cross to the core in Matroska; the core decodes them. It has no sound.

| Key | Type | Default | Meaning |
|---|---|---|---|
| `uri` | string | required | the camera's `http://` or `https://` MJPEG or snapshot address |
| `mode` | `auto`, `mjpeg`, `snapshot` | `auto` | `auto` takes a `.jpg`, `snapshot`, `still` or `image.cgi` address as a snapshot and anything else as MJPEG |
| `fps` | integer 1 to 30 | `5` | snapshots asked for a second |
| `user` | string | empty | the camera's login, basic or digest, as the camera asks |
| `password` | string, `format: secret` | empty | |

Health says how many pictures have arrived, or why the last snapshot failed.
`stats` answers `{address, mode, pictures}`.

`ipcam/discover` sends one WS-Discovery probe to `239.255.255.250:3702` for
ONVIF video transmitters, and for each that answers asks GetCapabilities,
GetProfiles and GetStreamUri, with a WS-Security password digest when a login
is set. Each profile becomes a candidate of the core's own `hls/source`, named
`<camera> (<profile>)`, with the RTSP address and the login in it. Cameras that
refuse without a login are named in its health.

| Key | Type | Default | Meaning |
|---|---|---|---|
| `user` | string | empty | tried on every camera found |
| `password` | string, `format: secret` | empty | |

Discovery stays on the local segment (TTL 1) and answers within the time the
core gives it. Cameras that want HTTP digest on their ONVIF service rather
than WS-Security are not yet logged in to.

## `rist/output`, and RIST in

Built into the core, beside `srt/output`. RIST (VSF TR-06-1, the Simple
Profile) is RTP with retransmission asked for over RTCP: the output muxes the
programme to MPEG-TS, seven packets a datagram, and hands it to `ristsink`.

| Key | Type | Default | Meaning |
|---|---|---|---|
| `uri` | string | required | `rist://<receiver>:<port>`. The port must be even; RTCP uses the one above it |
| `buffer_ms` | integer 50 to 30000 | `1000` | how much sent video is kept to answer retransmission requests; match the receiver's buffer |

The output says it is connected once the receiver's RTCP has given a round
trip time. `stats` answers `ristsink`'s own statistics. It opens no port on
this machine: it sends, and the receiver listens.

To receive RIST, add a source with the address to listen on,
`rist://0.0.0.0:5004`: `hls/source` claims `rist://` and treats it as a live
stream, as it does SRT and RTSP. Bonding and the Main Profile's encryption are
not offered yet.

## `rtsp/output`

Serves the programme at `rtsp://<this machine>:<port>/<path>`. The port opens at
`start` and closes at `stop`; nothing listens before the output exists. Every
player shares one media and one packetiser; a player may ask for RTP over UDP
or interleaved in the RTSP connection (TCP). Nothing is encoded again.

| Key | Type | Default | Meaning |
|---|---|---|---|
| `port` | integer 1 to 65535 | `8554` | the TCP port players connect to. 554 needs the machine's administrator |
| `path` | string | `live` | what follows the port; letters, digits, `-`, `_` and `/` |
| `bind` | string | `0.0.0.0` | the address to accept players on |

Carries H.264 and H.265 video, AAC, MP3 and Opus audio, as the programme or its
rendition has them. A player that asks before the programme has arrived is
answered 404 and tries again; one that joins starts at the next keyframe.
Health says how many players are connected and how many frames have gone
out. `stats` answers `{url, clients, bytes_read}`.

## Where they look for their elements

| Element | Used by | Where it comes from |
|---|---|---|
| `srtsrc`, `srtsink` | `srt`, `srt/output` | `gstreamer1.0-plugins-bad` |
| `whipclientsink`, `whipserversrc` | `whip`, `ingest` | `gstreamer1.0-plugins-rs`, GStreamer 1.28 or newer |
| `whepsrc`, `whepclientsrc` | `whip` | `gstreamer1.0-plugins-rs` |
| `ndisrc`, `ndisink` | `ndi` | `gstreamer1.0-plugins-rs`, plus the NDI runtime |

`ingest`'s RTMP path needs `flvdemux`, `h264parse`, `aacparse` and
`matroskamux`, all from the base and good sets, and no element at all for the
protocol itself.

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
