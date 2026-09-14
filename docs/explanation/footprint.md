# Footprint

What GodwinMix costs to run, what it is allowed to cost, and how to check.

The promise is that no performance claim here is a guess. Every number comes
from `gmx bench` on a named machine at a named commit, with the command that
produced it printed beside it, and the whole table is re run every release. OBS
publishes no CPU or memory requirement at all; the public record is forum
anecdote ranging from three percent idle to a 24 GB leak. A person sizing a
Raspberry Pi or a mini PC cannot plan against that, so we print the table
instead.

## The budget

"Target" is what the plan commits to. "m4pro" is the measured column from
`bench/results/m4pro-2026-09-14.md`, an Apple M4 Pro with 24 GB and GStreamer
1.28.7. The reference machines that matter most (`pi4`, `pi5`, `n100`) are not
measured yet, and an empty cell means exactly that.

| Measure | Target | m4pro | pi4 | pi5 | n100 |
|---|---|---|---|---|---|
| Core idle RSS, no sources, multiview off | at most 60 MB | 58.0 MB | | | |
| Core idle CPU, no sources, multiview off | at most 1 percent of one core on `pi4` | 0.022 cores | | | |
| Added RSS per file source, container mode | at most 40 MB | 34.7 MB (in process) | | | |
| Added CPU per 720p30 file source into the compositor, no encode | at most 15 percent of one core on `pi4` | 0.000 cores (hardware decode, see below) | | | |
| Multiview encoder, 8 fps mosaic 1280 wide | at most 10 percent of one core on `pi5`, and 0 with no subscriber | 0.025 cores, and 0 with no subscriber | | | |
| Snapshot and motion tracker | at most 5 percent of one core on `pi5`, and 0 when disabled | 0.062 cores, and 0 when nothing asks | | | |
| 720p30, two live sources, programme encode | `pi4` hardware at most 1.0 core; `n100` at most 0.6; `pi5` software at most 2.0 | 0.700 cores hardware, 0.880 software | | | |
| Cold start, process exec to first encoded programme frame | at most 2.0 s on `pi4` | 0.15 s | | | |
| Core binary | at most 30 MB, plus the platform's GStreamer (about 19 MB) | 8.4 MB | | | |
| A crossfade between two eight item scenes at 1080p30 | `gpu` and `n100`: no dropped frame | not yet, scenes do not exist | | | |
| Sixteen hidden slots at alpha 0 | within 2 percent of the no compositor baseline | not yet, scenes do not exist | | | |

## What each row means

**Core idle.** The daemon with the control server up, no sources, no outputs
and nobody subscribed to anything. It is the floor a plugin author inherits
before their plugin starts. Note what is inside it: GodwinMix builds and plays
the programme pipeline, encoder included, from boot, because the design is that
the output encoder starts once and never restarts. An idle GodwinMix is
therefore encoding a black slate. On a machine with a hardware encoder that
costs almost nothing; on a Pi 5, which has no H.264 encoder at all, it will not
be nothing, and this row is where that shows up.

**Added per file source.** One 720p30 file decoded into the compositor with no
encode on the far side, measured as the difference between a compositor at rest
and the same compositor with the file in it. On a machine that decodes in
hardware the decode happens somewhere else: macOS runs VideoToolbox in its own
XPC process, so the mixer's own CPU barely moves and the number describes the
mixer rather than the machine. The 40 MB target is for container mode, where a
sidecar plugin also pays about 41 MB for its own GStreamer process; the number
here is in process and so is the cheaper half of it.

**Multiview.** The mosaic the operator watches, and the only source of frames
for the snapshot routes. It does not exist until a client subscribes, and it is
taken down two seconds after the last one leaves, so the row is genuinely zero
when nobody is looking: not a small number, no pipeline at all. See
`crates/godwinmix-core/src/multiview.rs`.

**Snapshot and motion tracker.** Decodes each mosaic frame to luma and scores
the change per cell, which is what lets an agent ask "what is moving" for
thirty six tokens instead of a picture. It follows the mosaic only while
something is asking, and stops a few seconds after the last ask. The measured
figure is the difference between the mosaic alone and the mosaic with the
tracker on top, so on a fast machine it sits close to the noise between two
windows.

**Two live sources with programme encode.** The shape of a real small
broadcast: two cameras, a compositor, one encode at 720p30 and 2,500 kbit/s,
which is the default output preset. Both the software x264 path and whatever
the codec probe picks on the machine are measured. The software row's resident
memory is large (about 700 MB on a fourteen core machine) because x264 sizes
its lookahead and its frame threads from the core count; on a four core board
it is far smaller, and that is one more reason the Pi numbers have to be
measured rather than extrapolated.

**Cold start.** Wall clock in the parent process from `exec` of the binary to
the first buffer out of the programme encoder in a child. It includes dynamic
linking, GStreamer registry load, codec probing, pipeline construction and
preroll.

**Binary size.** The release binary, unstripped, one executable with no
interpreter. The platform's GStreamer packages are on top of it and are not
counted, because they are shared and are what the target machine already has.

## How to run it

```
cargo build --release
./target/release/gmx bench --machine laptop
```

The table goes to stdout and to `bench/results/<machine>-<date>.md`. Add
`--json` in CI and `--budget` to make a row over its target fail the build.
`--only <row>` runs one row, which is the way to get a clean resident memory
figure: within one run, an "added" row is a difference between two readings in
the same process, and a later row starts from whatever an earlier one did not
hand back to the operating system.

Build in release. A debug build makes every CPU number wrong by a large factor
(the motion tracker alone reads about fifteen times its real cost), and the
table prints a warning at the top when it notices.

## How the sampling works

CPU comes from `getrusage(RUSAGE_SELF)` on Unix: every thread of this process,
no children, differenced across the window and divided by the wall clock, so
1.0 means one core busy. Resident memory comes from `/proc/self/statm` on
Linux, from `ps -o rss=` on macOS (which has no `/proc`, and no current
resident figure in libc), and on Windows from a single
`powershell Get-Process` call, which is the documented fallback and needs no
new dependency. Each row warms up for five seconds and is then watched for
thirty, with resident memory read six times across the window and the highest
reading kept.

Rows that need scenes, a GPU or a reference board that is not the machine in
front of you print as "not yet" with their target, so the table has the same
shape everywhere and a gap is visible rather than absent.

## Turning things off

Both of the subsystems that cost something on behalf of a client who may not be
there have a switch, and off means absent rather than idle:

```toml
[multiview]
enabled = false   # no mosaic pipeline, no thumbnail end on any source

[snapshot]
enabled = false   # no motion tracker; the snapshot routes answer 404
```

With multiview off the snapshot routes answer 404 too, and say which switch did
it. `agent.state` still works and still tells an agent what is live; it reports
`motion: null` and no snapshot URLs.

For a machine where this matters most, see
`docs/how-to/run-on-a-raspberry-pi.md`.
