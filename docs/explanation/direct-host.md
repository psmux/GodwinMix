# The direct host

A headend has a feed per channel and wants each one sent on somewhere,
copied or converted, watched for black and silence, and nothing else. Two
hundred of those are two hundred shows. A show that composites is a process
with a compositor and a programme encode; two hundred of those would be two
hundred processes doing work nobody asked for. So a show with compositing off
does not get a process. It is a row in a table that one shared host runs,
inside the ingest plugin's channel server, beside the channels it is made of.

## What one show is

```text
  input (its own thread) ──► hub "direct.<id>/main" ──┬──► output, one thread each
                                                      ├──► transcode, once per show, only for renditions
                                                      ├──► vitals, keyframes only, only while asked
                                                      └──► relay, when a show that composites reads it
```

The input demuxes and parses, and hands the hub the same `MediaTag`s an RTMP
publisher's stream is made of. From there everything the channels already do
applies unchanged: the hub's bounded per reader queues, the restreamer's
sender with its backoff, the transcode graph built from the station's plan.
The direct host adds the table, the outputs that read the hub straight, a
muxer, and the events.

## One thread per output, and why the hub is the queue

A channel destination is two threads: a pump that drains whatever iterator
it was given into a bounded queue, and the sender that pops that queue onto
the network. That shape lets a destination read anything. A direct output
reads one thing, the hub, and a hub reader is already a bounded queue that
drops whole GOPs from the front when its reader falls behind. So the
restreamer's sender pops the hub reader directly (`restream::queue::Tags` is
the seam), and an output is one thread and one queue.

A tag is a pointer to its payload, shared by every reader. Fifty outputs of
one show hold fifty pointers to each frame, not fifty frames. What each
output does copy is its own container: an MPEG-TS output writes the frame
into transport packets, because each output has its own continuity counters
and its own clock. That copy is the output's work, on the output's thread.

A slow output fills its own reader's queue and loses GOPs there, counted in
`dropped_gops`. The input pushes into every reader's queue and returns; it
never waits on one. The test for this sends one output to a server that
accepts and never answers, holds a second reader that never reads, and
checks that the third output kept its frame rate and the input's slowest
push stayed under 20 ms.

## An MPEG-TS muxer of its own

SRT, UDP, RTP, RIST and recording to a file all want MPEG-TS. GStreamer's
`mpegtsmux` would do it, behind an `appsrc`, a `flvdemux`, two parsers and an
aggregator, which is a pipeline and three or four streaming threads per
output. At two hundred shows that is the difference between hundreds of
threads and a thousand. The muxer in `plugins/ingest/src/tsmux/` is a few
hundred lines: Annex B for H.264 and HEVC with the parameter sets before
every keyframe, ADTS for AAC, AC-3, E-AC-3 and MPEG audio as they came, PAT and PMT before every keyframe and at least
every 400 ms, the clock on the video PID 0.7 s behind the decode time as
ffmpeg puts it. It is tested against GStreamer's own `tsdemux` and libav, and
ffprobe reads its output clean.

One thing it learned the hard way: when the sound arrives before the first
picture, the first PMT lists sound alone, and GStreamer's demuxer keeps the
first PMT it read unless the version moves. The muxer moves the version
whenever the streams it announces change.

## Decode once, and only when asked

A show that only copies decodes nothing. An output with a rendition goes
through `crate::transcode`, the same code that converts a channel's stream:
the station plans it with the shared planner and admits it with its
governor, and the host builds what it is handed. One decode per show, one
scale per size, one encoder per rendition, however many outputs share them.

The renditions are published on the same hub as the input (`Transcoders::
sharing`), under `direct.<id>/main|<video node>|<audio node>`. That puts
every rendition where the relay can hand it to another process. The HLS
packager reads the one an HLS output asks for, or the show's own stream, with
nothing decoded a second time.

A direct show's HLS is packaged in a process of its own, the HLS
packager: the station's binary run with `--hls-packager`, which the station
starts while at least one such output is on. The packager, the rings that
hold the segments, the playlists, LL-HLS and DASH exist once, in the engine
and the control port's `/hls` routes, and that binary links both, so nothing
is written twice. Packaging means a GStreamer pipeline per output, `appsrc`
into `cmafmux`, and a pipeline can crash its process. In the host that
would take every channel and every direct show's outputs with it; in the
station it would take the control port. In the packager it costs the HLS outputs a few seconds: they say
`reconnecting`, with why, the station starts the packager again (after 1 s,
then 2, 4 and up to 30 s if it keeps stopping, and `failed` after five stops
in a row), hands it the outputs again, and each output comes back on the
same link. The viewer keys are the station's, so they do not change when
the packager does.

The station still answers `/hls/<output>/...?show=<id>` on its own port. It
finds the output, checks the viewer key or the token, and forwards the
request to the packager on loopback with a secret the packager was started
with; the packager answers from its rings, holding a blocking LL-HLS reload
as the control port does. Inside the packager each output is one thread and
one small pipeline; one that fails says so on its own output and nothing
else stops. `crates/godwinmix/tests/station/hls_isolation.rs` kills the
packager outright beside a second direct show and checks all of this.

A show whose renditions already decode the input hands one decoded picture a
second from that decode to the vitals (`transcode::Tap`), so the vitals do
not decode its keyframes again. A show that only copies has its keyframes
decoded by the vitals themselves, only while its alarms are on or someone is
looking.

## One socket per multicast group, read in Rust

A headend takes many programs from few multiplexes: fifty channels off
eight ATSC groups is six or seven programs a group. With one `udpsrc` per
show, seven programs on one group were seven joins, seven sockets each
taking the whole 30 to 40 Mbit/s multiplex, and seven demuxers each reading
all of it to keep one seventh. The `udp://` and `rtp://` input is now its
own code (`input/native/`), with nothing from GStreamer between the socket
and the hub:

```text
  socket thread, one per group ──► tables read once (the udp plugin's Filter)
    │  each show's own program's packets, a queue per show, never waited on
    ├──► show thread: PES ──► H.264 | HEVC | MPEG-2 | AAC | AC-3 | MPEG audio ──► tags ──► hub
    └──► show thread: ...
```

The socket thread is the only one that touches the network. It takes up to
32 datagrams a system call on Linux, reads the PAT, PMT and SDT for every
show on the group at once, counts the group's continuity errors, RTP gaps
and stuffing, and copies each show's packets into that show's queue with a
`try_send`. A show that falls behind loses packets in its own queue. The
lock on the list of shows is held while one batch is handed out, which is
copying, and never across a receive.

Each show's thread turns its packets into tags: PES packets put back
together, the times read off each PES header when it starts, unwrapped
across the 33 bit wrap and counted from the program's first PCR, the way
`tsdemux` counts them. The framing is the parsers', reimplemented: an
`avcC` or `hvcC` header and length prefixed access units, ADTS taken off
AAC, AC-3 and MPEG audio cut one frame per tag. The tags are byte for byte
what `parsebin`, the parsers and the tagger make of the same file, which
`native/tests/compare.rs` checks for H.264 with AAC, HEVC with 5.1 AC-3,
H.264 with layer II, and MPEG-2 with two AC-3 tracks, every track compared,
times within a millisecond. Sound and picture are kept as far apart as the
PES times say, to the millisecond, across a sender that restarts on another
clock (`native/tests/sync.rs`).

What that costs, measured with `dev/bench/mpts.sh` and the same run on a
real feed. CPU is a share of one core for the whole direct host, each show
with one UDP copy output and its alarms off; the second half of a 60 s run.

Nine programs on one multicast group on the loopback of a 64 core Linux
build server (one 720p30 H.264 encode at 3.5 Mbit/s with two AC-3 tracks,
copied into nine programs, about 35 Mbit/s):

| Input | CPU | Resident memory | Threads |
|---|---|---|---|
| native | 15.9 to 17.0 % | 18.4 MB | 24 |
| GStreamer | 95.3 to 106.6 % | 26.2 MB | 33 |

A box with 8 MPTS groups of ATSC programs (MPEG-2 video at 480i to 1080i,
AC-3 in one to three tracks) on a 48 thread Xeon, read from the real
multicast. Of the 50 programs the shows asked for, 45 were in the feed; the
other five were named in each show's `error` as not in it.

| Shows | What is carried | Input | CPU | Resident memory | Threads |
|---|---|---|---|---|---|
| 7, one group | MPEG-2 and every sound track | native | 6.1 to 8.3 % | 15.8 MB | 20 |
| 7, one group | MPEG-2 and every sound track | GStreamer | 12.8 to 13.8 % | 21.4 MB | 27 |
| 50, eight groups | the first sound track | native | 39.8 to 43.0 % | 25.1 MB | 113 |
| 50, eight groups | the first sound track | GStreamer | 94.1 to 152.2 % | 53.4 MB | 156 |
| 50, eight groups | MPEG-2 and every sound track | native | 50.2 to 55.9 % | 39.2 MB | 113 |
| 50, eight groups | MPEG-2 and every sound track | GStreamer | 118.5 to 164.5 % | 55.9 MB | 156 |

No GOP was dropped and no continuity error counted in any run. Per show the
native input is one thread and the GStreamer input two (its streaming
thread and its runner), beside each output's own; the native socket
threads are one per group, eight for fifty shows. `params.engine =
"gstreamer"` still takes the old input, for a feed this one reads wrong.

## Backup inputs

A backup is the input work's: `input/backup.rs` runs main and backup warm,
switches at the backup's keyframe when the main has been quiet for the stall
time, and goes back at the main's keyframe once it has been steady. Each
switch sends the incoming side's headers first and lays its timeline a frame
after the last tag sent, so the hub, and every output reading it, sees one
stream that never goes backwards and never needs to reconnect. The host only
reports it, in `direct.input`'s `backup`.

## Measured

On an M4 Pro, macOS, with the rest of the machine busy (load average 8 to 26
from other work during the runs), 50 shows, each a multicast MPEG-TS input on
the loopback (one 720p30 H.264 and AAC encode at about 2.8 Mbit/s fanned out
to 50 groups) and one UDP multicast copy output, about 140 Mbit/s in and out.
With the real inputs (`input/`, UDP through `udpsrc`, the udp plugin's
program filter and `parsebin`), 80 s:

| | |
|---|---|
| CPU of the host | 52% to 60% of one core, steady; about 1.1% per show |
| Memory | 42 to 58 MB resident for the whole host |
| Threads | 153: per show the input's streaming thread, its runner (waiting on the bus), and the output; three for the host |
| Dropped GOPs | 0 |
| Continuity errors on the inputs | 0 |
| One output against its input, 60 s | 1800 frames out for 1800 in, largest gap between two 34 ms, no continuity error, ffmpeg decodes it without a word |
| One show alone (stand in input) | 1.2% of one core, 15 MB |

An earlier run used a stand in for the inputs (bare UDP into `tsdemux` on
the socket's thread) and came out the same within a few percent, except
that under a load average of 26 one output's timestamps jumped about a
second ahead and came back, twice, with every frame present, while the same
feed recorded beside the host had no jump. Nothing between the hub and the
socket moves a time, so the jump was in the times that input took from
`tsdemux`, whose live clock follows arrival when its thread is starved. The
muxer keeps each stream's decode times going forward whatever it is given,
so a receiver never sees one go back.

`gmx-ingest --direct <table.json>` runs the host alone on a table file, so
the same run can be repeated at any size.
