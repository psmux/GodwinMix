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

UDP, RTP, RIST and recording to a file all want MPEG-TS. GStreamer's
`mpegtsmux` would do it, behind an `appsrc`, a `flvdemux`, two parsers and an
aggregator, which is a pipeline and three or four streaming threads per
output. At two hundred shows that is the difference between hundreds of
threads and a thousand. The muxer in `plugins/ingest/src/tsmux/` is a few
hundred lines: Annex B for H.264 and HEVC with the parameter sets before
every keyframe, ADTS for AAC, PAT and PMT before every keyframe and at least
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
every rendition where the relay can hand it to another process. The station,
which serves HLS from the control port, can read any of them with nothing
decoded a second time. That is the honest way to HLS for a direct show, and
it is the station's half to write; until then a direct show has no HLS.

A show whose renditions already decode the input hands one decoded picture a
second from that decode to the vitals (`transcode::Tap`), so the vitals do
not decode its keyframes again. A show that only copies has its keyframes
decoded by the vitals themselves, only while its alarms are on or someone is
looking.

## Backup inputs

A backup is the input work's: `input/backup.rs` runs main and backup warm,
switches at the backup's keyframe when the main has been quiet for the stall
time, and goes back at the main's keyframe once it has been steady. Each
switch sends the incoming side's headers first and lays its timeline a frame
after the last tag sent, so the hub, and every output reading it, sees one
stream that never goes backwards and never needs to reconnect. The host only
reports it, in `direct.input`'s `backup`.

## Measured

On an M4 Pro, macOS, with the rest of the machine busy (load average 13 to
26 from other work during the runs), 50 shows, each a multicast MPEG-TS input
on the loopback (one 720p30 H.264 and AAC encode at about 2.8 Mbit/s fanned
out to 50 groups) and one UDP multicast copy output:

| | |
|---|---|
| CPU of the host | 52% to 68% of one core, about 1.2% per show; a 110% peak when the encode's rate rose to 4.3 Mbit/s a feed |
| Memory | 33 to 43 MB resident for the whole host |
| Threads | 153: per show the input's streaming thread, its runner (idle), and the output; three for the host |
| Dropped GOPs | 0 in every run |
| One output against its input, 60 s | 1800 frames out for 1800 in, no continuity error on the output |
| One show alone | 1.2% of one core, 15 MB |

In one 60 s run under a load average of 26, one output's timestamps jumped
about a second ahead and came back, twice, with every frame present; the
same feed recorded beside the host at the same time had no jump. Nothing
between the hub and the socket moves a time, so the jump is in the times the
stand in input took from `tsdemux`, whose live clock follows arrival when its
thread is starved. The muxer now keeps each stream's decode times going
forward whatever it is given, so a receiver never sees one go back, and the
input work's own runner should be measured the same way once it merges.

The numbers above used a stand in for the inputs (bare UDP into `tsdemux`
and the parsers on the socket's one thread), because the inputs were built
alongside. `gmx-ingest --direct <table.json>` runs the host alone on a table
file, so the same run can be repeated at any size with the real inputs.
