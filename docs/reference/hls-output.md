# HLS output

`hls/output` serves the programme as HLS or LL-HLS from the control port.
It opens no port of its own, writes nothing to disk, and keeps each rung's
last `window` seconds in memory. It is a built in output on Windows, macOS
and Linux, and needs `cmafmux` from gst-plugins-rs (the `fmp4` plugin); a
ladder the output makes itself also needs `x264enc`.

## Adding one

```json
{"id": "viewers", "type": "hls/output", "uri": "hls://viewers",
 "rendition": {"preset": "abr-ladder-4"},
 "params": {"low_latency": true}}
```

`output.add` still asks for a `uri`; any string does, and `hls://<id>` reads
best. The id becomes part of every URL, so keep it a slug.

### What it serves

| `rendition` | Rungs |
|---|---|
| left out, or `{"preset": "copy"}` | one rung, `programme`: the programme's own encode, packaged as it is. No decode, no encode |
| `{"preset": "abr-ladder-4"}` | `1080p` 6000, `720p` 3000, `480p` 1500, `360p` 800 kbit/s |
| `{"preset": "abr-ladder-3"}` | `720p`, `480p`, `360p` |
| `{"ladder": [RenditionRequest, ...]}` | one rung per request, named by its `id`, sized by `video.height` (and `video.width`, 16:9 when left out), at `video.bitrate_kbps` |

`ladder = "abr-ladder-4"` in `params` is the same as the preset. The sound is
one AAC track, `audio`, shared by every rung. A rung's id is a slug and may
not be `audio`.

Until the rendition planner builds the encoders, a ladder is made inside the
output: the programme encode is decoded once, scaled once per rung and encoded
with x264, with every rung's keyframes on the same frames every `segment_ms`.
A custom rung asking for a codec other than H.264 is refused with that reason.

### Params

| Param | Default | Range | Meaning |
|---|---|---|---|
| `segment_ms` | 2000 | 500 to 10000 | target segment length; a segment is cut at the first keyframe at or after it |
| `low_latency` | false | true or false | LL-HLS with parts of 333 ms |
| `part_ms` | 0 | 0, or 100 to half a segment | part length; any value but 0 turns LL-HLS on |
| `window` | 30 | three segments to 600 | seconds of the past each rung keeps and lists |
| `viewer_key` | derived | 16 characters or more | the key a viewer's link carries (see below) |

A frame rate that does not divide a part makes the parts a little longer: at
30 fps a 250 ms part is 8 frames, 267 ms, and `PART-TARGET` says 0.267.

## Routes

All on the control port.

| Route | Answer | `Cache-Control` |
|---|---|---|
| `GET /hls/{output}/master.m3u8` | multivariant playlist, `application/vnd.apple.mpegurl` | `no-store` |
| `GET /hls/{output}/{rung}/index.m3u8` | media playlist | `no-store` |
| `GET /hls/{output}/{rung}/init.mp4` | init segment (`init1.mp4` and on after a rebuild) | `max-age=<2 x window>, immutable` |
| `GET /hls/{output}/{rung}/{n}.m4s` | CMAF segment `n` | the same |
| `GET /hls/{output}/{rung}/{n}.{p}.m4s` | LL-HLS part `p` of segment `n` | the same |

Segments are `video/mp4`, or `audio/mp4` on the `audio` rung, and carry a
`Content-Length`. Every viewer is handed the same bytes the ring holds; nothing
is copied per request.

### Waiting

* `master.m3u8` asked for before every rung has a segment waits up to three
  segments (six seconds at least), then answers 503 with `Retry-After: 2`.
* `index.m3u8?_HLS_msn=M` holds until segment `M` is whole, and
  `?_HLS_msn=M&_HLS_part=P` until part `P` of it exists, for up to three target
  durations, then 503. `M` more than two past the newest is a 400. `_HLS_part`
  without `_HLS_msn` is a 400.
* A part or segment asked for a moment before it exists (the preload hint)
  is held until it does, up to a segment and two parts.
* Anything older than the window is a 404 that names the segments there are.

### Playlists

The multivariant playlist has one `EXT-X-STREAM-INF` per video rung with
`BANDWIDTH` (the peak measured over the window, or what the rung asked for if
that is higher, plus the audio), `AVERAGE-BANDWIDTH`, `CODECS` (RFC 6381, from
the encoder's caps), `RESOLUTION`, `FRAME-RATE` and `AUDIO="aud"`, and one
`EXT-X-MEDIA:TYPE=AUDIO` for the shared sound. An audio only output is a
single variant.

A media playlist carries `EXT-X-PROGRAM-DATE-TIME` on every segment, the
wall clock when its first frame was made. With LL-HLS it adds
`EXT-X-SERVER-CONTROL:CAN-BLOCK-RELOAD=YES,PART-HOLD-BACK=<3 parts>`,
`EXT-X-PART-INF`, `EXT-X-PART` for the last three target durations,
`EXT-X-PRELOAD-HINT` for the next part, and `EXT-X-RENDITION-REPORT` for
every other video rung. `EXT-X-TARGETDURATION` is the configured length, or
the longest segment there has been if a source's keyframes wandered; it grows
and never shrinks. Segment numbers are consecutive, and the first is set from
the running time, so every rung numbers the same seconds alike.

## Who may read

Two ways in, and nothing else on the port changes:

* the output's **viewer key**, as `?key=` on the URL. It opens this output's
  playlists and segments and nothing else: not another output, not
  `/api`, not `/rpc`.
* a control token with the `read` scope, as `Authorization: Bearer` or
  `?token=`, the same rule as the preview streams.

We chose a viewer key per output rather than leaving HLS open, so a mixer
with a control token does not serve its programme to anyone who guesses an
output's name. The key is derived: an HMAC of the output id under this
machine's secret key (`~/.godwinmix/secrets/key`), so it is the same after a
restart and a link that was handed out keeps working. To change a link that
leaked, give the output a new name or set `params.viewer_key`. On a mixer
with no control token at all, `/hls/` is open like the rest of the port.

Whatever a player presented in the query (`key`, `token`) is written onto
every URI in every playlist it is given, so a player that cannot send a
header keeps presenting it. A refusal is 401 (no key, wrong key, no token)
or 403 (a token without `read`), with a JSON body whose `error` says where
the right link is.

## Status

`output.list` and `output.get` add, for an `hls/output`:

| Field | Meaning |
|---|---|
| `playback.master_url_path` | `/hls/<id>/master.m3u8?key=<viewer key>`: put the page's own origin in front of it for the link |
| `playback.viewers`, `viewers` | clients that fetched a segment or part in the last two windows |
| `egress_kbps` | what the output sent over the last few seconds |
| `memory_bytes` | every rung's ring, segments and inits |
| `low_latency`, `segment_ms`, `part_ms`, `window` | what it was asked for |
| `rungs[]` | `id`, `kind`, `codecs`, `width`, `height`, `segments` held, `last_msn`, `memory_bytes` |

A viewer is told apart by the `v=` the multivariant playlist puts on every URI
it hands out, or, for a player that opened a media playlist directly, by its
address and user agent.

`state` is `live` once every rung has a whole segment.

## For the planner and the governor

`godwinmix_core::hls::attach(pipeline, stream, Input { id, kind, pad,
declared_kbps })` packages one encoded pad as a rung and returns a handle
whose `detach` takes it away again; `docs/explanation/hls-output.md` and the
top of `crates/godwinmix-core/src/hls/mod.rs` say what the pad must carry.
`godwinmix_core::hls::stream::egress_kbps()` is the sum over every HLS
output, the number the governor's uplink budget counts.

## Not here yet

DASH is not served. The ring and the CMAF segments would serve an MPD
unchanged; what is missing is a SegmentTimeline built from each segment's
decode time, and it is not written yet.
