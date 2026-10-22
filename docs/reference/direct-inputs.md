# Reference: what a direct show takes in

A direct show (one with compositing off) takes one input and hands it to its
outputs without decoding it. This page is the input half: the `InputSpec` a
show is given, the addresses and params each transport reads, the
`InputStats` it reports, and what it carries. The code is
`plugins/ingest/src/direct/input/`. How the direct host runs shows and
outputs is its own page.

## InputSpec

```json
{"uri": "udp://@239.1.1.1:5000", "program": 2,
 "params": {"interface": "en0"},
 "backup": {"uri": "srt://10.0.0.9:9000"}}
```

| Field | Type | Meaning |
|---|---|---|
| `uri` | string | the address; its scheme picks the input (below). A bare string in place of the object is read as `uri` alone |
| `program` | integer, 1 to 65535 | the MPEG-TS program to take from a multiplex. Left out, the first program in the PAT |
| `params` | object | per transport, below |
| `backup` | InputSpec | run beside the main input and switched to when the main stalls (below) |

A spec that cannot be used is refused before anything opens, with a sentence
and `data.field` naming what to change: an unknown scheme lists the ones
there are in `data.schemes`, a program of 0 or over 65535 names `program`, a
file that is not there names its path, an RTSP transport that is not one of
the three names `params.transport` with `data.allowed`.

## Addresses and params

| Address | What it opens | Params |
|---|---|---|
| `udp://@239.1.1.1:5000`, `udp://0.0.0.0:5000` | MPEG-TS on a multicast group or a unicast port | `interface` (a name such as `en0`, or its address) to join on; `source_address` for source specific multicast, or write `udp://10.0.0.9@232.1.1.1:5000`; `receive_buffer_kb` (64 to 262144, default 16384, or 4096 with the GStreamer engine); `engine`: `gstreamer` for the GStreamer input this one replaced (below) |
| `rtp://@239.1.1.1:5000` | the same, in RTP (RFC 2250, SMPTE 2022-2). A `udp://` address takes RTP too: each datagram says which it is | as `udp://` |
| `srt://10.0.0.9:9000` | SRT caller, dialling a sender that listens | `latency_ms`, `passphrase`, `streamid` |
| `srt://@:9000`, `srt://0.0.0.0:9000`, or `params.mode` `listener` | SRT listener on that port, waiting for a sender to call. The port stays open for as long as the input exists: a sender that hangs up leaves the input quiet, the next one to call is read in its place, and one that calls while another is sending takes over from it | `latency_ms`, `passphrase`. Any stream id is taken |
| `rist://@0.0.0.0:5004` | RIST Simple Profile, listening. The port must be even: RTCP is on the next one up | none |
| `rtsp://user:pass@camera/stream1` | RTSP pull from a camera or an encoder | `transport`: `tcp`, `udp` or `auto` (UDP first, the default); `latency_ms` (default 200) |
| `https://host/live.m3u8`, `https://host/live.mpd` | HLS or DASH pull, handed on at the clock's pace | none |
| `rtmp://host/app/key`, `rtmps://...` | RTMP play from someone else's server | none |
| `file:///clips/loop.ts`, `/clips/loop.mp4` | a file, played at its own pace and from the top again at its end | none |
| `channel:church/main` | a channel's stream, read off the hub with no demux at all | none |

A password in an address never appears in a message or in `error`: it is
written `user:***@host`, and an SRT `passphrase=` in the query is written
`passphrase=***`.

## What is carried

Video and audio come out as the hub's tags, FLV tag bodies, whatever the
transport. Nothing is decoded.

| Codec | How the tag carries it |
|---|---|
| H.264 | classic FLV (codec id 7), the AVC configuration as its sequence header |
| HEVC | enhanced RTMP (`hvc1`), the HEVC configuration as its sequence header |
| AAC | classic FLV (sound format 10), the AudioSpecificConfig as its sequence header |
| AC-3, E-AC-3 | enhanced RTMP v2 audio: sound format 9, then the FourCC `ac-3` or `ec-3`, then one sync frame. No sequence header; each frame carries its own |
| MPEG audio layer II | enhanced RTMP v2 audio with the FourCC `.mp3`, which names MPEG audio; the frame header says layer II, and `audio_codec` reads it as `mp2` |
| MP3 | classic FLV (sound format 2) |

One video and one audio stream are taken, the first of each to appear. A
second audio track, teletext, subtitles and MPEG-2 video go nowhere, and
`error` says so once the input is live: `left out: a second audio stream
(audio/mpeg); the first is taken`.

## UDP and RTP: one socket per group

A `udp://` or `rtp://` input is read in Rust, with no GStreamer between the
socket and the hub. Every show that takes a program from the same group
(the same address, port, source and `interface`) shares one socket, joined
once, and one thread that reads it:

* The thread reads the PAT, PMT and SDT once for every show on the group,
  and hands each show the packets of its own program (its PMT, its PCR and
  its streams) through a queue of 512 batches. A show that falls behind
  loses packets in its own queue, counted in `packets_lost`; the thread
  never waits for it.
* On Linux it takes up to 32 datagrams in one `recvmmsg` call; elsewhere one
  receive and then whatever is already waiting. The receive buffer asked for
  is `receive_buffer_kb`; on Linux a buffer over `net.core.rmem_max` is
  asked for with `SO_RCVBUFFORCE`, which needs `CAP_NET_ADMIN`, and without
  it the buffer is what the ceiling allows.
* The socket closes when the last show on it goes.

Each show demuxes its program on its own thread: PES packets put back
together, the 33 bit PTS and DTS unwrapped, continuity errors counted per
PID, scrambled packets (transport scrambling control set) counted and
named in `error`, and the PMT followed as it changes. Times come from the
PES headers alone, counted from the program's first PCR as `tsdemux` counts
them, with one zero for the video and every sound track, so sound and
picture keep the distance the sender gave them. When the PCR jumps back, or
ahead by more than 3 s, or is flagged as a discontinuity, the new clock is
laid 40 ms after the old one and every stream moves with it.

The tags are byte for byte the ones the GStreamer input makes from the same
feed (`plugins/ingest/src/direct/input/native/tests/compare.rs` checks
H.264 with AAC, HEVC with 5.1 AC-3, and H.264 with MPEG layer II), with two
differences in what arrives: the native input hands on the first frames of
a feed, which the GStreamer input loses while it links its pads, and it
carries every sound track and MPEG-2 video when the show asks for them
(`InputSpec.carry`, set by the host for a show that converts).

An H.264 keyframe is an IDR, or an I slice behind a recovery point SEI,
which is how a broadcast encoder that never sends an IDR starts its GOPs;
HEVC's are IDR, CRA and BLA pictures. Both are what `h264parse` and
`h265parse` mark. A field coded H.264 stream is one tag per field, as
`h264parse` frames it, so its `fps` reads the field rate, 59.94 for 1080i
or 480i at 29.97 frames.

`params.engine = "gstreamer"` takes the GStreamer input instead: `udpsrc`,
the udp plugin's program filter and `parsebin`, one socket per show.

What every open group is doing is one row each in
`direct::input::native::registry::snapshot()`: address, interface, shows
on it, datagrams, bytes in, continuity errors, TS and RTP packets lost,
stuffing dropped, how long it has been silent, the receive buffer the OS
gave, and why the socket is not open when it is not. Nothing reads it over
the protocol yet.

## Backup

With `backup` set, both inputs run all the time, so the backup is warm when
it is needed. The show moves to the backup when the main has sent no frame
for `params.stall_ms` (default 2000) and the backup has, at the backup's
next keyframe; it moves back when the main has been steady for
`params.return_ms` (default 5000), at the main's next keyframe. Each move
sends the incoming side's codec headers first, and its timeline is laid a
frame after the last tag sent, so a reader sees one stream whose time never
goes back. While the backup carries the show, `error` begins `on the backup
input:` and then says why the main is not.

## InputStats

About once a second, whether or not anything arrived. What a multi program
UDP feed reported in the tests, taking program 2:

```json
{"audio_channels":1,"audio_codec":"mp2","cc_errors":0,"fps":25.03,"height":288,"kbps":1192,
 "keyframe_ms":999,"last_frame_ms":3,"packets_lost":0,"program":2,
 "programs":[{"name":"Sport","number":1,"provider":"FFmpeg","streams":["h264","aac"]},
             {"name":"News","number":2,"provider":"FFmpeg","streams":["h264","mpeg audio"]}],
 "state":"live","video_codec":"h264","width":352}
```

| Field | Meaning |
|---|---|
| `state` | `connecting` until the first frame; `live` while frames arrive (one within 3 s); `retrying` after a failure or a silence, with `error` saying which |
| `error` | why it is retrying, what of the feed is left out, or that the backup is carrying the show. Absent when there is nothing to say |
| `kbps` | for a TS transport (UDP, RTP, SRT, RIST) the rate of the chosen program with stuffing dropped, as received; for the rest, the media in the tags |
| `fps` | frames over the last second, timed by the stream's own clock, so a burst after a stall does not read as a fast feed |
| `width`, `height` | from the H.264 SPS or the HEVC configuration |
| `video_codec` | `h264` or `h265`, empty with no video |
| `audio_codec`, `audio_channels` | `aac`, `ac3`, `eac3`, `mp2` or `mp3`, and the channel count its header gives (6 for 5.1) |
| `cc_errors` | MPEG-TS continuity counter jumps on the program's PIDs, one per jump, as TR 101 290 (1.4) counts them. Kept across reconnects |
| `packets_lost` | what the transport says never arrived: RTP sequence gaps, the packets SRT or RIST gave up on, an RTSP camera's RTP losses, or for bare TS over UDP the TS packets the counters say are missing. Kept across reconnects |
| `keyframe_ms` | the gap between the last two keyframes, by the stream's clock |
| `last_frame_ms` | how long since the last video frame (the last audio frame for a feed with no video) |
| `program` | the TS program being taken |
| `programs` | every program of a TS feed, with its name and provider from the SDT and what each stream is, so a person can choose one. Absent for a feed that is not TS |

## Reconnecting

An input that fails or ends opens again on its own, after 1 s, then 2, 4, 8
and 10 s at most, and starts again at 1 s once a connection has held for 10
s. A pull (SRT caller, RTSP, HLS, DASH, RTMP) that sends no frame for 10 s
is closed and opened again. A UDP, RTP or RIST socket, and an SRT listener,
stay open through any silence, because whoever sends may start again at any
time. The one exception is a TS feed that was read before and keeps
arriving with nothing readable in it for 5 s, which is what a sender started
again with new PIDs and a new program number looks like: it is read again
from the start, and `error` says "is sending, but nothing in it could be
read". A feed that never carried anything readable is not rebuilt for it.
A file starts again from the top at its end with no wait. Each new
connection's timeline is laid after the last tag of the one before.

## Cost

Measured on an Apple M4 Pro with other work running, release build: one
input carrying 1080p25 H.264 at 8 Mbit/s and AAC, CPU as a share of one
core, the sender in another process.

| Input | CPU |
|---|---|
| UDP | 0.7 to 1.1 % |
| RTP | 0.7 to 1.4 % |
| SRT listener | 1.5 to 3.0 % |
| RIST | 1.7 to 2.1 % |
| File, looped | 0.7 to 0.9 % |
| RTMP pull | 1.3 to 2.0 % |
| HLS pull | 0.8 to 1.4 % |

The ranges are three runs on a machine other work was loading. Each input
also holds three to five threads while it runs: its own, the transport's,
and the demuxer's queue for each stream.

The test that measures it is `cpu_per_input_at_1080p_8_mbit` in
`plugins/ingest/src/direct/input/tests/cpu.rs`, run by hand in release.
RTSP is not in it, because its test server runs inside the test process.
