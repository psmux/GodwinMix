# Streams

Every preview and monitoring endpoint, exactly. For a walkthrough with working
code, read [see and hear the mixer from
anywhere](../how-to/preview-and-audio.md) first.

These routes sit beside `/api/v1` and `/rpc` on the same port. They carry bytes
rather than JSON, which is why they are written out here rather than generated
from the method table.

## Authentication

The rules the rest of the control plane uses.

* `Authorization: Bearer <token>` on any request.
* `?token=<token>` on a `GET`, because an `img` tag and a browser opening a
  WebSocket cannot set a header.
* A `POST` does not accept the query form. It comes from code that can set a
  header, and a token in a URL ends up in more logs than it should.

Every route here needs the `read` scope. A token without it gets 403 with a
message saying so. A core with no tokens configured accepts everything, which is
the default and is right for a mixer on a LAN nobody else is on.

## Common behaviour

* **Nothing runs unless asked.** A branch is built when the first client opens
  it and removed when the last one closes. `gmx_stream_clients{kind}` in
  `/metrics` is the count.
* **Sharing.** Clients asking for the same thing share one branch. For audio
  that means the same target at the same rate, channel count and format; a
  different shape is a second branch.
* **Deadlines.** Every write to a client has a five second deadline. A peer that
  stops reading is disconnected, because these streams hold a mosaic
  subscription or an audio branch and one stuck client must not keep a pipeline
  up for everybody.
* **Dropping, not queueing.** A client that falls behind loses frames rather
  than accumulating them. A late preview frame is worth nothing.
* **A preview never reaches air.** Picture streams are fed from the multiview
  pipeline, never the programme one. Audio branches hang off a raw tee behind a
  leaky queue, so they cannot apply backpressure to the encoder.

---

## `GET /mjpeg/{target}`

`multipart/x-mixed-replace`, one JPEG per part, fed from the mosaic.

### Targets

| Target | Shows |
|---|---|
| `sheet` | the whole mosaic |
| `program` (or `programme`) | the programme return cell |
| `preview` | the armed scene. The programme tile while no scene server is present |
| any other value | the source with that id |

### Parameters

| Name | Type | Default | Meaning |
|---|---|---|---|
| `width` | integer | the mosaic's own | For `sheet`, the width of the answer. For a cell, the width of the mosaic it is cut from, which affects every client |
| `fps` | integer, 1 to 30 | `[multiview] fps` | The rate the mosaic runs at while this client is on it |

Both are clamped: the mosaic is 160 to 1920 wide and at most 30 fps.

### Response

```
HTTP/1.1 200 OK
Content-Type: multipart/x-mixed-replace; boundary=gmxframe
Cache-Control: no-store

--gmxframe
Content-Type: image/jpeg
Content-Length: 24601

<24601 bytes of JPEG>
--gmxframe
...
```

`Content-Length` is on every part, so a client never has to scan for the next
boundary. A JPEG can contain the boundary bytes by chance.

### Status codes

| Code | When |
|---|---|
| 200 | The stream is open. Parts follow until one side hangs up |
| 401 | No token, or one this core does not know |
| 403 | A token without the `read` scope |
| 404 | `[multiview] enabled = false`, so there is no picture to preview |

A cell that is not on the mosaic yet is not an error. The stream waits, because
a source that is still connecting will appear. The stream ends on its own if no
mosaic frame arrives within five seconds.

## `GET /mjpeg/item/{id}`

One scene item as a projector. Answers **501** until the scene server lands, with
a message naming `/mjpeg/preview`, `/mjpeg/program` and `/mjpeg/{source}` as
what works today.

---

## `GET /pcm/{target}`

A WebSocket carrying raw audio as binary frames, 10 ms to a frame.

### Targets

`program` (or `programme`) for the programme mix, or a source id.

### Parameters

| Name | Values | Default |
|---|---|---|
| `rate` | 8000 to 48000, clamped | 48000 |
| `channels` | 1 or 2, clamped | 2 |
| `format` | `f32`, `f32le`, `F32LE`, `s16`, `s16le`, `S16LE` | `f32` |

An unreadable value falls back to the default rather than refusing the
connection.

### Frames

Each WebSocket binary frame is a 16 byte header and then exactly 10 ms of
interleaved samples.

| Offset | Size | Type | Meaning |
|---|---|---|---|
| 0 | 4 | `u32` LE | Sequence number, from the first frame of this stream |
| 4 | 4 | `u32` LE | Reserved, zero |
| 8 | 8 | `u64` LE | Running time in nanoseconds on the programme clock |
| 16 | varies | samples | Interleaved, in the format asked for |

Payload size is `rate / 100 * channels * bytes_per_sample`:

| Shape | Payload | Whole frame |
|---|---|---|
| 48 kHz stereo F32LE (the default) | 3840 | 3856 |
| 48 kHz stereo S16LE | 1920 | 1936 |
| 16 kHz mono S16LE | 320 | 336 |

Sequence numbers count every frame the core produced for this stream. A gap
means frames were dropped because the client was not reading fast enough.
Running times are continuous across a gap and always advance by exactly
10,000,000 ns per frame, whatever size buffers the audio mixer produced.

Text frames from the client are ignored. Closing the socket removes the branch
when this was the last client on that shape.

### Status codes

| Code | When |
|---|---|
| 101 | Upgraded. Frames follow |
| 401, 403 | As above |
| 404 | No such source. The message lists the sources that are here |

The refusal is a plain HTTP status before the upgrade, not a socket that closes
for no stated reason.

## `GET /opus/{target}`

The same socket and the same header, carrying Opus at 48 kHz, 20 ms a frame,
64 kbit/s.

`rate`, `channels` and `format` are accepted and ignored: Opus is a 48 kHz
codec. The payload is one Opus packet, variable length.

Needs the GStreamer `opus` plugin. Without it the open is refused with a message
saying which element is missing.

---

## `POST /whep/{target}`

WebRTC through WHEP. The body is an SDP offer, `Content-Type: application/sdp`.
The answer is an SDP answer.

### Targets

`program` (or `programme`), or a source id.

### Status codes

| Code | When |
|---|---|
| 201 | An SDP answer, with a `Location` for the session |
| 405 | A `GET`. WHEP is a POST; the message says so |
| 501 | `whepserversink` is not installed. The message names the package for this platform, or says the session path is not wired yet on a build that has the element |

Ask `GET /api/v1/core/info` for `whep` in `features` before building on it.

### Configuration

```toml
[whep]
# Discover the public address. Empty offers host candidates only, which is what
# a mixer on a LAN that should not talk to the internet wants.
stun = "stun://stun.l.google.com:19302"
# Relay where no direct path exists. Tried in order. Nothing is relayed unless
# it has to be.
turn = ["turn://user:password@turn.example.com:3478"]
```

---

## `preview.open` and `preview.close`

Not HTTP routes but RPC methods, because they answer with a path rather than a
stream. Reachable on `/rpc`, on the local socket, on
`POST /api/v1/preview/open`, and through the CLI.

### `preview.open {target}`

`target` is `program` or a source id. Scope: `read`.

```json
{"target": "program",
 "path": "/home/you/.godwinmix/preview/program.sock",
 "transport": "unixfd"}
```

Read the socket with `unixfdsrc`. The frames are raw, at canvas size, with no
encode anywhere in the chain.

Opening twice on one target answers the same path and counts two holders. The
socket goes when the last `preview.close` arrives.

### `preview.close {target}`

```json
{"target": "program", "closed": true}
```

`closed` is `true` when this was the last holder and the socket went, `false`
when others still hold it.

### Platforms

Linux and macOS. On Windows both answer with the reason and point at `/mjpeg`.
A core whose GStreamer has no `unixfdsink` says which element is missing.

`godwinmix --info` prints the socket directory and the paths without opening
anything.

---

## Metrics

| Metric | Type | Labels | Meaning |
|---|---|---|---|
| `gmx_stream_clients` | gauge | `kind` | Clients on each stream. `mjpeg`, `pcm`, `opus`, `whep`, `unixfd`, `preview` |
| `gmx_multiview_subscribers` | gauge | | Clients holding the mosaic up, including every MJPEG stream |
| `gmx_multiview_fps` | gauge | | The mosaic's measured rate, zero when there is none |
| `gmx_encoder_running` | gauge | | 1 while the programme encode chain is attached |
| `gmx_encoder_consumers` | gauge | `kind` | What holds the encoder up: `output`, `whep`, `record` |
| `gmx_encoder_starts_total` | counter | | Times the encode chain has started since boot |

Every kind is written on every scrape, including the ones at zero, so a
dashboard can tell "none" from "not scraped yet".

## Features

`GET /api/v1/core/info` lists what this build has.

| Feature | Means |
|---|---|
| `mjpeg` | `/mjpeg/*` answers |
| `audio-monitor` | `/pcm/*` and `/opus/*` answer |
| `whep` | `whepserversink` is installed |
| `local-preview` | `preview.open` can make a socket here |
| `multiview` | `[multiview] enabled = true`, so there is a mosaic to cut from |

Branch on a feature rather than on a 404 you have to provoke first.
