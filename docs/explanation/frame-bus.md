# The frame bus: decode once, read everywhere

A camera, a capture card or an incoming stream is opened and decoded once, by
one owner, and every consumer reads that one decoded picture. That is
Decision 4 of [the shows plan](../../dev/plans/shows-and-renditions.md). Shows
are separate processes so that one failing takes nothing else with it, which
means the decoded frames have to cross a process boundary, several times a
frame, without costing what a second decode would have cost.

This page says what the bus had to promise, what was measured before choosing
how to build it, how it works, and what it does not do yet. The surface is in
[the reference](../reference/frame-bus.md).

## What it has to promise

1. The owner never waits on a reader. A show that stops reading, or reads
   slowly, costs the owner nothing and costs the other readers nothing.
2. A reader that falls behind skips to the newest frame. It never reads an
   old one and never makes anyone queue for it.
3. A reader that dies, even killed mid read, leaves nothing locked.
4. Readers get the frame in place, with no copy.
5. It runs on a CPU. A GPU is not required for any of it.

The first three are the programme never stopping, applied to sources. The last
two are what make it worth having: a copy per reader at 1080p is 3 MB per frame
per show, and the whole point is to spend less than a second decoder would.

## What was measured

Three ways to move decoded frames between processes were tried against the
first promise before any code was written.

**`shmsink` and `shmsrc`.** One shared memory area of fixed size, which the
writer copies each buffer into, and readers are told where. A reader that does
not release its buffers fills the area, and then the writer blocks. With one
reader sleeping half a second per buffer, an owner meant to push 10 seconds of
1080p30 was still going after 40.

**`unixfdsink` and `unixfdsrc`.** The core already uses these for plugin media
and local previews on Linux and macOS. Each buffer's memory is a descriptor
sent to every reader, which maps it; when upstream allocates from the sink's
own pool the frame is never copied at all. That is also its problem. The pool
is the owner's, and a reader holding buffers holds the owner's pool. In the
benchmark below, an owner whose decoder wrote straight into `unixfdsink`'s
pool (I420, no conversion) published 10 frames in 10 seconds once one reader
held each frame for a second. With a conversion in front, which allocates its
own memory and so makes `unixfdsink` copy, the owner kept up, but then it is
a copy anyway, and each reader paid about 2.4% of a core to map and unmap a
new descriptor per frame, twelve times what a frame bus reader costs.

**A ring of slots in shared memory.** A handful of frame sized slots in one
region, a small header of atomics saying which slot holds the newest frame,
and a lease per reader per slot it is reading. This is what the crate is.

Sharing the decoder's own GPU memory by handle (an IOSurface on macOS, a
DMA-BUF on Linux from a VA decoder) was also looked at. It would save the one
copy the owner makes. It is not built, for three reasons: on a machine with no
GPU, which is the reference machine, the decoder's output is in system memory
anyway; the compositor that reads it is the software one on those machines, so
the frame has to be in system memory before it is used; and on macOS an
IOSurface crosses processes only as a Mach port, which needs a second channel
beside the Unix socket. The header has room for it. When a GPU path exists end
to end, which is the rule in the plan's Performance section, a slot can carry a
handle instead of bytes, and the lease protocol does not change.

## How it works

The owner makes one anonymous shared memory region per format: `memfd_create`
on Linux, and on macOS a POSIX shared memory object that is unlinked the moment
it is made, so in both cases the only way in is the descriptor. The region
starts with a header, then the slots, each a whole number of pages.

A reader connects to the owner's Unix socket, found by name in a directory the
station owns. The owner gives it a reader place in the header and sends the
region's descriptor over the socket. The reader maps the header read and write,
because its leases live there, and the frames read only.

**Writing.** The owner picks the lowest numbered slot that is not the newest
frame and that no reader leases, writes the frame into it, stamps it with the
next sequence number, and points the header's `latest` at it. Then it sends
each reader one byte, without blocking. If a reader's socket is full, the byte
is dropped: the reader will find the newest frame in the header on the next
one. If every slot is leased, which needs readers to hold far more than they
are allowed, the owner drops the frame and counts it. It never waits.

**Reading.** A reader woken by its byte reads `latest`, sets the lease bit for
that slot in its own place, and checks the slot still holds the sequence number
it read. If it does, the frame is its to read in place until it drops it. If it
does not, the owner got there first, and the reader lets go and tries the new
newest frame. A reader that took frame 10 and next looks when frame 17 is out
gets frame 17, and `skipped` says 6.

**The one race.** The owner marks a slot as being written and then reads the
leases; a reader sets its lease and then reads the slot's sequence number. All
four operations are sequentially consistent atomics, so in their single order
one side comes second and sees the other, and that side backs off. A test runs
four reader threads against 20 000 frames and checks every frame's checksum.

**A reader that dies.** Its leases are in its place in the header, and the
owner holds its socket. When the socket closes, which the kernel does for a
process killed with SIGKILL, the owner's bus thread clears the place. A test
kills a reader holding a frame in a ring of three slots and checks that the
owner goes on without dropping anything and that a new reader gets the place.

**An owner that dies.** Readers keep the frames they hold (the mapping stays
valid while they have it open), `next` returns nothing, and they attach to the
next owner of the name by themselves, whatever size or format it publishes.
The new owner finds the old socket, sees nothing answers on it, and replaces
it.

**A new format.** The owner makes a new region and sends it to every reader;
frames held from the old one stay readable until dropped.

**Memory.** A region has room for `max_readers * leases + 2` frames, 26 at the
defaults, but pages are only allocated when first written, and the owner always
reuses the lowest free slot. With readers keeping up, a 1080p region has pages
in two or three slots (a test checks this with `mincore`).

**The one copy.** The owner copies each decoded frame from the decoder's memory
into a slot: one copy per frame, however many readers. At 1080p that is 3 MB
at memory speed, which a profile of the owner puts below half a percent of a
core. It could be removed by offering the slots to the decoder as its buffer
pool, so it decodes into shared memory directly. That is not done, because a
decoder keeps reference frames in its buffers, and a slot held as a reference
is a slot readers cannot have.

## Sound

The bus carries sound too, under the same name with `#audio` after it. A
channel stream has a picture and sound that must stay together, and deciding
the stream once is only half done if every show still decodes the sound for
itself from a second copy of the stream, on a timeline of its own.

A picture and a chunk of sound want opposite things from the ring. A reader
wants only the newest picture, and missing one is fine. It wants every chunk of
sound, in order, because a missing chunk is a click. So a sound region is used
the other way round: the owner overwrites the oldest free chunk rather than the
lowest numbered one, which leaves the chunks a reader has not reached yet for
last, and a reader leases the chunk after the one it had rather than the
newest. A slot holds up to 100 ms and records how much of it the chunk filled.
One rule is kept from pictures: a reader more than eight chunks behind jumps to
the newest, because sound that late is as useless as a late picture.

## In the mixer

A source whose plugin declares `share` in its manifest (the camera, screen
capture, and `ingest/rtmp` reading a channel) is a `SharedSource` in
`crates/godwinmix-core/src/plugin/host/shared/`. The plugin does nothing
different, which is what lets a third party camera plugin have this by writing
one line in its manifest.

**Who opens the device.** Two mixers that both want a camera have to agree
which of them opens it before either does, and the agreement has to end by
itself when the one that won dies, however it dies. An exclusive `flock` on a
lock file beside the name's socket does both: one holder at a time across
every process, and the kernel lets go when the holder's descriptor closes,
which it does for a process killed with SIGKILL. Every shared source has a
thread that asks for it every 50 ms. Asking is one system call that fails at
once and touches nothing the owner has.

**Every source reads the bus, the owner's own included.** The source the mixer
holds is always `gmxbussrc` in front of the normaliser every source has.
Whether this source is also the one running the plugin is the owner thread's
business, and it can change while the source runs without the mixer seeing
anything change. That is what makes the handover cheap: when the owner goes,
a reader takes the claim, starts the plugin, publishes under the same name,
and every reader's `gmxbussrc`, its own included, finds the new owner by
itself. No pipeline is rebuilt anywhere.

**The feed.** The owner runs the plugin exactly as an unshared source would
(its socket or its pipe, then the normaliser) in a pipeline of its own, and
hangs `gmxbussink` on the normaliser's tees where the branch to the programme
would be. It is not in the programme's pipeline, so a feed that stalls is a gap
for its readers and nothing else, and its failures are polled for rather than
handled on a bus handler. A camera that sent pictures and then nothing for
3 s is reopened, with the claim given up for a second first so another source
can try. A channel that goes quiet is left alone: nobody is publishing to it.

One thing had to change in that normaliser. Its `videorate` fills gaps by
holding each frame until the next arrives, and whether it holds one depends on
where the frames fall against its output grid. With it as it was, the first
measurement of a reader in another mixer came to 33.8 ms from publish to
programme, one frame. The feed's `videorate` now only drops, which holds
nothing; every reader's own normaliser still fills gaps for its own canvas.

**Keeping sound and picture together.** A picture alone is stamped when it
arrives, on the reading mixer's clock, which is what a camera plugin does with
its own frames. Sound and picture together cannot be stamped that way: the
owner decodes the picture more slowly than the sound, and stamping on arrival
would put that difference into the programme. So both tracks keep the owner's
timestamps, which its demuxer made from one clock, and the reader moves them
onto its own running time with one shift shared by the two, set by the first
buffer of either and set again, once for both, when a new owner starts a new
timeline. The later track arrives a little behind its time by the decoder's
delay, as it does for a source that decodes for itself.

**Where it is not used.** A source whose params name nothing to share (a
channel source that listens for a publisher of its own) is opened on its own.
So is anything on Windows, a source placed on a node, and everything when the
mixer runs with `GODWINMIX_FRAMEBUS=off`, which is how the unshared numbers
below were taken. A sound device is not shared: every desktop system lets
several programs open one, and each show keeps its own trim.

## Platforms

Linux and macOS are the same code apart from how the anonymous region is made.
On Windows the crate builds and `available()` returns `unsupported`: there is no
transport yet, and a source there opens its own device, as every source did
before the mixer used the bus, so nothing breaks. The Windows transport would be an
anonymous file mapping duplicated into the reader with `DuplicateHandle`, and a
named pipe per reader for the nudges and for noticing it die. The header and
the lease protocol would not change.

## The numbers

On an Apple M4 Pro (14 cores) under macOS 26, with other builds running on the
machine, `dev/framebus-bench.sh --repeat 3`, then again with `--format I420`.
The clip is 1080p30 H.264 at
6 Mbit/s, decoded in real time by `avdec_h264`. In the first table the owner
converts to NV12 before publishing, because NV12 is what hardware decoders give
and what a show will be handed. Every reader reads every cache line of every frame once, the
way a compositor reading it would, and so does every consumer that decodes for
itself. CPU is percent of one core over a 10 second window measured by each
process with `getrusage`. Latency is from the owner being handed a decoded
frame to a reader holding it, on the monotonic clock every process shares.

**NV12, as a show would want it** (the owner converts; a consumer decoding for
itself does not, which flatters the rows where each consumer decodes):

| Scenario | Owner CPU | Reader CPU, mean / max | Total CPU | Owner frames, dropped | Frames per reader, min | Skipped per reader, max | Latency p50 / p99 / max ms | Stalled reader got / skipped |
|---|---|---|---|---|---|---|---|---|
| frame bus, owner alone | 11.4 |  | 11.4 | 300, 0 |  |  |  |  |
| frame bus, 1 reader | 10.8 | 0.2 / 0.2 | 11.0 | 300, 0 | 300 | 0 | 0.07 / 0.16 / 0.68 |  |
| frame bus, 1 reader, one stalled | 11.1 | 0.0 / 0.0 | 11.1 | 300, 0 |  |  |  | 10 / 290 |
| frame bus, 4 readers | 12.0 | 0.2 / 0.2 | 12.8 | 300, 0 | 300 | 0 | 0.13 / 0.74 / 1.36 |  |
| frame bus, 4 readers, one stalled | 12.9 | 0.2 / 0.3 | 13.6 | 300, 0 | 300 | 0 | 0.11 / 2.17 / 4.79 | 10 / 291 |
| frame bus, 8 readers | 12.1 | 0.2 / 0.2 | 13.7 | 301, 0 | 300 | 0 | 0.13 / 0.40 / 0.70 |  |
| frame bus, 8 readers, one stalled | 11.1 | 0.2 / 0.2 | 12.4 | 301, 0 | 301 | 0 | 0.14 / 0.75 / 6.54 | 10 / 291 |
| each decodes itself, 1 reader |  | 8.5 / 8.5 | 8.5 |  | 300 | 0 |  |  |
| each decodes itself, 4 readers |  | 8.7 / 8.7 | 34.7 |  | 300 | 0 |  |  |
| each decodes itself, 8 readers |  | 8.9 / 9.0 | 70.9 |  | 299 | 0 |  |  |
| unixfdsink, 1 reader | 10.9 | 2.4 / 2.4 | 13.3 | 299, 0 | 299 | 0 |  |  |
| unixfdsink, 1 reader, one stalled | 10.9 | 0.1 / 0.1 | 11.0 | 300, 0 |  |  |  | 10 / 0 |
| unixfdsink, 4 readers | 11.0 | 2.5 / 2.6 | 21.2 | 300, 0 | 300 | 0 |  |  |
| unixfdsink, 4 readers, one stalled | 11.0 | 1.9 / 2.5 | 18.6 | 300, 0 | 300 | 0 |  | 10 / 0 |
| unixfdsink, 8 readers | 11.3 | 2.6 / 2.7 | 32.2 | 300, 0 | 300 | 0 |  |  |
| unixfdsink, 8 readers, one stalled | 11.6 | 2.3 / 2.6 | 29.9 | 300, 0 | 300 | 0 |  | 10 / 0 |

**I420, as the decoder gives it** (no conversion anywhere, so the owner rows
are the decode plus the bus, and the comparison is like for like):

| Scenario | Owner CPU | Reader CPU, mean / max | Total CPU | Owner frames, dropped | Frames per reader, min | Skipped per reader, max | Latency p50 / p99 / max ms | Stalled reader got / skipped |
|---|---|---|---|---|---|---|---|---|
| frame bus, owner alone | 8.8 |  | 8.8 | 300, 0 |  |  |  |  |
| frame bus, 1 reader | 8.8 | 0.2 / 0.2 | 9.0 | 300, 0 | 300 | 0 | 0.08 / 0.58 / 1.59 |  |
| frame bus, 1 reader, one stalled | 8.7 | 0.0 / 0.0 | 8.7 | 300, 0 |  |  |  | 10 / 291 |
| frame bus, 4 readers | 8.9 | 0.2 / 0.2 | 9.7 | 300, 0 | 300 | 0 | 0.10 / 0.59 / 0.72 |  |
| frame bus, 4 readers, one stalled | 8.8 | 0.2 / 0.2 | 9.4 | 300, 0 | 300 | 0 | 0.09 / 0.43 / 1.88 | 10 / 291 |
| frame bus, 8 readers | 9.3 | 0.2 / 0.3 | 11.2 | 300, 0 | 300 | 0 | 0.21 / 1.82 / 2.71 |  |
| frame bus, 8 readers, one stalled | 8.9 | 0.2 / 0.2 | 10.3 | 300, 0 | 300 | 0 | 0.14 / 0.47 / 2.84 | 10 / 291 |
| each decodes itself, 1 reader |  | 10.0 / 10.0 | 10.0 |  | 299 | 0 |  |  |
| each decodes itself, 4 readers |  | 8.8 / 8.8 | 35.2 |  | 300 | 0 |  |  |
| each decodes itself, 8 readers |  | 8.6 / 8.7 | 68.9 |  | 299 | 0 |  |  |
| unixfdsink, 1 reader | 8.7 | 2.4 / 2.4 | 11.1 | 300, 0 | 300 | 0 |  |  |
| unixfdsink, 1 reader, one stalled | 0.4 | 0.1 / 0.1 | 0.5 | 10, 0 |  |  |  | 10 / 0 |
| unixfdsink, 4 readers | 8.8 | 2.5 / 2.6 | 18.9 | 300, 0 | 300 | 0 |  |  |
| unixfdsink, 4 readers, one stalled | 0.3 | 0.1 / 0.1 | 0.7 | 10, 0 | 10 | 0 |  | 10 / 0 |
| unixfdsink, 8 readers | 9.1 | 2.6 / 2.7 | 29.9 | 300, 0 | 300 | 0 |  |  |
| unixfdsink, 8 readers, one stalled | 0.3 | 0.1 / 0.1 | 1.1 | 10, 0 | 10 | 0 |  | 10 / 0 |

Reading the tables:

* Publishing costs nothing that shows above run to run noise. In I420 the
  owner alone is 8.8% of a core and a consumer decoding for itself is 8.6 to
  10.0; the owner does not grow with readers beyond about half a point at
  eight. In NV12 the owner is about three points dearer, and that is the
  conversion, not the bus.
* A frame bus reader costs about 0.2% of a core, and holds a frame about a
  tenth of a millisecond after the owner was handed it (p50; p99 under 2.2 ms,
  worst 6.5 ms, on a machine with a load average near 9 from other builds).
* Four shows reading one camera cost the owner plus four readers: 9.7% of a
  core against 35.2% for four decodes. Eight: 11.2% against 68.9%.
* A stalled reader got one frame a second and skipped the rest. The owner
  published all 300 frames, dropped none, and every other reader got all 300.
* `unixfdsink` fed straight from the decoder stopped the owner as soon as one
  reader stalled: 10 frames in 10 seconds, and with it every other reader got
  10. Behind a conversion it kept up, at 2.4% of a core per reader.

Rerun it after any change to the crate; a number that gets worse needs a
reason in the commit.

## Measured in the mixer

Two mixers on one MacBook Pro (M4 Pro, macOS 26), each started from a copy of
`godwinmix.example.toml` with its control port changed and one source, the
built in camera at the example's 1080p30 canvas. Release build. Other agents'
builds were running on the machine throughout, with a load average between 6
and 11, so every number is from a busy machine. CPU is percent of one core from
each process's CPU time over 60 s, for both mixers and every process they
started; memory is resident size at the end of the window.

| | Bus on | Bus off (`GODWINMIX_FRAMEBUS=off`) |
|---|---|---|
| Processes that opened the camera | 1 | 2 |
| Mixer A, its camera plugin | 2.9%, 5.4% | 2.4%, 4.7% |
| Mixer B, its camera plugin | 2.0%, none | 2.5%, 5.2% |
| Both, with every child | 10.3% | 14.9% |
| Resident memory, both, with every child | 707 MB | 684 MB |

On macOS the second mixer could open the camera without the bus: AVFoundation
lets two processes capture one camera. On most Linux cameras through V4L2 it
could not. Memory comes out about even: a camera process fewer (about 120 MB),
but the shared frames are counted in the resident size of each mixer that maps
them.

The latency the bus adds, from each mixer's own measurement of frames in the
same run (2,733 frames in the reader):

| | p50 | p99 | max |
|---|---|---|---|
| owner: from the plugin's frame reaching the mixer to its publish | 0.003 ms | 0.018 ms | 0.032 ms |
| reader in the other mixer: from publish to holding the frame | 0.21 ms | 0.44 ms | 2.8 ms |

Everything after that is the normaliser any source runs, whose `videorate`
holds a frame or not depending on where frames fall against its grid (in this
run the reader's held one, 33.4 ms, and the owner's own reader's did not,
0.22 ms). So the bus adds about a fifth of a millisecond to a picture.

A channel stream, 720p30 H.264 and AAC published with ffmpeg to the first
mixer's channel, flashing white and beeping for 100 ms at the top of every
second, and both mixers recording their programme:

| | Bus on | Bus off |
|---|---|---|
| Mixer A (decodes the stream) | 13.1% | 11.9% |
| Mixer B | 2.5% | 11.9% |
| Sound minus picture in A's recording, median over 25 flashes | +9 ms | -8 ms |
| Sound minus picture in B's recording | -4 ms | +27 ms |

A reader's lip sync through the bus is as good as decoding for itself. After
the reader took the stream over, +14 ms.

**The handover.** With both mixers on the camera, the owner was killed with
SIGKILL six times, alternating which mixer owned it, and the survivor's own
log says how long its picture stopped: 580 to 690 ms, the plugin taking 530 to
550 ms of that to start and show a first frame. The same six rounds at a
quieter moment earlier in the day gave 365 to 469 ms with the plugin starting
in 222 to 246 ms, so the gap is the camera plugin starting on a busy machine,
plus up to 50 ms for the reader to notice and a frame. Either way it is well
inside the two seconds after which the mixer counts a source as stalled, and
the programme holds the last frame through it. For a channel stream the new
owner also waits for the next keyframe, so the gap is up to one keyframe
interval longer: 580 ms with a one second interval, with the owner's source
removed rather than its process killed, since that process also held the
channel server.

In the tests, with a shell plugin standing in for the camera, the gap is 110 to
230 ms (`cargo test -p godwinmix-core --test shared_source --test
shared_channel`).
