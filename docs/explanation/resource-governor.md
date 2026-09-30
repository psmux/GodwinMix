# The resource governor

A mixer that drops frames because somebody added a fourth output has broken
the one promise it makes. The usual way to avoid that is a setting: "maximum
CPU 80%", "keep two cores free", and a person guessing. The guess is wrong on
most machines, it stays wrong after a driver update, and it cannot know that
the laptop is also running a browser with forty tabs. So the governor works
the number out itself, and there is nothing to type (Decision 2 in
`dev/plans/shows-and-renditions.md`).

## One question, asked before anything starts

Can this machine do this without taking a frame from what is already on air?

Every encoder, scaler, preview and transcode asks before it starts, with a
`Cost`: thousandths of a core, a share of a hardware encoder, encoder
sessions, memory, uplink. There is one governor per machine, because a
second show or a channel transcode shares the same cores (Decision 3). If
the answer is yes the work gets a ticket and holds it while it runs. When it
stops, the ticket is dropped and the share comes back. If the answer is no,
nothing is started and the person is told why, in numbers, with something
that would fit.

Refusing up front is the whole point. Starting an encoder and watching the
programme stutter is how every other tool finds out it was one output too
many.

## Measure, do not guess

What an encode costs depends on the CPU, the encoder, its preset, the driver
and the GStreamer version. None of that is knowable from a table. So the
first time the mixer runs on a machine it spends about five seconds timing
each encoder the codec catalogue says is installed: a second of moving test
bars at 720p30 and at 1080p30, a few x264 presets, a scale, a decode and the
audio encoders. Hardware encoders have their session limit probed by opening
small sessions until one is refused, because a consumer NVIDIA card stops at
a number nobody publishes and VideoToolbox does not stop at all.

The result is stored under the mixer's runtime directory, keyed by a
fingerprint of the CPU, memory, OS, GStreamer version and the encoders
present. The next start reads it. A new GPU, a driver that adds or loses an
encoder, or a GStreamer upgrade changes the fingerprint and the machine is
measured again.

It never measures while a show is on air unless a person asks. The encodes
take every core they can get for a few seconds, which is what a measurement
needs and what a live programme cannot spare. On air with no measurement for
this fingerprint, it uses the last one it has and measures once the air is
clear.

Test bars are easier to encode than a camera. Pure noise, the worst case,
took x264 about 1.7 times as long as the scrolling bars on the M4 Pro this
was built on. Real pictures sit somewhere between, so software encode costs
carry a margin of 1.3 on top of what was measured. That figure is a
judgement, written down where it can be argued with.

## Headroom it works out itself

What is free is the machine less three things.

Other programs, at their peak over the last ten seconds. The peak rather than
the average, because a backup or a browser tab that spiked a few seconds ago
will spike again.

This process, at whichever is larger: what it measures, or what its tickets
add up to. A ticket granted a moment ago is not in the measurement yet. Work
started without a ticket (the compositor itself) is only in the measurement.
Taking the larger of the two covers both.

A reserve. On a desktop, where the page, the window system and whatever the
person has open share the machine, a fifth of it and at least a core and a
half. On a headless server, a twelfth and at least half a core. On top of
either, how far the machine's load has jumped above its mean lately, so a
bursty machine keeps more room than a steady one. The reserve never passes
half the machine. Someone who wants to hold CPU back for something else can
set `reserve_cores` under Advanced; nobody has to.

The load is sampled once a second from what the operating system already
counts: `/proc/stat` on Linux, the Mach host statistics on macOS,
`GetSystemTimes` on Windows. A sample costs a couple of microseconds, which
at one a second is a few millionths of a core. Hardware encoder load is read
where the platform gives it cheaply (AMD cards in sysfs) and otherwise
counted from what each ticket on the device declared at calibration.

## A refusal that helps

"Not enough CPU" is useless on its own. A refusal carries what was needed,
what is free, and for every encoder calibrated the largest standard shape
that would be granted right now:

```
the 1080p60 HEVC rendition needs 6.0 cores and 3.3 cores is free.
1080p30 H.264 on h264-software-x264 fits, or 1080p60 H.264 on the GPU encoder h264-gpu.
```

The same list rides in the error's `data`, so the page can offer each one as
a button rather than as text to act on.

A software encode that does not fit at its configured preset is offered the
slowest preset that does, before it is refused. A Pi gets `ultrafast` at
720p and is told so; a desktop gets a better preset for the same load.

## Live wins

Admission keeps the machine out of trouble at the moment something starts.
It cannot stop a thermal throttle an hour into a show, or somebody opening a
video editor on the same laptop. When measured load eats half the reserve,
`shed()` answers with what to give up, in a stated order:

1. thumbnails and previews, which exist only because somebody is looking;
2. the lowest rung of each ladder, then the next one up, never the top;
3. faster presets on software encodes, lower rungs first and the programme
   last.

The programme encode and the top rung of a live output are never stopped.
Each step carries the sentence for the alert: what was dropped and why. The
governor only decides the order. The caller does the stopping, because it
owns the pipeline and the governor does not.

## Why a crate of its own

Admission is arithmetic, and arithmetic should be tested in milliseconds with
made up machines, not with a camera. So everything but the timing is plain
Rust with no media stack, and GStreamer comes in only behind the
`calibrate` feature. The crate does not depend on the core: the core will
ask the governor before it starts a preview, and Cargo refuses a dependency
cycle even through an optional feature. The codec catalogue reaches
calibration as a list of candidates, each with a closure that applies the
catalogue's properties using the core's own code, so there is still one list
of encoders and one way to configure them.

The profile's method names are the planner's `CostModel` trait, so the
planner can price a graph with the numbers this machine actually produced.

## What it does not do yet

It is not wired into anything. Nothing asks it before starting today; the
station and the planner do that in the next phase. It does not read NVIDIA,
Apple or Windows encoder load, and it does not measure the uplink: an output
that measures its own throughput will set that.
