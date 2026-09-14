# Run the soak test

`dev/smoke.sh` asks whether the mixer works. `dev/soak.sh` asks whether it still
works an hour later.

It starts a core, then every five seconds does one round of the things an
operator does: adds a source and removes one, takes between two scenes with a
fade, opens and closes an MJPEG stream and a PCM stream, installs a plugin and
removes it. After each round it writes down four numbers. At the end it prints a
table, writes the numbers to `bench/results/`, and exits non-zero if any of the
four went the wrong way.

```
dev/soak.sh                  # ten minutes
dev/soak.sh --minutes 60     # what the nightly runs
dev/soak.sh --minutes 10 --machine pi5 --keep
```

It builds in release first. A debug core misses the frame bar on its own,
before anything has gone wrong, so a soak built that way measures the compiler.

## What it writes down

| Number | Where it comes from | What it means |
|---|---|---|
| `gmx_programme_frame_stall_ms` | `/metrics` | The worst average gap between programme frames over sixty consecutive frames |
| Resident memory | `ps -o rss=` on the core | What the process is actually holding |
| Open descriptors | `/proc/<pid>/fd` on Linux, `lsof -p` on macOS | Sockets, pipes and files the core has not closed |
| Threads | `/proc/<pid>/task` on Linux, `ps -M` on macOS | How many threads the core is carrying |

The stall gauge is the one worth understanding. There is a second metric with a
similar name, `gmx_programme_frame_interval_ms`, and it is a histogram of raw
wall clock gaps: it includes every ordinary late thread wake up and it is not
what the 34 ms bar is written against. The stall gauge averages over sixty
frames, so a single hiccup averages out of it and a pipeline that really
stopped cannot hide in it.

## The four bars

| Bar | Fails when |
|---|---|
| Programme stall | the gauge ever reads more than 34 ms |
| Resident memory | it is more than 10 percent above the warm up sample at the end |
| Open descriptors | the last sample is more than 8 above the warm up sample |
| Threads | the last sample is more than 4 above the warm up sample |

The warm up sample is the first one taken at or after sixty seconds. The first
minute is where the pipeline builds its pools and the allocator takes its
arenas, so growth there is the core arriving at its working size. Comparing
against the start instead would fail every run for the wrong reason.

The slack on descriptors and threads is what one round in flight costs. A
sample is taken while the round is still settling: a socket the kernel has not
finished closing, an idle thread that GStreamer's pool or tokio's blocking pool
kept rather than tearing down, and a plugin on its way out. That last one is
worth knowing about, because `plugin.remove` returns at once and then gives the
process eight seconds to leave on its own before signalling it. With a round
every five seconds that is one or two stopping plugins alive at any moment, by
design, and the slack covers them.

Anything that grows per round walks straight through a fixed slack, because a
hundred rounds of one leaked descriptor is a hundred descriptors, not eight.

## Reading a failure

**The stall bar.** Note the round and the elapsed time the summary names, then
look at the core's log from that moment (`--keep` leaves the working directory
and names it). A stall that happens at one particular round number is usually
the work of that round: the plugin install, or the mosaic coming up for the
MJPEG stream. A stall that starts partway through and never goes away is the
interesting one, because something is now in the way of every frame.

**Memory.** Run it again with `--minutes 60` and look at the `samples` array in
the JSON. A leak is a straight line. A core that settles at a new level and
stays there for half an hour is a pool that grew once, which is the sort of
thing to explain in a comment rather than fix.

**Descriptors.** Divide the growth by the number of rounds. If it is close to a
whole number, that is how many a round leaks, and the round has four parts to
try removing one at a time. On Linux, `ls -l /proc/<pid>/fd` on the running
core names what is open, which usually says which part it is.

**Threads.** Same arithmetic. A thread per round is almost always a task
spawned per call that nobody joins.

## The phase table, which is not a bar

Under the four bars the summary prints what each part of a round cost:

```
Phase            first    median     worst      last   (seconds)
----------------------------------------------------------------
sources           0.05      0.58     25.09      0.58
take              0.03      0.03     10.05      0.04
streams           0.33      0.59     20.10      1.39
plugin            1.05      1.05      1.07      1.06
```

Nothing here fails the run. It is often the first thing to read anyway, because
a phase whose last is far above its first is a call that has started taking
longer than it used to, which has the same shape as a leak and usually the same
cause. The table above is a real run: the sources phase went from 50
milliseconds to 25 seconds while the other three stayed where they were, which
named the call to go and look at.

Every call in a round has a budget of ten seconds. One that goes over it is
abandoned and named under "calls that did not answer" at the end, so the round
after it still starts on time and is still comparable to the first.

The soak also stops adding sources when more than three of its own are still on
the mixer, and says so. Adding one every five seconds while removals do not
keep up turns a slow mixer into an overloaded one, and then every other number
in the run is measuring the overload rather than the mixer.

## What it writes

`bench/results/soak-<machine>-<date>.json`, alongside what `gmx bench` writes.
It carries the machine, the commit, the bars it was judged against, the warm up
and last samples, and every sample it took, so two runs on the same machine can
be compared line for line.

```
python3 -c 'import json;d=json.load(open("bench/results/soak-m4pro-2026-09-15.json"));
print([s["rss_kb"] for s in d["samples"]])'
```

## In CI

`.github/workflows/nightly.yml` runs it at sixty minutes and uploads
`bench/results/`, next to the eval suite and the footprint bench. An hour is
long enough for a per round leak to be obvious and short enough that the
nightly still finishes.
