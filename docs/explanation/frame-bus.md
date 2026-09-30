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

## Platforms

Linux and macOS are the same code apart from how the anonymous region is made.
On Windows the crate builds and `available()` returns `unsupported`: there is no
transport yet, and a show there decodes its own sources, which is what every
consumer does now, so nothing breaks. The Windows transport would be an
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
