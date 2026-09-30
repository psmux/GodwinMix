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

GStreamer 1.28 offers four ways. We measured them on twenty minutes of
360p30 H.264 at 3 Mbit/s (36,000 frames, 426 MB) read from a file and pushed
through as fast as it would go, so the number is the element's own work:

| Option | What it does | CPU for 20 min of video | Why not, or why |
|---|---|---|---|
| `hlssink3` | MPEG-TS segments and its own playlist | not measured | TS, not CMAF, so no LL-HLS parts and no sharing segments with DASH |
| `hlscmafsink` | CMAF segments and its own playlist, written through `GOutputStream`s | 0.15 s user, 0.43 s sys | writes whole segments only, so no parts; its playlist would have to be thrown away for LL-HLS, the viewer key and rendition reports; it wants a file or a stream per segment |
| `isofmp4mux` with `fragment-duration` and `chunk-duration` | fragmented MP4, one fragment per segment, one chunk per part | 0.17 s user, 0.15 s sys | the same muxer as `cmafmux`, without the CMAF brand and single track rule |
| `cmafmux` into an appsink | CMAF, one track per muxer | 0.16 s user, 0.14 s sys | **chosen** |
| splitting fMP4 ourselves at keyframes | our own box writer | not built | `cmafmux` already writes correct `moof`s at the right places, and a hand written muxer is a codec's worth of edge cases |

The parser alone on the same input took 0.10 s user and 0.06 s sys, so
`cmafmux` adds about 0.15 s of CPU per twenty minutes of a rung, around a
hundredth of a percent of one core. With chunks on or off it costs the same.
`hlscmafsink` costs about twice as much because it writes every segment to
disk, and it still cannot make parts.

So the choice is `cmafmux` with `fragment-duration` set to the segment and
`chunk-duration` to the part, into an appsink, and our own playlists. The
muxer's output is easy to cut without parsing much (see
`crates/godwinmix-core/src/hls/cutter.rs`): the init segment is one buffer
starting with `ftyp`; every chunk starts with a buffer holding its `moof`,
flagged `HEADER`, and `DELTA_UNIT` unless it starts a fragment; the last
sample of a chunk is flagged `MARKER`. A `moof` without `DELTA_UNIT` is a new
segment, every `moof` is a new part, `MARKER` ends one.

A fragment must be a whole number of chunks. `cmafmux` ends a fragment only
on a chunk boundary, and only once that boundary has reached the fragment's
length. Asked for 2 s fragments and 333 ms chunks, it makes six chunks of
333,333,333 ns, which come to two nanoseconds short of two seconds, so it
misses the keyframe at 2 s and ends the fragment at the next boundary that
starts with one: every segment 2.67 s, at 30 and at 60 fps, measured. The
packager therefore divides the segment into a whole number of parts, rounds
the chunk down to the nanosecond, and asks for a fragment of exactly that
many chunks (`HlsParams::mux_durations`). Parts of 250 ms or 200 ms never had
the problem, which is why it hid until the default third of a second.

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
`examples/clock_ts.rs` as the only source (1280x720, the wall clock burned
in, `snow` for the CPU and memory rows so every encoder spends its whole
bitrate). The programme is 1080p30 at 6 Mbit/s. Other programs, including
other GodwinMix cores, were running on the machine, so treat the CPU rows as
plus or minus half a point.

| What | Measured |
|---|---|
| The core with the programme on air and no HLS | 13.6% of one core, 176 MiB |
| One `hls/output` of the programme as it is, LL-HLS, 40 s twice | 15.4% and 15.7%: about 2% of one core for the whole output, its pipeline and proxy included |
| The same, plain HLS | 15.7% and 16.1% |
| A four rung ladder made inside the output (decode, four x264 `veryfast` encodes, packaging) | 126% of one core. This is the stand in's encoding; the planner will share encoders |
| Memory per rung, 30 s window (16 whole segments and the open one) | 1080p 33.8 MB, 720p 16.9 MB, 480p 8.5 MB, 360p 4.5 MB, audio 0.7 MB. Roughly the rung's real bitrate times the window and two segments; x264 overshot its target on noise |
| Keyframe to its first LL-HLS part being served | 273 to 281 ms, with parts of 267 ms: about 10 ms of muxing after the part's last frame |
| Last frame of a segment to the segment being served, plain HLS | 34 to 45 ms |
| 50 simulated LL-HLS viewers on the four rung ladder, 60 s | 18,811 requests, 157 Mbit/s out, no errors; the core went from 125.9% to 127.9% of one core and its memory rose by 1 MiB |
| 50 simulated viewers on the single 1080p rung, 60 s | 317 Mbit/s out, no errors; 15.7% to 18.4% of one core, memory flat at 241 MiB |
| Glass to player, hls.js 1.6.15 with `lowLatencyMode`, headless Chrome, 1080p | 1.67 s and 1.87 s from the time burned into the picture to the page's clock, through the source's own encode, the mixer, the programme encode, the ladder and the packager. hls.js said 1.35 to 1.5 s behind the live edge |
| The same player without `lowLatencyMode` | 6.1 s behind the live edge |
| Network throttled to 1.4 Mbit/s | one stall of a few hundred milliseconds, then 360p within 0.1 s, playing 2.7 s behind; with the throttle off it climbed to 480p, 720p and 1080p within 50 s |

The viewer rows are what sharing by reference buys: two percent of one core
to send 157 Mbit/s, and no memory per viewer at all. The simulated viewers
are `dev/hls-viewers.py`, each a thread doing what an LL-HLS player does:
blocking reloads, then every new part. The page is `/test/hls.html` with
`GMX_UI_DEV=1`.
