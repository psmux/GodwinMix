# How late the picture is

A camera frame passes through several hands before an operator sees it: the
camera's own process, the pipe to the mixer, the source's pipeline, the
programme's compositor, the mosaic, the WebSocket and the page. Each one can
hold a frame. This page says what each stage holds, what was measured on
2026-10-06, and what the mixer does about a source that falls behind.

## The stages

```
camera sidecar ──pipe──> source pipeline ──> programme compositor ──> encoder ──> outputs
 (leaky, 4 frames)  (blocks at 16 MB)  │        (1 s budget, see below)
                                        └──> thumbnail ──> mosaic ──> JPEG ──> /rpc ──> page
                                                             └──> Studio preview ──> JPEG ──┘
```

| Stage | What it can hold |
|---|---|
| Camera sidecar | a leaky queue of four frames ahead of the Matroska muxer, so a core that stops reading costs the camera frames and never time |
| The pipe | on Windows an `appsrc` that blocks at 16 MB, about five raw 1080p frames (`input::pipe_source`) |
| Placement | the frames' place on the programme's timeline, decided once when the source's first segment arrives (`TimelineAligner`). This is where seconds went |
| Programme compositor | waits for an input only while that input has no frame for the moment it is drawing, at most its one second budget (`MIN_UPSTREAM_LATENCY_NS`). With every input on time it draws within tens of milliseconds |
| Mosaic and Studio preview | one compositor each, at the mosaic's rate (8 frames a second by default), then a JPEG |
| Transport | the JPEG on `/rpc`, two frames deep per client, newest kept. No HLS, no WebRTC and no video encoder on this path, so there is no GOP, B frame or rate control buffer to tune |
| Page | decode with `createImageBitmap`, then one `drawImage` per canvas |

## Placement, and the guard

A source in its own pipeline starts its timestamps near zero. The aligner
shifts it onto the programme's clock by the programme's running time when its
first segment arrives, and keeps that shift. A source whose first frames arrive
late keeps that lateness for good: every later frame is due as long after it
arrives as the first one had waited, and the compositors hold it that long.

The camera on Windows does exactly that. Its sidecar starts capturing at once;
its first frames wait in the pipe while the core types the stream and links the
decoder, then arrive in a burst. The installed app's webcam came up 0.6 to
0.9 s behind and stayed there (its log on 2026-10-06: the first picture's
running time plus the aligner's offset against the programme's, 582, 606 and
932 ms on three starts). On Linux and macOS the camera comes over `unixfd` and
stamps its frames from the programme's own clock, so no aligner runs there and
none of this applies.

So every frame entering the programme pipeline is measured: how long it will
wait before it is due, its lead. The supervisor reads the least lead of each
half second. When every frame for two seconds was due more than 200 ms after
it arrived, the source is moved earlier by the least lead seen, less 40 ms.
The frames already queued are then late, the compositors drop them, and the
picture jumps to the newest frame. Picture and sound move by the same amount,
so lip sync is kept. The log says so:

```
a live source was running behind its own frames; dropped to the newest
    source=cam behind_ms=1476 caught_up_ms=1436 total_ms=1436
```

A source that is not live, a stream that pushes as fast as the queues let it,
fills them again at once. If a second catch up inside ten seconds finds three
quarters of the first lead again, the guard leaves that source alone until it
restarts and says `this source filled its queues again straight after a catch
up, so it is not live`. A source that can be scrubbed is a file and is never
touched. `RUST_LOG=godwinmix_core::mixer::catch_up=debug` prints each source's
least lead every tick. The code is `crates/godwinmix-core/src/mixer/catch_up`.

## Measured

A debug build on this laptop (Windows 11, 16 threads, other builds running),
the canvas 1920x1080 at 30, the mosaic 1280 wide at 8 frames a second. A
source writes the wall clock into every frame as a barcode; a reader on the
WebSocket stamps each frame as it arrives and decodes it afterwards, and a
second reader takes the programme over SRT (20 ms of SRT latency, software
decode). Medians in milliseconds behind the wall clock, two rounds of 20
seconds each, every round within a few milliseconds of the other.

A source whose first 1.5 s of frames arrive at once, as the camera's do:

| Where | Before | After |
|---|---|---|
| The source's tile | 1649 to 1708 | 201 to 202 |
| The programme tile | 1676 | 234 to 235 |
| Studio preview | 1515 to 1570 | 212 |
| The programme output over SRT | 1648 | 257 |

A source that starts on time:

| Where | Before | After |
|---|---|---|
| The source's tile | 102 to 111 | 68 to 69 |
| The programme tile | 188 to 189 | 187 |
| Studio preview | 96 to 98 | 82 to 83 |

The mosaic on its own, from the core's test that writes the programme's
running time into each frame and reads it back out of the JPEGs
(`multiview::latency_tests`, a 640x360 mosaic): the source tile 113 to 144 ms
before and 34 to 35 after, the Studio preview 34 to 49 before and 35 to 48
after. Two things were making every tile late. Each tile had a second
`videorate` at the rate it already arrived at, which decides a frame only once
the frame after it has come, and so held every tile a frame. And the programme
tile was stamped a tenth of a second before it arrived, so each of its frames
ended a tenth of a second before the mosaic frame it was needed for, and the
mosaic waited for the next one before drawing anything. The programme tile now
shows the programme frame that has arrived rather than one the whole mosaic
waited for: 65 ms in that test against 35 before, while every other tile got
the time back.

The page, in headless Chrome against the same core, read by a probe on
`drawImage` and on the socket's messages (the source on time):

| | Before | After |
|---|---|---|
| A frame's message read by the page | 130 | 130 |
| The programme monitor painted, median | 200 | 140 |
| The same, 90th percentile | 262 | 171 |
| The same, worst | 491 | 181 |
| Paints of the monitor in 15 s | 1018 | 120 |

The Sources panel attaches every tile again each time it renders, and each
attach used to repaint every canvas on the page, the monitor included: about
sixty full repaints a second for eight new frames. Now an attach paints the
canvas being attached and nothing else.

## What is not measured

The real camera. It was in use by the installed app, so the camera's own
stage (the device, Kernel Streaming, the Motion JPEG decode in the sidecar)
was not timed, and the sidecar was run with `element = "videotestsrc"` instead.
On this debug core it came up between 10 and 300 ms ahead; once it was 304 ms,
and the guard moved it back by 243. The installed app's webcam came up further
ahead than that (0.6 to 0.9 s, above), and while it ran its tile's queue in the
mosaic stood full, two frames held by a compositor waiting for their time.
The installed app was only read, never changed, so the fix has not run there.

The programme tile moves between about 70 and 190 ms from one run to the next
with nothing changed. It is drawn from the programme at the mosaic's rate, and
where the programme's eight frames a second fall against the mosaic's eight is
set when each starts.

The programme compositor waits for its slowest input, up to its one second
budget, while that input has no frame for the moment it is drawing. A source
that delivers late holds the whole programme back by its lateness. That budget
is what keeps a layered source's sound whole (see `MIN_UPSTREAM_LATENCY_NS`),
so it was not changed here.
