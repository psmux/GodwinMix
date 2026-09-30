# Why the planner copies first and shares every encoder

A church sends its service to YouTube, Facebook and its own website, and
records it. Done the obvious way that is four encoders, each decoding the
camera, scaling it and compressing it, and on a laptop with no GPU four
H.264 encodes at 1080p30 is about six cores. Most of that work is the same
work done four times.

The rendition planner exists to do each piece of work once. It is a pure
function in `crates/godwinmix-render`: it takes what every output asks for
and what every source carries and returns a graph, and it never touches a
pipeline. `docs/reference/renditions.md` has the rules in full; this page
is about why they are in that order.

## Copy costs almost nothing, so try it first

Remuxing an H.264 stream from RTMP into another RTMP connection moves bytes.
It does not decode a frame. The channel destinations already do this, and
two live 720p streams with three destinations cost the channel server about
1% of one core. An encode of the same stream costs a hundred times that.

So the first question for every output is whether the source already is
what it wants. Same codec, same size, same frame rate, a bitrate close
enough (a quarter either way unless the output says otherwise), and a
container that can carry the codec. When the answer is yes, the output
gets a copy and nothing else. A publisher sending 1080p30 H.264 at 6 Mbit/s
to a mixer that forwards it to three platforms at 1080p30 should cost no
more than the network.

The last condition matters more than it looks. An SRT feed in HEVC can be
copied into MPEG-TS or into enhanced RTMP, but a WebRTC viewer cannot take
it, so that one output becomes an encode while the others stay copies. The
decision is per output and per track: the video can be copied while the
sound is turned from AAC into Opus for WebRTC, which costs a few
thousandths of a core.

## When something has to change, change it once

Past the copy, the work is a chain: decode, scale, encode. The planner
gives each link an id made from what it does, so two outputs that need the
same link find the same node.

A source is decoded once, whatever reads it. A 720p picture is scaled from
1080p once, and every 720p encode reads that one. An encoder is started
once per distinct shape, and five outputs that want H.264 720p30 at
3 Mbit/s with the same keyframe interval share it: five Mux nodes reading
one encoder's bytes. This is the difference between six cores and one and
a half on the church's laptop, and it is also why the bitrate a person
leaves out is filled in from a fixed table rounded to 100 kbit/s. Two
outputs that both said nothing about bitrate must land on the same number,
or they would not share.

## A ladder has one keyframe interval

An adaptive stream switches between rungs at keyframes, and HLS segments
must start on one. If the 720p rung has a keyframe every two seconds and
the 480p rung every four, a player cannot switch cleanly at half of them.
So every encode of one source uses one interval: the shortest any rung
asked for. Platforms state a maximum, so the shortest satisfies all of
them. The cost is that one rung asking for a shorter interval restarts the
others, which is the right trade for something that only changes when a
person edits an output.

## Hardware first, and never a full device

A GPU encoder costs a tenth of a core, so it is always preferred when there
is one of the right codec. But consumer NVIDIA cards stop at a few sessions,
and a VideoToolbox engine has a throughput ceiling. The planner asks the
cost model how much room each device has and keeps count of what it has
already placed there in this plan. When the GPU is full, the next encode
goes to the next encoder in line, down to software, and the node says so in
words: "using h264-software-x264 because the GPU nvidia0 is full". A person
looking at the plan should never have to guess why their laptop fan came
on.

Software is never refused for room. Whether the whole plan fits on the CPU
is one question about the whole machine, and it belongs to the governor,
which answers it for every show and channel at once.

## Changing a running show

The plan is recomputed on every change to an output, and the diff between
the old and the new plan is what gets applied. Because ids come from the
work and not from a counter, adding a 360p rung to a running ladder is a
diff of three new nodes (its scale, its encoder and its Mux) and nothing
else restarts. Removing the last output that reads an encoder stops it.
For this to hold, planning must be cheap enough to run without a second
thought: 16 sources and 64 outputs plan in about 130 microseconds.
