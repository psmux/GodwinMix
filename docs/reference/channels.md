# The channel methods

A channel is a named place encoders publish to, with one set of keys, over
every protocol it has switched on:

```
rtmp://<mixer>:<rtmp port>/<app>/<stream>?psk=<key>
rtmps://<mixer>:<its port>/<app>/<stream>?psk=<key>
srt://<mixer>:<srt port>?streamid=<app>/<stream>&passphrase=<key>
POST http://<mixer>:<control port>/whip/<app>/<stream>   Authorization: Bearer <key>
```

One port per protocol serves every channel, and several streams may be live on
one channel at once: an encoder that sends its own ladder publishes each
rendition as a stream of its own. The listeners are the ingest plugin's
`ingest/discover`; the methods below are the core's. Nothing listens until
that plugin is installed and switched on, and even then no port is open until
a channel that is switched on uses it. `channel.list` says which are open, for
which channels, and why one a channel wants is not.

| Method | REST | Scope | Destructive | What it does |
|---|---|---|---|---|
| `channel.list` | `GET /api/v1/channels` | read | no | Every channel, and the port they share |
| `channel.get {id}` | `GET /api/v1/channels/{id}` | read | no | One channel |
| `channel.add {name, app?, auto_source?, key_mode?, protocols?}` | `POST /api/v1/channels` | admin | no | A channel and its first key |
| `channel.set {id, name?, app?, enabled?, auto_source?, key_mode?, protocols?, rtmps?}` | `POST /api/v1/channels/{id}/set` | admin | no | Change what is named, leave the rest |
| `channel.remove {id}` | `DELETE /api/v1/channels/{id}` | admin | yes | The channel, its keys, and the sources it made that no scene holds |
| `channel.key.add {id, label?}` | `POST /api/v1/channels/{id}/key/add` | admin | no | One more key |
| `channel.key.remove {id, key}` | `POST /api/v1/channels/{id}/key/remove` | admin | yes | Take one key back |
| `channel.key.reveal {id, key}` | `POST /api/v1/channels/{id}/key/reveal` | admin | no | Read one key back, to give it out again |
| `channel.certificate.set {cert, key}` | `POST /api/v1/channels/certificate/set` | admin | no | Give RTMPS a certificate and its private key, as PEM |
| `channel.certificate.generate {names?}` | `POST /api/v1/channels/certificate/generate` | admin | no | Make a self signed certificate for RTMPS |

WHIP is not a method: it is `POST /whip/<app>/<stream>` on the control port,
described under [WHIP](#whip-on-the-control-port) below.

`channel.destination.*` sends a channel's streams on to YouTube, Facebook,
Twitch or any RTMP or SRT address; see [Destinations](#destinations) below.

Every refusal says what state things are in and what to do, with `data` a
client can act on (`docs/reference/errors.md`). An unknown channel or key
answers `-32004` with the ids that would have worked in `data.valid`. A bad
application name or one already taken answers `-32602` with `data.field` set
to `app`, and `data.channel` naming the channel that has it.

## The Channel record

```json
{
  "id": "sunday-service",
  "name": "Sunday service",
  "app": "sunday-service",
  "enabled": true,
  "auto_source": true,
  "key_mode": "query",
  "protocols": ["rtmp", "srt", "whip"],
  "rtmps": { "enabled": false, "port": 443 },
  "keys": [
    { "id": "key-1", "label": "Key 1", "created": "2026-09-29T16:44:23.872Z", "hint": "jca7" }
  ],
  "publish": {
    "server": "rtmp://192.168.77.106:19381/sunday-service",
    "example": "rtmp://192.168.77.106:19381/sunday-service/main?psk=<key>",
    "addresses": [
      { "protocol": "rtmp", "server": "rtmp://192.168.77.106:19381/sunday-service", "example": "rtmp://192.168.77.106:19381/sunday-service/main?psk=<key>" },
      { "protocol": "srt", "server": "srt://192.168.77.106:19382", "example": "srt://192.168.77.106:19382?streamid=sunday-service/main&passphrase=<key>" },
      { "protocol": "whip", "server": "http://192.168.77.106:18681/whip/sunday-service", "example": "http://192.168.77.106:18681/whip/sunday-service/main" }
    ]
  },
  "streams": [
    {
      "name": "main", "state": "live", "since_ms": 1790700583138, "from": "127.0.0.1:52616",
      "key": "key-1",
      "protocol": "srt",
      "video": { "codec": "h264", "width": 1280, "height": 720, "fps": 29.95, "kbps": 2542 },
      "audio": { "codec": "aac", "channels": 1, "sample_rate": 44100, "kbps": 70 },
      "source": "sunday-service-main",
      "dropped_gops": 0
    }
  ],
  "destinations": []
}
```

| Field | What it is |
|---|---|
| `id` | A slug made from the name when the channel is made. It never changes |
| `app` | The RTMP application name, the path segment after the port. Letters, digits, dashes and underscores, up to 64. Defaults to the id |
| `enabled` | Off turns every publisher away with a sentence saying the channel is switched off, and cuts off the ones already live |
| `auto_source` | A stream that goes live becomes a mixer source by itself. On by default |
| `key_mode` | `query`: the key rides on the stream name as `?psk=`, `?key=`, `?token=` or `?Token=`, or is the SRT passphrase, or the WHIP bearer token. `stream`: the whole stream name is the key, over every protocol |
| `protocols` | Which of `rtmp`, `srt` and `whip` it takes publishers over. At least one, unless RTMPS is on. A channel made before protocols existed is `["rtmp"]` |
| `rtmps` | `{enabled, port}`. RTMPS on a port of its own, 443 offered first. It needs the mixer's certificate |
| `keys` | Hints only. `hint` is the last four characters. The key itself is in the answer that made it, and after that only `channel.key.reveal` sends it |
| `publish.server` | What an RTMP encoder's server box takes. The address is this machine's address on its network |
| `publish.addresses` | The same for every protocol it has on, RTMP first: `{protocol, server, example}` with `<key>` where the key goes |
| `streams` | Every live stream, and any stream that left while a scene still holds its source (`state: "idle"`) |
| `streams[].key` | The id of the key that let it in |
| `streams[].protocol` | How it arrived: `rtmp`, `rtmps`, `srt` or `whip` |
| `streams[].video`, `.audio` | Read from the codec sequence headers and the byte rate. `fps` and `kbps` are measured over the last second |
| `streams[].source` | The mixer source it feeds, `<app>-<stream>` |
| `streams[].dropped_gops` | Whole GOPs readers of this stream lost by falling behind, this session. A reader that falls behind loses from the front of its queue and starts again at the next keyframe; the publisher is never slowed |

In `stream` key mode the stream is named after the key's id, so a key never
becomes part of a source id or a log line.

## `channel.list`

```json
{
  "channels": [ ... ],
  "rtmp": { "port": 19381, "urls": ["rtmp://192.168.77.106:19381", "rtmp://127.0.0.1:19381"], "listening": true },
  "listeners": [
    { "protocol": "rtmp", "transport": "tcp", "port": 19381, "open": true, "because": ["sunday-service"] },
    { "protocol": "srt", "transport": "udp", "port": 19382, "open": true, "because": ["sunday-service"] },
    { "protocol": "whip", "transport": "tcp", "port": 18681, "open": true, "because": ["sunday-service"] },
    { "protocol": "webrtc", "transport": "udp", "port": 19390, "last_port": 19405, "open": false, "because": [] }
  ],
  "hosts": ["192.168.77.106", "127.0.0.1"]
}
```

`rtmp.port` is `rtmp_port` in the ingest plugin's settings, 1935 unless set.
`listening` is true while the RTMP port is open to the network. When the
plugin is not there, or the port would not open, `rtmp.problem` says why in
one sentence.

`listeners` has a row for every listener, open or not: `because` names the
channels that are switched on and use it, and `open` says whether it is
listening now. A row a channel wants that is not open carries `problem`, the
reason and what to do. The rows:

| `protocol` | Open while |
|---|---|
| `rtmp` | a channel that is on has RTMP on. On every interface |
| `relay` | a channel is on but none has RTMP: the RTMP port number, on `127.0.0.1` only (`loopback: true`), which is how the mixer's own sources read a channel's streams |
| `srt` | a channel that is on has SRT on. One UDP port for every channel, `srt_port`, 9000 unless set |
| `rtmps` | a channel that is on has RTMPS on, at that port. One row per port chosen |
| `whip` | a channel that is on has WHIP on. It is the control port, which is open anyway; the row says WHIP is being answered on it |
| `webrtc` | a WHIP publisher is connected. A range of 16 UDP ports from `webrtc_port`, 8189 unless set, one per publisher |

`hosts` is where an encoder can reach this machine, first one first.
`certificate` is there once RTMPS has one: see below.

## `channel.add`

```json
{ "jsonrpc": "2.0", "id": 1, "method": "channel.add", "params": { "name": "Sunday service" } }
```

answers with the channel and its first key:

```json
{ "channel": { "id": "sunday-service", ... }, "key": { "id": "key-1", "label": "Key 1", "secret": "vjktb3s7s868eiquqfgcjca7" } }
```

The secret is 24 lower case letters and digits, with 0, o, 1 and l left out
so it reads back over a phone. No list or event carries it. An admin reads it
again with `channel.key.reveal`.

## `channel.key.add` and `channel.key.remove`

`channel.key.add {id, label?}` answers `{key: {id, label, secret}}`. The id is
a slug of the label, `Key 2` when there is no label. `channel.key.remove {id,
key}` answers with the channel. A publisher live on the key taken back is cut
off at once; publishers on the other keys are not touched.

## Protocols and ports

`channel.set {id, protocols: ["rtmp", "srt"]}` switches SRT on for a channel
and leaves the rest. The first channel to switch a protocol on opens its port;
the last one to switch it off, be switched off, or be removed closes it. A
publisher already live over a protocol that is switched off is cut off, and
told why.

`channel.set {id, rtmps: {enabled: true, port: 443}}` turns RTMPS on at a port.
Channels that choose the same port share one listener. A port that is the RTMP
port or the control port is refused with `-32602` and `data.field` set to
`rtmps.port`. A channel with no protocol and RTMPS off is refused with
`data.field` set to `protocols`.

### SRT

The stream id is `<app>/<stream>`, or the access control form
`#!::r=<app>/<stream>,m=publish`. A stream id with no stream in it is the
stream `main`. The key is either:

* the SRT passphrase, with nothing in the stream id. The passphrase is the
  channel's first key, or the key whose id is given as `u=<key id>` in the
  access control form. libsrt checks it during the handshake, so a wrong one
  never reaches the mixer; or
* `?psk=<key>` on the stream id (`sunday-service/main?psk=<key>`, or `psk=` in
  the access control form), with no passphrase. Not encrypted.

A caller is refused with SRT's access control codes: 1400 for a stream id that
names no channel, 1401 for a key, 1403 for a channel that is off or does not
take SRT or for `m=request`, 1404 for no such channel, 1409 for a stream name
already live. Each refusal is also `event/channel.refused`, as for RTMP.

The stream has to be MPEG-TS with H.264 video and AAC audio. It is demuxed
and parsed, never decoded, into the same hub RTMP feeds.

### WHIP on the control port

| Route | Answer |
|---|---|
| `POST /whip/{app}/{stream}` | `201 Created`, `Content-Type: application/sdp`, the SDP answer, and `Location: /whip/{app}/{stream}/{session}` |
| `DELETE /whip/{app}/{stream}/{session}` | `200` and the session ends; `404` when it had ended already |
| `PATCH /whip/{app}/{stream}/{session}` | `405`: every candidate is in the answer and none are taken later |

The body is the SDP offer, sent with `Content-Type: application/sdp`
(anything else is `415`). The key is `Authorization: Bearer <key>`, or
`?psk=`, `?key=` or `?token=` on the URL for a client that cannot set a
header. These routes are not behind the control token: the channel's key is
what lets a publisher in. A refusal is the status and the same sentence an
RTMP publisher gets, as plain text: `403` for a key or a channel that is off
or does not take WHIP, `404` for no such channel, `409` for a stream name
already live, `400` for an offer with no H.264, `503` when the ingest plugin
is not running.

The video must be H.264, and is never decoded. Opus sound is turned into AAC
on the way in. WebRTC media uses one UDP port per publisher from the range in
the `webrtc` row: GStreamer's `webrtcbin` does ICE with libnice, which cannot
share one UDP port between sessions, so a range is what it allows.

### RTMPS and the certificate

`channel.certificate.set {cert, key}` takes a certificate (and its chain) and
its private key as PEM, checks that a TLS server starts with them, and keeps
them sealed. `channel.certificate.generate {names?}` makes a self signed one
for `names`, or for this machine's addresses and `localhost`. Both answer:

```json
{ "source": "self_signed", "names": ["192.168.77.106", "127.0.0.1", "localhost"], "fingerprint": "3A:1F:...", "created": "2026-09-30T10:40:00Z" }
```

The same is in `channel.list` as `certificate`. The private key is never read
back by any method. A certificate that is not PEM, or a key that does not
belong to it, is refused with `-32602`, a sentence saying which, and
`data.field` set to `cert`. RTMPS opens nothing until a channel has it on and
there is a certificate; until then its `listeners` row says it needs one.

## `channel.key.reveal`

```json
{ "jsonrpc": "2.0", "id": 2, "method": "channel.key.reveal", "params": { "id": "sunday-service", "key": "key-1" } }
```

answers with the key and nothing else:

```json
{ "secret": "vjktb3s7s868eiquqfgcjca7" }
```

The keys are sealed in the mixer's secret store, not thrown away, because the
listener needs them to let publishers in. This reads one back. It needs an
admin token: a read token calling it is refused with `-32002`, and
`channel.list`, `channel.get` and `event/channel.changed` still carry only the
hint, so a read token never sees a key. Keys made before this method existed
are read back the same way.

Each call is logged at info as `channel key revealed`, with the channel, the
key's id and the token that asked. The key itself is never logged. The call is
not a mutation, so it is not in the session log and an idempotency key does
not cache its answer.

An unknown key answers `-32004` with the channel's key ids in `data.valid`. A
key whose record is there but whose secret is not (the secret store was
deleted, or its key file replaced) answers `-32001` saying so, with `data.channel`
and `data.key`: revoke it and make a new one.

## Events

Subscribe with `core.subscribe {events: ["channel.*"]}`.

| Event | Payload | When |
|---|---|---|
| `event/channel.changed` | `{channel}` | A channel was made or changed, a key was made or taken back, a stream went live, learned its codecs, or left, a destination was added, changed or removed, or a destination's state, error or reconnect count moved |
| `event/channel.removed` | `{id}` | A channel was removed |
| `event/channel.refused` | `{id, stream, from, why}` | A publisher was turned away. `why` is the sentence its encoder was sent. `stream` is empty when the stream name was the key |

Nothing is measured for a channel nobody is looking at beyond what the
listener keeps anyway: the codec numbers and bit rates are read from the listener when
`channel.list` or `channel.get` is called, and `event/channel.changed` goes out
on changes, never on a timer. The Channels page calls `channel.list` every two
seconds while it is on screen and something is live, which is where its bit
rates come from.

## What a refused publisher is told

| Why | The sentence |
|---|---|
| No channel by that name | there is no channel called 'x' on this mixer. Check the server address in the encoder: it ends with the channel's name. |
| The channel is off | the channel 'x' is switched off. Switch it on in the mixer's Channels page and publish again. |
| No key | the channel 'x' needs a key. Put it on the stream name as main?psk=<key>, or copy the whole stream key from the mixer's Channels page. |
| The protocol is off | the channel 'x' does not take SRT. Switch SRT on in its settings on the mixer's Channels page, or publish over a protocol it has on. |
| A wrong key | that key is not one of the keys of the channel 'x'. It may have been taken back; copy the current one from the mixer's Channels page. |
| The name is taken | x/main is already being published from 10.0.0.9:51000. Give this encoder another stream name, or stop the other one first. |

Each goes to an RTMP encoder as `NetStream.Publish.Denied`, to an SRT caller as
its rejection code, to a WHIP client as the body of its refusal, to the log,
and out as `event/channel.refused`. A wrong key is never repeated in any of them.

## Where it is kept

Channels are written to `<config stem>.runtime.channels.toml` beside the
runtime store, the way the scene collection sits beside it, and are there
after a restart. The keys are not in that file: they are sealed in the same
secret store plugin secrets use, and only their hints are written down. A
destination's platform, label, stream and host are in the file; its whole
address and its key are sealed beside the channel's keys, and removing the
channel forgets them too. A
source a channel made is listed there too, so after a restart the core knows
which sources are its own to take away again. So are each channel's
`protocols` and `rtmps`, and what a page shows of the RTMPS certificate; the
certificate and its key are sealed.

## The plugin side

The core hands `ingest/discover` its table as `channels` in the plugin's
settings, at every start and through `configure` on every change, and the
RTMPS certificate as `tls: {cert, key}`. Each channel carries `protocols` and,
when RTMPS is on, `rtmps_port`; the plugin opens and closes its listeners to
match (`plugins/ingest/src/listeners.rs`) and reports them as `listeners` in
its `streams` answer. The core hands it a WHIP offer as the `tool.call`
`whip.offer {app, stream, key, sdp, peer}`, answered `{session, sdp}` or
`{status, why}`, and `whip.end {session}`; neither is in the manifest's
tools, so no agent is offered them. Each channel in the table also carries
`destinations`, the ones that are switched on, each as `{id, platform, url,
stream}` with the whole address. The plugin raises `event/channel.stream` (a
stream went live, learned its codecs, or left), `event/channel.refused`, and
`event/channel.destination` (`{channel, destination, state, since_ms, kbps,
reconnects, error}`) when a destination's state, error or reconnect count
moves. It answers the `streams` tool with every live stream measured and each
destination's `kbps` under `destinations`. It runs one restream per
destination, each reading the hub through a bounded queue of its own. A mixer source reads its stream from the listener over
loopback on the RTMP port number, which stays open on `127.0.0.1` while any
channel is on. `plugins/ingest/src/hub.rs` is the registry every
reader goes through; `subscribe(app, stream)` is how the restreamer reads a
stream.

## Destinations

A channel's destinations are where its stream is sent on to as it arrives:
YouTube, Facebook, Twitch, any RTMP or RTMPS server, or an SRT receiver. The
publisher's own bytes are remuxed and sent. Nothing is decoded or encoded, so
a destination costs a socket and a little memory, not a CPU core.

All three methods need the `admin` scope. Each answers the whole channel, with
its `destinations` list as below. A core started with `--rehearsal` refuses
`channel.destination.add` and `channel.destination.set`, as it refuses
`output.add`.

### `channel.destination.add`

`POST /api/v1/channels/{id}/destination/add`

| Param | | |
|---|---|---|
| `id` | required | the channel |
| `platform` | required | `youtube`, `facebook`, `twitch`, `instagram`, `kick`, `linkedin`, `x`, `tiktok`, `custom` or `srt` |
| `label` | optional | what the list calls it. The platform's name when left out |
| `server` | optional | the ingest address. Left out, the platform's own. `custom` and `srt` need one |
| `key` | optional | the stream key. Write only |
| `stream` | optional | which of the channel's streams to send. `*`, the default, is the one live longest, and when it leaves, the next one still live |
| `enabled` | optional | `true` unless given |

The destination's id is made from its label (`twitch-backup`), or the
platform's id, with `-2`, `-3` on the end when that is taken.

What is refused, with `data.field` naming the field:

* A platform not on the table. `data.platforms` lists the ones there are.
* A platform that needs a key (every one but `custom` and `srt`) with no key.
* No server for a platform that hands out one per stream: `instagram`,
  `linkedin` and `tiktok`.
* No server for `custom` or `srt`, or a server of the wrong kind: `custom`
  takes `rtmp://` or `rtmps://`, `srt` takes `srt://`.
* A `custom` server with no key and no key on the end of the address.

`srt` keeps no key: an SRT passphrase goes in the address, and the address is
never shown back.

### `channel.destination.set`

`POST /api/v1/channels/{id}/destination`

`id` and `destination` pick the destination; `label`, `server`, `key`,
`stream` and `enabled` change only what is named. A key left out is kept. An
empty key clears it, which only `custom` allows. Moving a destination to
another platform is a remove and an add.

### `channel.destination.remove`

`POST /api/v1/channels/{id}/destination/remove`

`id` and `destination`. The stream to that destination stops and the
destination is forgotten; the publisher and the other destinations are not
touched. It is destructive, so `dry_run: true` answers what it would stop.

### The destination record

| Field | |
|---|---|
| `id` | a slug, unique within the channel |
| `platform` | the platform id |
| `label` | |
| `uri_host` | scheme, host and port, as `rtmps://live-api-s.facebook.com:443`. Never the path or the query, since either can carry a key |
| `has_key` | false while a platform that needs a key has none |
| `stream` | the stream it sends, or `*` |
| `enabled` | |
| `state` | `off`, `waiting`, `connecting`, `live`, `reconnecting` or `failed` |
| `since_ms` | how long it has been in that state, in milliseconds. A stream's `since_ms` is a time of day instead, in milliseconds since 1970 |
| `kbps` | what is going out, over the last second |
| `reconnects` | connections lost and made again since it was switched on |
| `error` | the last thing that went wrong, in words, or `null` |

The states:

* `waiting`: on, and nothing to send yet because the stream is not live.
* `connecting`: dialling for the first time. `error` says why the last try
  failed, for instance `nothing answered at rtmp://10.0.0.9:1935`.
* `live`: sending.
* `reconnecting`: the far end went away and is being dialled again. A server
  you run is retried from 100 ms up to every 2 s; YouTube, Facebook and
  Twitch from 1 s up to every 30 s, the same two policies as outputs.
* `failed`: the far end refused the key, as in `YouTube refused the key
  (...)`. It is asked again after the longest wait, and after three refusals
  in a row it stops until the destination is changed.

On every connection the far end gets the stream's metadata and codec headers
first, then nothing until a keyframe, so a platform never sees a picture it
cannot decode. A destination that falls behind loses whole GOPs from the
front of its own queue and starts again at the next keyframe; the publisher
and the other destinations do not wait for it.

The platform servers are in `godwinmix_protocol::destination::PLATFORMS`.
The web page's form keeps its own copy in `ui/client/destinations.js`, and a
test fails when the two disagree.
