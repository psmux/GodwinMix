# HLS output

`hls/output` serves the programme as HLS or LL-HLS, and the same segments
as DASH, from the control port.
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
| `segments` | `cmaf` | `cmaf`, `ts` or `both` | which segments are made: CMAF, MPEG-TS beside them, or MPEG-TS alone. Only a show without compositing makes MPEG-TS so far |
| `access` | `keyed` | `keyed` or `open` | whether a player needs the viewer key (see [Who may read](#who-may-read)) |
| `open_public` | false | true or false | with `access = "open"`, answer players outside loopback and the private networks too |

A frame rate that does not divide a part makes the parts a little longer: at
30 fps a 250 ms part is 8 frames, 267 ms, and `PART-TARGET` says 0.267.

## Routes

All on the control port.

| Route | Answer | `Cache-Control` |
|---|---|---|
| `GET /hls/{output}/master.m3u8` | multivariant playlist, `application/vnd.apple.mpegurl` | `no-store` |
| `GET /hls/{output}/manifest.mpd` | the same rungs as a DASH MPD, `application/dash+xml` | `no-store` |
| `GET /hls/{output}/{rung}/index.m3u8` | media playlist | `no-store` |
| `GET /hls/{output}/{rung}/init.mp4` | init segment (`init1.mp4` and on after a rebuild) | `max-age=<2 x window>, immutable` |
| `GET /hls/{output}/{rung}/{n}.m4s` | CMAF segment `n` | the same |
| `GET /hls/{output}/{rung}/{n}.{p}.m4s` | LL-HLS part `p` of segment `n` | the same |
| `GET /hls/{output}/master-ts.m3u8` | the MPEG-TS multivariant playlist, with `segments` `ts` or `both` | `no-store` |
| `GET /hls/{output}/{rendition}/ts.m3u8` | an MPEG-TS media playlist: `main`, or `audio1`, `audio2` for a further sound | `no-store` |
| `GET /hls/{output}/{rendition}/{start_ms}.ts` | one MPEG-TS segment, `video/mp2t`, named by the UTC millisecond its first frame was made | `max-age=<2 x window>, immutable` |

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
`EXT-X-MEDIA:TYPE=AUDIO` per sound track, all in the group `aud`. An audio
only output is a single variant.

Every sound track is its own rendition (`audio` for the first, then
`audio1`, `audio2` in the order the input's PMT lists them). Its
`EXT-X-MEDIA` has:

* `NAME`: the language in English when the PMT's ISO 639 code is one this
  build knows (`English`, `Spanish`), else the code itself, else `Track N`.
  Two tracks of one language are `English` and `English 2`.
* `LANGUAGE`: the code as RFC 5646 wants it, the two letter form where
  there is one (`eng` and `en` both give `en`), left out when the PMT gave
  none or `und`.
* `DEFAULT=YES` on the first track and `NO` on the others, `AUTOSELECT=YES`
  on all of them, and `CHANNELS`.

`BANDWIDTH` on each variant adds the largest sound track's, and `CODECS`
names every sound codec once.

A media playlist carries `EXT-X-PROGRAM-DATE-TIME` on every segment, the
wall clock when its first frame was made. With LL-HLS it adds
`EXT-X-SERVER-CONTROL:CAN-BLOCK-RELOAD=YES,PART-HOLD-BACK=<3 parts>`,
`EXT-X-PART-INF`, `EXT-X-PART` for the last three target durations,
`EXT-X-PRELOAD-HINT` for the next part, and `EXT-X-RENDITION-REPORT` for
every other video rung. `EXT-X-TARGETDURATION` is the configured length, or
the longest segment there has been if a source's keyframes wandered; it grows
and never shrinks. Segment numbers are consecutive, and the first is set from
the running time, so every rung numbers the same seconds alike.

The sound is cut where the top rung's picture is cut, so `audio/index.m3u8`
has the same segment numbers as the picture for the same moments, each
starting within 125 ms of the picture's (a part's half when parts are on),
and both windows cover the same time. When the output is rebuilt, or the feed
behind it stops and starts again, every rung and the sound get one
`EXT-X-DISCONTINUITY` before the same segment number, and
`EXT-X-DISCONTINUITY-SEQUENCE` counts those breaks. A muxer that writes a new
header without a break gets a new `EXT-X-MAP` and no discontinuity.

### DASH

`manifest.mpd` is a dynamic MPD (`isoff-live`) over the same CMAF files: one
video `AdaptationSet` with a `Representation` per rung and one audio set, each
with a `SegmentTemplate` naming `{rung}/$Number$.m4s` and a `SegmentTimeline`
whose `t` is each segment's `tfdt` and whose `d` is the gap to the next.
`availabilityStartTime` is the first segment's programme date time less its
media time, `suggestedPresentationDelay` three segments, and `UTCTiming` the
server's clock. It lists whole segments only, so it plays at ordinary DASH
latency (about three segments) whatever `low_latency` says. After a rebuild
of the output, or a restart of its feed, it lists only the segments since the
newest break and decoded by the newest init segment.

## MPEG-TS

With `segments = "ts"` or `"both"`, an output of a show without compositing
also makes MPEG-TS segments, for players that take nothing else. They are
written in Rust from the frames the packager already reads (no GStreamer
muxer), cut at the same keyframes as the CMAF picture and given its numbers,
so `main/ts.m3u8` and `main/index.m3u8` list the same segments with the same
`EXT-X-MEDIA-SEQUENCE` and the same `EXT-X-PROGRAM-DATE-TIME`.

| Rendition | Carries |
|---|---|
| `main` | the picture and the first sound track in one stream, for a player that cannot take separate sound |
| `audio1`, `audio2`, ... | each further sound track alone |

The TS multivariant playlist lists the first sound as an `EXT-X-MEDIA`
without a `URI` (it is in `main`) and each further one with its playlist.
Every segment starts with a PAT and a PMT; a segment of `main` then starts
with a keyframe and its parameter sets. The continuity counters run on
from one segment to the next, so two segments back to back are one clean
stream. Sound is ADTS AAC. PIDs are fixed: PMT 0x1000, picture 0x100, sound
0x101.

The TS playlists are plain HLS whatever `low_latency` says: no parts and no
blocking reloads. `EXT-X-TARGETDURATION` follows the same rule as CMAF's.
A segment's name is its start in UTC milliseconds, so a name is never used
twice for one output, not after a restart either, and a cache that keeps
segments by path can keep them for as long as it likes.

With `segments = "ts"` the CMAF playlists answer 404 and say where the TS
one is. The picture is still cut as CMAF inside the packager, since that is
what decides where segments end, but no CMAF sound is made.

## Who may read

An output with `access = "open"` answers a player on loopback or a private
network (10/8, 172.16/12, 192.168/16, 100.64/10, link local, IPv6 ULA and
link local) with no key and no token. It is meant for an output behind a
gateway that checks a signature of its own. A player anywhere else gets a
403 that says so, and still gets in with the key or a token. Add
`open_public = true` to answer everyone; then it is open to the internet if
the port is. This applies to CMAF, LL-HLS, DASH and MPEG-TS alike.

Otherwise (`access = "keyed"`, the default) there are two ways in, and
nothing else on the port changes:

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
| `playback.dash_url_path` | `/hls/<id>/manifest.mpd?key=<viewer key>`, the same for DASH |
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

## From a show without compositing

A direct show has no programme encode and no show process, so its HLS output
is an output of the show, packaged by the station's HLS packager (a child
process of the station) and served from the station's port. It is added with
`show.output.add` (or in `show.add`'s `outputs`), not `output.add`:

```json
{"id": "bbc-one", "uri": "hls://viewers", "params": {"segment_ms": 2000, "window": 30}}
```

| | A show that composites | A show without compositing |
|---|---|---|
| Added with | `output.add {type: "hls/output"}` | `show.output.add {uri: "hls://<name>"}` |
| Its id | the output's `id` | the name in `hls://<name>`, or `output` |
| `params` | the table above | the same names, defaults and limits |
| Rungs | `programme`, or one per ladder rung, and `audio` | `main`, and `audio`, `audio1`, `audio2` for each sound track |
| `rendition` | none, a preset or a ladder | none (a copy), or one rendition; a ladder is refused with `data.ladder: true` |
| Served by | the show, relayed by the station | the station's port: the station lets the player in, its HLS packager answers |
| Link | `playback.master_url_path` in `output.list` | `playback.master_url_path` on the output in `show.list` |

The routes are the ones above with `show=<id>` in the query, which every URI
a playlist hands out carries on:

```
/hls/viewers/master.m3u8?show=bbc-one&key=...
```

With no rendition the input's own H.264, HEVC or AV1 and AAC are packaged as
they arrive. Nothing is decoded, and every segment starts on one of the
input's own keyframes, so a feed with a keyframe every second and
`segment_ms = 2000` gets two second segments; a longer GOP gives longer
segments. With a rendition the output reads the pair the plan made, admitted
by the governor like any other.

Every sound track the input carries is packaged, each as its own rung,
named and tagged from the language `direct.input` lists for it. The
packager reads them all from the relay (`GMXHUB <app>/<stream> tracks=all`,
each tag's track in the FLV stream id).

HLS here carries AAC sound and nothing else. An input whose sound is MPEG
audio (layer II or MP3), AC-3 or E-AC-3 is refused rather than packaged into
segments no player plays:

* `show.output.add` refuses a copy when the input is already live with such
  sound, with `data.audio_codec` and `data.rendition: {"audio": {"codec":
  "aac"}}`, the rendition to add instead.
* An output added before the input was heard goes to `state: "failed"`, and
  its `error` names the codec and the `show.output.set` call that fixes it.

`{"audio": {"codec": "aac"}}` decodes the sound once, encodes it as AAC and
still copies the picture. MPEG audio and AC-3 are decoded by the codec
catalogue's `mpeg-audio-decode` (`mpg123audiodec`) and `ac3-a52dec`
(`a52dec`), or `ac3-decode` (`avdec_ac3`) where only gst-libav has it; E-AC-3 has no decoder in the catalogue yet and is refused by
the planner.

The packager is the station's binary run with `--hls-packager`, a child of
the station that runs only while at least one such output is on. It reads
the show's stream from the ingest plugin's relay on loopback, the same
`GMXHUB` door a source process reads a channel through
(`plugins-network.md`), so the relay is opened for a show with an HLS output
even when no channel has opened it. The station checks the viewer key or
the token itself and forwards what it let in to the packager on loopback.
Its CPU is counted with the station's other children in `governor.status`.

The output's `state` is `waiting` until the input is live, `connecting`
until every rung has a whole segment, then `live`; `kbps` is what is packaged.
When the packager stops (it crashed, or left three asks in a row unanswered,
about 15 s, since one slow answer on a busy machine is not a hang), every
such output is `reconnecting`, with an `error` that says so and when the
station starts it again: after 1 s, doubling to 30 s while it keeps
stopping. After five stops in a row the outputs say `failed` instead, and
the station keeps trying. Each restart counts once in `reconnects`. Requests
meanwhile get a 503 with `Retry-After: 2`. The link stays the same, but the
segments start again from the first, so a player may have to open it again.
The egress of these outputs is not yet counted in
`godwinmix_core::hls::stream::egress_kbps()`, so the governor's uplink budget
does not see it.

### MPEG-TS for a caller inside the station

The station's Flussonic facade serves an output's MPEG-TS under its own
paths and names, without a viewer key. It calls three methods on
`station.direct.hls` (`station/direct/hls/facade.rs`), each one request to
the packager on loopback:

| Call | Answers |
|---|---|
| `ts_master(&st, show, output)` | `ts::Master`: `main` and every sound track, as data |
| `ts_media(&st, show, output, rendition)` | `ts::Media`: the window, each segment's number, discontinuity, UTC start in ms and length |
| `ts_segment(&st, show, output, rendition, at)` | the segment's bytes; `at` is `ts::At::Msn`, `Start` (ms), `Second` or `Clock { unix_s, duration_ms }` |

`ts::Master::render` and `ts::Media::render` write the playlists with the
caller's own URIs, and `ts::clock_name` and `ts::parse_clock` make and read
names of the form `2026/10/23/08/04/12-2000.ts`. A refusal is a
`TsRefusal` with the HTTP status to answer and the sentence.

## Not here yet

MPEG-TS from an `hls/output` of a show that composites: its rungs arrive
as GStreamer pads rather than the hub's frames, so `segments` other than
`cmaf` is refused there, with that reason.

Low latency DASH (chunked transfer of the part being written) is not served;
the MPD lists whole segments.
