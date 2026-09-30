# Converting a channel's stream for one destination

A channel destination used to be one thing: the publisher's own bytes,
remuxed and sent on. That is still what it is unless it asks for something
else. This page is about the ones that do ask, why the work is split the way
it is between the core and the ingest plugin, and what it costs.

## Two processes, one decision

The ingest plugin is a process of its own. It holds the ports, carries every
channel's streams in its hub and runs one sender per destination. The core
holds the channels, the codec catalogue and the resource governor, and
already hears everything worth planning from: each stream's codec, size and
frame rate arrive as `event/channel.stream`.

So the decision is made in the core and the work is done in the plugin.

* **The core plans.** Every destination of a channel that asked for a
  rendition goes into one call of the planner (`godwinmix-render`), with the
  channel's live streams as the sources. The cost model the planner prices
  things with is the governor's calibration where there is one, the
  planner's fixed figures where there is not, and the codec catalogue for
  which encoders this machine has.
* **The core admits.** Every node of the plan that costs something (decode,
  scale, encode, audio encode) asks the governor for a ticket before it goes
  into the table, and holds it while it stays in the plan. A node that stays
  the same keeps its ticket, so a replan never asks again for what is on
  air. Copies and muxes ask for nothing.
* **The core hands over.** The channel table the plugin already receives
  gains, per stream, the list of nodes to build, each with the GStreamer
  element the catalogue chose and its properties already worked out into
  that element's units; and, per converting destination, the two nodes its
  video and sound come from.
* **The plugin builds.** It has no planner, no catalogue and no governor. It
  reads the stream from its hub like any other reader, feeds one decoder per
  stream, hangs a scale per size and an encoder per rendition off tees, and
  publishes each distinct pair of video and sound on a second hub, where the
  ordinary senders read it exactly as they read a copy.

The other arrangement written down in the plan, the plugin planning for
itself with a cost model handed to it and asking the core for tickets over a
new plugin to core call, would have put a second copy of the planner and a
new request direction into the plugin protocol for no gain: the core has
every input the plan needs already, and the governor, which must be one per
machine, lives there. With the decision in the core, `rendition.plan` answers
a channel's plan from the same place it answers the programme's, and a
refusal is the same Safety error with the same advice in both.

## Where the governor is

There is one governor per machine and the station owns it. Wiring it into the
station is the graph work's. Until that lands, the channels make one of their
own the first time a destination asks for a rendition, from the calibration
on disk if there is one, and start its sampler; a channel that only copies
makes nothing. The seam is `Channels::use_governor`, which the station calls
with its own governor once it has one, before the first channel converts.

Two things about the governor's view are worth knowing when reading its
refusals. It counts this process's own CPU as its own and every other
process, the ingest plugin included, as someone else's, so a running channel
conversion is counted once as a ticket and again, a moment later, in the
measured load of the others. And on a machine busy with other work it is
right to refuse: on the machine these numbers were taken on, other programs'
recent peak left nothing free for a while, and the governor refused a 720p
encode it would have granted a minute earlier. Both are the governor's to
refine; neither is worked around here.

## Copy stays a copy

A destination with no rendition never reaches any of this: its table row,
its hub reader and its sender are byte for byte what they were. One whose
rendition the stream already matches gets that same row too, so the copy
path is the copy path whatever the destination asked. Only a destination the
plan converts reads from the second hub.

## Decode once, share everything

However many destinations convert a stream, there is one reader of the hub
and one decoder. Each distinct size and frame rate is one scale off the
decoder's tee; each distinct rendition is one encoder off its scale's tee.
Three destinations asking for YouTube 720p read the bytes of one encoder: the
three senders hold the same `Arc` of each tag. Sound the stream already has
in the shape asked for is copied beside the converted picture rather than
decoded.

Hardware is used where the catalogue says the machine has it: `vtdec_hw` and
`vtenc_h264_hw` on a Mac, NVDEC and NVENC, VA, and so on. Hiding the hardware
entries with `GMX_CODEC_DISABLE` makes the same plan run on x264 and
`avdec_h264`.

## Nothing waits on a destination

Every branch off a tee starts with a small leaky queue on a thread of its
own, so an encoder that cannot keep up loses frames of its own. The reader
of the hub has the hub's bounded queue: if the decoder cannot keep up, the
queue drops whole GOPs, counted, and the publisher never waits. Each
converted pair is published on the second hub, where every sender has its
own bounded queue again. A dead destination reconnects on the usual backoff
and a slow one loses GOPs of its own; neither reaches the encoder, the other
destinations or the publisher. `transcode::tests` holds that to numbers with
a real publisher, a live receiver, a destination nobody answers and a reader
that never reads.

## The plan follows the stream

The plugin tells the core when a stream's codecs, size or frame rate change,
including the frame rate the publisher's `onMetaData` states, which is exact
where a second of measured frames cannot tell 29.97 from 30. The core plans
again and hands over the table. Node ids name the work (`scale:main:854x480p30`),
so the plugin keeps every node whose description is unchanged, and takes
down and rebuilds only the ones that changed and whatever reads from them,
each branch taken off its tee when the pad is idle. A new bit rate alone does
not replan: the stream is planned against the shape it had when its shape
last changed, so a plan does not flap between copy and encode as the rate
moves.

## What it cost

One 1080p30 6 Mbit/s RTMP publisher (a looped file sent with `ffmpeg -re -c
copy`), destinations to local `ffmpeg -listen` receivers, the ingest plugin's
own CPU over 20 seconds after 8 seconds to settle, on an M4 Pro with 14
cores shared with other work. The numbers are in `CHANGELOG.md` with this
feature and below.

| Destinations | VideoToolbox | CPU only |
|---|---|---|
| none, the publisher alone | 0.3% | 0.3% |
| three copies | 1.1% | 1.2% |
| three at YouTube 720p30 | 9.3% | 27.8% |
| one copy, one 720p, one 480p | 12.6% | 37.4% |

Percentages are of one core. Three copies measured 1.1% with the plugin as it
was before this change and 1.1% with it after, run back to back, so a copy
costs what it did. Three destinations at 720p cost one decode, one scale and
one encode, not three: every receiver got 1280x720 at 30 fps, at 2.9 to 3.2
Mbit/s. The mixed run's receivers got 1920x1080, 1280x720 and 854x480.

The CPU only run is the same core started with
`GMX_CODEC_DISABLE=h264-videotoolbox,h265-videotoolbox,h264-videotoolbox-any`,
which takes the VideoToolbox entries out of the catalogue, so the plan chose
`x264enc` and `avdec_h264` for the same requests.

On that CPU only run, eight destinations each asking for 1080p60 from the
30 fps source, one after another: two were admitted (the first, and one later
when other programs' load dipped) and six were refused with `Safety`, each
saying what it needed ("the H.264 1920x1080 at 60 fps encode on
h264-software-x264 for channel `bench` needs 3.0 cores and 0.1 cores is
free"), four of them with buttons for 1080p30, 720p30 and 480p30. The machine
was shared with other builds and tests throughout, at a load average between
10 and 12 on 14 cores, which is why so little was free; the two admitted
encodes ran at 92% of one core together.

## What is not here yet

* Codecs other than H.264 and AAC, in or out. RTMP here is classic RTMP and
  the SRT sender makes MPEG-TS from the same tags, so a destination sends
  H.264 and AAC; a stream that arrives as HEVC can be copied, not converted.
* A per destination preset lowering when the governor sheds: a shed channel
  conversion is stopped and brought back, never slowed.
* The graph work's `rendition.plan` handler answering the `channel:<id>`
  scope, which it does by calling `Channels::rendition_plan`.
