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
| `channel.add {name, app?, auto_source?, key_mode?, protocols?, secret?}` | `POST /api/v1/channels` | admin | no | A channel and its first key |
| `channel.set {id, name?, app?, enabled?, auto_source?, key_mode?, protocols?, rtmps?}` | `POST /api/v1/channels/{id}/set` | admin | no | Change what is named, leave the rest |
| `channel.remove {id}` | `DELETE /api/v1/channels/{id}` | admin | yes | The channel, its keys, and the sources it made that no scene holds |
| `channel.key.add {id, label?, secret?}` | `POST /api/v1/channels/{id}/key/add` | admin | no | One more key |
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
to `app`, and `data.channel` and `data.app` naming the channel that has it
and its spelling. A `secret` that cannot be a key answers `-32602` with
`data.field` set to `secret` and the rule in `data.min_len`, `data.max_len`
and `data.allowed`.

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
| `app` | The RTMP application name, the path segment after the port, kept as it was typed: `Church` stays `Church`. Letters, digits, dashes, underscores and single spaces between words, up to 64, starting with a letter or digit. Defaults to the id. Matched without regard to case, so two channels cannot differ only in case. See [Case and spaces in an application name](#case-and-spaces-in-an-application-name) |
| `enabled` | Off turns every publisher away with a sentence saying the channel is switched off, and cuts off the ones already live |
| `auto_source` | A stream that goes live becomes a mixer source by itself. On by default |
| `key_mode` | `query`: the key rides on the stream name as `?psk=`, `?key=`, `?token=` or `?Token=`, or is the SRT passphrase, or the WHIP bearer token. `stream`: the whole stream name is the key, over every protocol |
| `protocols` | Which of `rtmp`, `srt` and `whip` it takes publishers over. At least one, unless RTMPS is on. A channel made before protocols existed is `["rtmp"]` |
| `rtmps` | `{enabled, port}`. RTMPS on a port of its own, 443 offered first. It needs the mixer's certificate |
| `keys` | Hints only. `hint` is the last four characters. The key itself is in the answer that made it, and after that only `channel.key.reveal` sends it. `imported: true` marks a key whose secret a person typed (`secret` on `channel.add` or `channel.key.add`); a key the mixer made leaves it out |
| `publish.server` | What an RTMP encoder's server box takes. The address is this machine's address on its network. A space in `app` is written `%20` here and in every address, because that is how an encoder has to send it |
| `publish.addresses` | The same for every protocol it has on, RTMP first: `{protocol, server, example}` with `<key>` where the key goes |
| `streams` | Every live stream, and any stream that left while a scene still holds its source (`state: "idle"`) |
| `streams[].key` | The id of the key that let it in |
| `streams[].protocol` | How it arrived: `rtmp`, `rtmps`, `srt` or `whip` |
| `streams[].video`, `.audio` | Read from the codec sequence headers and the byte rate. `fps` and `kbps` are measured over the last second |
| `streams[].source` | The mixer source it feeds, `<app>-<stream>` |
| `streams[].dropped_gops` | Whole GOPs readers of this stream lost by falling behind, this session. A reader that falls behind loses from the front of its queue and starts again at the next keyframe; the publisher is never slowed |
| `streams[].relay` | Where a mixer on this machine reads the stream: the listener's own port on loopback, `127.0.0.1:<rtmp port>`. Any show under a station adds the stream as a source with `source.add {type: "ingest/rtmp", relay, stream: "<app>/<name>"}`, and every show that does reads the one stream the station received. Absent while nothing is live |
| `streams[].source_error` | Why the mixer would not make the stream a source, for a channel with `auto_source` on, for instance that the plugin could not listen on its RTMP port because another program holds it. The stream is in but no scene can show it. Absent once the source is made, and whenever nothing was refused |

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

`secret` keeps a password encoders already send instead, the one after
`?psk=` in the address they publish to:

```json
{ "jsonrpc": "2.0", "id": 1, "method": "channel.add",
  "params": { "name": "Church", "app": "Church", "secret": "Sunday-2024" } }
```

The channel's id is still a slug, `church`; its `app` is `Church`, and its
first key carries `imported: true`. A typed secret is sealed in the secret
store exactly as a made one is, and never written to the channels file or a
log. The rule for one:

| Rule | Value |
|---|---|
| Length | 6 to 128 characters, after spaces at either end are trimmed |
| Characters | `A` to `Z`, `a` to `z`, `0` to `9`, `-`, `_`, `.`, `~`, and spaces |

Those are the characters a URL query carries without escaping, plus the
space, which Livebox allowed in a password. An encoder may send a space as it
is, as `%20` or as `+`; the listener reads all three as a space, which is why
`+` itself is refused. A refusal never repeats the secret:

```json
{ "code": -32602,
  "message": "that secret cannot be a key: it is 3 characters long, and a key needs at least 6. Use 6 to 128 letters, digits, dashes, underscores, dots, tildes or spaces, or leave it out and the mixer makes one.",
  "data": { "field": "secret", "min_len": 6, "max_len": 128, "allowed": "A-Z a-z 0-9 - _ . ~ space", "retryable": false } }
```

A refused secret makes nothing: the channel is not made without its key.

### Case and spaces in an application name

The listener finds a channel by its `app` without regard to case, so an
encoder sending `church`, `Church` or `CHURCH` reaches the channel whose
`app` is `Church`. Encoders are set up by hand, and a capital typed one way
on one encoder and another way on the next is a common reason a stream does not
arrive. The stream is then named with the channel's own spelling everywhere
after the listener (the hub, the source `<app>/<stream>` a show reads, the
events), so one stream never has two names. Because of that, `channel.add`
and `channel.set` refuse an `app` that differs from another channel's only in
case, and the refusal says so.

A space is allowed between words because Livebox allowed one in a channel
name. OBS and ffmpeg both end an RTMP address at a raw space and read what
follows as options: ffmpeg given `rtmp://host/Youth Hall/main` asks for the
application `Youth`. So the space has to travel as `%20`
(`rtmp://host/Youth%20Hall/main`), every address the mixer shows is written
that way, and the listener decodes `%XX` in the application name and the key
before it compares. An encoder that does send a raw space is still let in.

## `channel.key.add` and `channel.key.remove`

`channel.key.add {id, label?, secret?}` answers `{key: {id, label, secret}}`. The id is
a slug of the label, `Key 2` when there is no label. `secret` follows the
rule under [`channel.add`](#channeladd) and marks the key `imported`. A secret
the channel already has as another key is refused with `data.field` set to
`secret` and `data.key` naming that key, since encoders sending it are let in
already. `channel.key.remove {id,
key}` answers with the channel. A publisher live on the key taken back is cut
off at once; publishers on the other keys are not touched.

## The default channel

A mixer with a config file on disk and the ingest plugin installed, and no
channels, makes one as it starts: id `live`, name `Live`, app `live`, RTMP
only, `auto_source` on, one key with id `default-key` and label
`Default key`. It is a channel like any other, so the RTMP port is open from
that start. The channels file then carries `default_made = true`, which stays
after the channel is removed, so it is made once. A file that already has
channels without that line is taken as made. A plugin installed later by
`plugin.add` makes it then, under the same rules, and `event/channel.changed`
announces it. The secret store is per machine: when a key for `live` with id
`default-key` is sealed there already, from another mixer on the machine, the
default takes it rather than sealing a new one over it. There is no setting
to turn it off; remove the channel instead.

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
take SRT, 1404 for no such channel or, for a player, a stream not on air, and
1409 for a stream name already live. Each refusal is also
`event/channel.refused`, as for RTMP.

The stream has to be MPEG-TS with H.264 or HEVC video and AAC audio (HEVC goes
on the hub as enhanced RTMP HEVC). It is demuxed and parsed, never decoded,
into the same hub RTMP feeds.

A caller with `m=request` in the access control form
(`#!::r=<channel>/<stream>,m=request`) is a player: it is sent that stream as
MPEG-TS on the same port, with the same key rules, for as long as it reads.
Nothing is decoded; the stream goes out in the codecs it came in.

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
already live, `400` for an offer with neither H.264 nor VP8, `503` when the ingest plugin
is not running.

H.264 video is never decoded. VP8, from a browser that offers nothing else, is decoded and encoded as H.264 where the ingest plugin has a VP8 decoder and an H.264 encoder. Opus sound is turned into AAC
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

## `channel.thumbnail`

Read scope. A live stream's picture as a small JPEG, for the Channels page and
the monitoring wall.

```json
{"id": "sunday-service", "stream": "main", "width": 320}
```

`stream` is the first live one when left out. `width` is 16 to 640, made even,
320 when left out; the height follows the picture's shape. The answer:

```json
{"channel": "sunday-service", "stream": "main", "jpeg": "/9j/4AAQ...", "width": 320, "height": 180, "at_ms": 1791043200000}
```

`jpeg` is base64 and `at_ms` is when the keyframe it was decoded from arrived.
While the first keyframe is on its way the answer is
`{"channel", "stream", "pending": true, "retry_after_ms": 1000}`.

The ingest plugin makes the picture from the stream it already holds. The
first ask puts a tap on the stream that decodes keyframes and nothing else,
about one a second, scales each to the width asked for and keeps the newest
JPEG. Every ask keeps the tap for ten seconds more; ten seconds after the
last ask it goes, with its decoder. A channel nobody is looking at is never
decoded.

| Code | When |
|---|---|
| `-32004` | No channel by that id. `data.valid` lists the ids |
| `-32001` | Nothing is live on it, or not under that stream name. `data.state` is `idle` and `data.live` lists the streams that are; the picture comes once an encoder publishes |
| `-32001` | The ingest plugin is not running, with why and what to do, or it did not answer: `data.retry_after_ms` |

### `GET /api/v1/channels/{id}/streams/{stream}/thumbnail.jpg?width=320`

The same picture as the JPEG itself, for an `<img>`. Read, with the token as a
header or `?token=`. `200` with `image/jpeg`; `409` with the error body above
and `data.retry_after_ms` while the first keyframe is on its way or the
listener did not answer in three seconds, `409` with `data.state: "idle"` when
nothing is publishing to that stream, `404` for a channel that is not there.
A station serves it for its own channels on its control port.

```sh
curl -s -o main.jpg -H "Authorization: Bearer $TOKEN" \
  "http://127.0.0.1:8080/api/v1/channels/sunday-service/streams/main/thumbnail.jpg?width=320"
```

## Hooks

Two hook events follow a channel, configured like any other in
[hooks](hooks.md): `channel.stream.state` when a stream goes live or leaves,
and `channel.destination.state` when a destination's state, error or
reconnect count moves. Neither is built unless a hook asks for it.

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

### A publisher that went away without hanging up

A name is taken only while its publisher is sending. A publisher with a valid
key that finds the name held by a session that has sent nothing for 2 seconds
takes it over, over any of the four protocols: the old session is cut off, its
readers are told it ended, and the new one is live from its first keyframe.
One that finds a session quiet for between half a second and 2 seconds waits
for the rest of the 2 seconds and then takes it, or is refused if the old one
starts sending again in that time. A session still sending is never taken
over, and the newcomer gets the sentence above.

This is what lets a reloaded or restarted browser, or an encoder whose network
dropped, get straight back on air. Before, the old session held the name until
its protocol's own timeout ended it, about 30 seconds for WebRTC, and every
publish in between was refused. The two numbers:

* 2 seconds, because every working publisher sends far more often (sound every
  20 to 40 ms, a picture every frame, and a static screen share still sends one
  a second), and because it is when the mixer itself judges a source stalled,
  so a stream that quiet is already off the programme.
* Half a second, below which a session is treated as live and a newcomer is
  refused at once, so a second tab on a name that is working is told so
  without waiting.

The log says `<app>/<stream>: <old address> had sent nothing for 2 s, so <new
address> took the name over`. The core is not told the stream went idle, since
it never did. The hub's own publishers (a transcode's output, a direct show's
input) are never taken over.

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
YouTube, Facebook, Twitch, any RTMP or RTMPS server, or an SRT receiver; or
kept on this machine, as a recording (`file`) or a watch link (`hls`), see
[Record and Watch link](#record-and-watch-link). The publisher's own bytes are
remuxed and sent. Nothing is decoded or encoded, so
a destination costs a socket and a little memory, not a CPU core, unless it
asks for a rendition of its own (see below).

All three methods need the `admin` scope. Each answers the whole channel, with
its `destinations` list as below. A core started with `--rehearsal` refuses
`channel.destination.add` and `channel.destination.set`, as it refuses
`output.add`.

### `channel.destination.add`

`POST /api/v1/channels/{id}/destination/add`

| Param | | |
|---|---|---|
| `id` | required | the channel |
| `platform` | required | `youtube`, `facebook`, `twitch`, `instagram`, `kick`, `linkedin`, `x`, `tiktok`, `custom` or `srt`; or `file` or `hls` |
| `label` | optional | what the list calls it. The platform's name when left out |
| `server` | optional | the ingest address. Left out, the platform's own. `custom` and `srt` need one. For `file` a folder, for `hls` the link's params |
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

`rendition` is optional too: `{"preset": "youtube-720p30"}`, or a rendition
request written out (`{"video": {"height": 480, "bitrate_kbps": 1400}}`), as
`output.add` takes. See [Converting a destination](#converting-a-destination).

### `channel.destination.set`

`POST /api/v1/channels/{id}/destination`

`id` and `destination` pick the destination; `label`, `server`, `key`,
`stream`, `enabled` and `rendition` change only what is named. A key left out
is kept. An empty key clears it, which only `custom` allows. `rendition: null`
or `{"preset": "copy"}` goes back to sending the stream as it arrives. Moving a
destination to another platform is a remove and an add.

### Converting a destination

A destination with no `rendition` is sent the publisher's own bytes, exactly
as before renditions existed. One with a `rendition` is planned with every
other converting destination of its channel, by the same planner the
programme's outputs use:

* A rendition the stream already matches (same codec, size and frame rate, a
  bit rate within a quarter of the one asked for, AAC sound at the rate and
  channel count asked for) is a copy. Its table row, its queue and its
  sender are the ones a destination with no rendition gets.
* Otherwise the stream is decoded once, however many destinations convert
  it; scaled once per distinct size and frame rate; and encoded once per
  distinct rendition. Three destinations asking for `youtube-720p30` read the
  bytes of one encoder. Sound the stream already has in the right shape is
  copied beside converted video.
* Hardware decoders and encoders are used when the codec catalogue says this
  machine has them (`vtdec_hw` and `vtenc_h264_hw` on a Mac), software
  otherwise. Setting `GMX_CODEC_DISABLE` to the hardware entries' ids makes a
  run CPU only.
* Every node costs a ticket from the resource governor before it starts, and
  holds it while it runs.

A destination sends H.264 or HEVC and AAC: HEVC as enhanced RTMP, and SRT as
MPEG-TS made from the same tags. A rendition that asks for another codec is
refused with that reason. A stream that is not H.264 or HEVC with AAC can be copied
but not converted.

`rendition` in the record is what was asked. While the stream is live, `plan`
is what the plan gave it:

| Field | |
|---|---|
| `mode` | `copy` or `transcode` |
| `stream` | the stream it was planned against, when the destination sends `*` |
| `reason` | `copied: the source's video goes out as it is`, or `encoded because the source is 1920x1080 and this output wants 1280x720` |
| `encoder`, `encoder_reason` | the catalogue id (`h264-videotoolbox`) and why that one |
| `video`, `audio` | the shapes going out |
| `nodes` | the ids of the plan's nodes it reads, which other destinations may share |

A destination the plan cannot serve has `state: "failed"`, the reason in
`error`, and `refused`:

| Field | |
|---|---|
| `code` | `governor` (no room on this machine), `plan` (nothing here can make it) or `shed` (it ran and was stopped to keep what is on air) |
| `message` | the same sentence as `error` |
| `need`, `have` | what it would cost and what is free, for `governor` |
| `advice` | `[{text, request}]`, each a rendition that fits now, to offer as a button that sets it |

When the stream is live and the governor refuses a rendition that
`channel.destination.add` or `.set` asked for, the edit is not kept and the
call fails with `Safety` (`-32003`), `data: {need, have, advice, destination,
channel}`, as a refused programme output does. A refusal that only happens
later, when the stream goes live or changes, shows on the destination
instead, and is asked about again every ten seconds.

When the machine runs short on air the governor sheds channel conversions
before a show's own renditions. The destination is marked `refused.code:
"shed"` with the governor's sentence, an alert and `event/governor.shed` say
the same, and it is planned again once the machine has had 30 seconds with
nothing to shed.

The plan follows the stream. When a publisher changes size or frame rate,
the channel is planned again and only the nodes whose work changed are
rebuilt: a 480p branch keeps running when the 720p one moves. A new bit rate
alone changes nothing.

`Channels::rendition_plan(id)` in the core is the channel's plan in the
`rendition.plan` shape, for the `channel:<id>` scope, and `event/rendition.plan`
is sent with `scope: "channel:<id>"` whenever the channel's table changes.

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
| `rendition` | what it asked to be converted to; absent for a plain copy |
| `plan` | what the plan gave it, while its stream is live; absent for a plain copy |
| `refused` | why it is not sending what it asked for, and what would fit; absent otherwise |
| `file` | a recording's file: `name`, `path`, `bytes`, `duration_ms`, and `open` while it is written. The last file stays after the stream stops. Absent on every other platform |
| `playback` | a watch link's `master_url_path`, `dash_url_path` and `viewers`. Absent on every other platform |

The states:

* `waiting`: on, and nothing to send yet because the stream is not live.
* `connecting`: dialling for the first time. `error` says why the last try
  failed, for instance `nothing answered at rtmp://10.0.0.9:1935`.
* `live`: sending.
* `reconnecting`: the far end went away and is being dialled again. A server
  you run is retried from 100 ms up to every 2 s; YouTube, Facebook and
  Twitch from 1 s up to every 30 s, the same two policies as outputs. This
  goes on for as long as the destination is switched on. A connection that
  stops taking the stream for ten seconds, which is how a pulled cable or a
  dead link looks, counts as gone, and so does a server that hangs up or
  stops answering during the publish; none of those is a refusal.
* `failed`: the server answered no, as in `YouTube refused the key (...)`.
  It is asked again after the longest wait. After three refusals in a row it
  is asked once a minute, and `error` ends with `Refused 3 times in a row;
  asking again every 60 s.` so a key fixed on the platform's side is picked
  up without touching the destination.

On every connection the far end gets the stream's metadata and codec headers
first, then nothing until a keyframe, so a platform never sees a picture it
cannot decode. A destination that falls behind loses whole GOPs from the
front of its own queue and starts again at the next keyframe; the publisher
and the other destinations do not wait for it.

### Record and Watch link

Two platforms keep the stream on this machine. Neither has a key or a far
end, and neither converts anything: a `rendition` on either is refused with
`data.field: "rendition"`.

| Platform | Title | `server` | What it does |
|---|---|---|---|
| `file` | Record | a folder on the mixer, as `D:/Recordings` or `file:///srv/recordings`. Left out, `Videos/GodwinMix` in the home folder of the user the mixer runs as, the folder `record/output` uses | writes the stream as MPEG-TS, copied, to `<channel>-<stream>-<yyyymmdd-hhmmss>.ts` in that folder, with the local time the file opened |
| `hls` | Watch link | `hls://` with params, as `hls://?segment_ms=2000&window=6&low_latency=true`, or left out for the defaults. The params are an `hls/output`'s, see [hls-output.md](hls-output.md) | packages the stream as HLS, copied, and serves it from the control port |

A recording starts a new file every time its stream goes live, and closes
the file when the stream stops or the destination is switched off. MPEG-TS
needs no index, so a file cut short by a power cut still plays. Its `uri_host`
is the folder. `state` is `live` while it writes; `file.bytes` and
`file.duration_ms` move with every `channel.get`.

A watch link is packaged by the station's HLS packager, the process a show
without compositing uses for its `hls://` output, and it runs only while a
watch link or such an output is switched on. It reads the stream from the
listener's loopback relay, so nothing is decoded. Its link is

```
/hls/channel/<channel>/<destination>/index.m3u8?key=<viewer key>
```

with `manifest.mpd` beside `index.m3u8` for DASH. The viewer key opens this
one link and nothing else on the mixer, so the link needs no control token;
it is made from the channel and the destination's id with this machine's
secret key, so it is the same after a restart. `playback.viewers` counts the
players that fetched something in the last two windows. A request with a
wrong key is answered 401; one for a link the channel does not have, 404
with the links it has.

HLS carries H.264 or HEVC and AAC. A stream with other sound or picture is
`failed` with an `error` that names the codec and says to change the encoder,
since the link converts nothing. Under a single process core with no station,
a watch link is `failed` with a sentence saying it needs the station.

The platform servers are in `godwinmix_protocol::destination::PLATFORMS`.
The web page's form keeps its own copy in `ui/client/destinations.js`, and a
test fails when the two disagree.
