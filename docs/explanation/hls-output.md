# How the HLS output works

An `hls/output` turns one or more encoded renditions into HLS and LL-HLS and
serves them from the control port. This page says why it is built the way
it is, and what it costs. `docs/reference/hls-output.md` is the surface.

## The shape of it

```
encoded rung ─▶ leaky queue ─▶ parser ─▶ cmafmux ─▶ appsink ─▶ cutter ─▶ ring ─▶ /hls/ routes
   (x N, and the audio)                    fragments every segment_ms,
                                           chunks every part_ms
```

One packager per rung. Each writes into its own ring of segments in memory,
and the control server reads the rings. The two halves share nothing but the
ring's mutex, which is held for a push or for cloning a few `Bytes`, and a
`tokio::sync::watch` channel that says how far the ring has got.

## Which GStreamer element cuts the segments

GStreamer 1.28 offers four ways, and we measured them on two minutes of 720p30
H.264 at 3 Mbit/s, then on twenty minutes to get past start up costs:

| Option | What it does | CPU for 20 min of video | Why not, or why |
|---|---|---|---|
| `hlssink3` | MPEG-TS segments and its own playlist | not measured further | TS, not CMAF, so no LL-HLS parts and no sharing segments with DASH |
| `hlscmafsink` | CMAF segments and its own playlist, written through `GOutputStream`s | 0.15 s user, 0.43 s sys | writes whole segments only, so no parts; its playlist would have to be thrown away for LL-HLS, the viewer key and rendition reports; it wants a file or a stream per segment |
| `isofmp4mux` with `fragment-duration` and `chunk-duration` | fragmented MP4, one fragment per segment, one chunk per part | 0.17 s user, 0.15 s sys | the same muxer as `cmafmux`, without the CMAF brand and single track rule |
| `cmafmux` into an appsink | CMAF, one track per muxer | 0.16 s user, 0.14 s sys | **chosen** |
| splitting fMP4 ourselves at keyframes | our own box writer | not built | `cmafmux` already writes correct `moof`s at the right places, and a hand written muxer is a codec's worth of edge cases |

The parser alone on the same input took 0.10 s user, so `cmafmux` adds about
0.2 s of CPU per twenty minutes of a 3 Mbit/s rung, a few hundredths of a
percent of one core. `hlscmafsink` costs about twice that because it writes
every segment to disk, and it still cannot make parts.

So the choice is `cmafmux` with `fragment-duration` set to the segment and
`chunk-duration` to the part, into an appsink, and our own playlists. The
muxer's output is easy to cut without parsing much (see
`crates/godwinmix-core/src/hls/cutter.rs`): the init segment is one buffer
starting with `ftyp`; every chunk starts with a buffer holding its `moof`,
flagged `HEADER`, and `DELTA_UNIT` unless it starts a fragment; the last
sample of a chunk is flagged `MARKER`. A `moof` without `DELTA_UNIT` is a new
segment, every `moof` is a new part, `MARKER` ends one.

One track per muxer is the CMAF rule, and it suits a ladder: the sound is
encoded once and packaged once as its own rung, `audio`, and every video rung
points at it with `EXT-X-MEDIA`. Nothing is muxed twice.

`send-force-keyunit` is off. A rung's encoder may be shared by other outputs,
and a muxer asking it for keyframes would put extra ones into every other
output's stream. Keeping a ladder's keyframes on the same frames is the
encoders' job (see below).

## Memory

A rung keeps `window / segment + 2` segments: the window it lists, and two
more for a player that read the playlist just before the oldest left. A
segment is its list of parts, and each part is one `bytes::Bytes`, filled by
one copy out of the GStreamer buffer. A segment request is answered with a
body made of the ring's own `Bytes`, so fifty viewers of one segment are
fifty reference counts on one allocation, not fifty copies. A player that
stops reading holds its `Bytes` and nothing else: the ring moves on and drops
its reference, and the memory goes when the player does.

Measured memory per rung for the default 30 second window is in the table at
the end.

## Waiting without a thread

LL-HLS asks the server to hold a playlist request until a named part exists
(`_HLS_msn`, `_HLS_part`). Each rung has a `watch` channel carrying its
position, the newest whole segment and the parts of the open one, updated
by the packager after every change. A request awaits `wait_for` on it with a
deadline, on the control server's runtime. It holds no lock while it waits
(`watch` takes its read lock only to run the predicate) and no thread. The
packager's side is `send_if_modified`, which never waits for a receiver.

The playlist itself is rendered once per change of the ring and kept, so
fifty viewers woken by the same part render it once. Each request adds its
own `key`, `token` and viewer id to the URIs, which is a pass over a few
kilobytes of text.

## Numbering, keyframes and a ladder

A playlist numbers its segments one apart. The first segment of a rung is
numbered from its start in running time (`round(start / segment)`), and
every one after it is the one before plus one. So rungs that start together,
or a rung that joins a running ladder, give the same number to the same two
seconds, and a player switching rungs asks for the same number on the other
one.

That holds only if every rung's keyframes are on the same frames. The ladder
the output makes itself (until the rendition planner builds encoders) forces
them: a probe on each encoder's input asks for a keyframe on the first frame
at or past each multiple of `segment_ms` of running time. Every rung sees the
same frames with the same times, so every rung picks the same frames. With no
ladder, the programme's own encode is packaged as it is, and its segments are
as long as its keyframe interval makes them; `EXT-X-TARGETDURATION` follows
the longest and never shrinks.

## Who may read

The page never puts the control token in a link, because a link is handed to
viewers and the token runs the mixer. Each output has a viewer key instead,
which opens that output's playlists and segments and nothing else. It is an
HMAC of the output's id under the machine's secret key, so it survives a
restart without being stored anywhere new. See the reference for the rules.

## What it costs

Measured on an M4 Pro, macOS, GStreamer 1.28.7, the release build, with
`examples/clock_ts.rs` as the only source.

MEASUREMENTS
