# Benchmark GodwinMix at headend scale

A headend takes a couple of hundred channels off multicast and sends each one
on. The wave 4 contract asks for four numbers about that load: what 200 direct
shows cost the station and the direct host, whether any packet or GOP went
missing, how many 720p transcodes the governor admits, and how long an output
goes quiet when a show's compositing is switched on and off.
`dev/bench/scale.sh` produces them in one command and writes a report under
`dev/bench/results/`.

```sh
dev/bench/scale.sh                                    # 200 feeds, a show for each, 60 s measured
dev/bench/scale.sh --toggle                           # and switch compositing on and off for one show
dev/bench/scale.sh --format youtube-720p30 --feeds 60 # transcodes: as many as the governor admits
dev/bench/scale.sh --mode feeds                       # the feed generator and the checker alone
dev/bench/scale.sh --mode legacy --legacy-shows 8     # today's shape: a compositing show per feed
```

It needs cargo, ffmpeg (once, to make the clips), curl and tmux. Python is not
used anywhere in it. Every program it starts runs in a tmux session named
`gmx-scale-station`, `gmx-scale-feeds` or `gmx-scale-check`, so
`tmux kill-session -t gmx-scale-station` stops a run you want to abandon, and
every wait in the script gives up with a message instead of hanging.

## What a run does

1. Builds the station, the udp plugin and the harness in release. `--no-build`
   skips this.
2. Makes four ten second clips in `dev/bench/media/` the first time, with
   ffmpeg: 1080p30 at 8 Mbit/s with AAC, 720p30 at 4 Mbit/s with MP2, SD at
   2 Mbit/s with AAC, and a 6.5 Mbit/s multiplex carrying two programs (720p
   with AAC, SD with MP2). Each is constant bitrate with a keyframe every
   second.
3. Part one. Sends `--feeds` feeds for 20 seconds with nothing else running
   and checks every one straight off the generator. This is what was offered
   and what the generator cost, before the station is in the picture.
4. Starts a release station from a copy of `godwinmix.example.toml`, with the
   bind changed to `127.0.0.1:18480` (`--port`) and the example's sources and
   outputs left out, and a plugins folder of its own. The default `live`
   channel is removed so port 1935 is free for anything else on the machine,
   and the udp plugin is installed from `plugins/udp`.
5. Part two. Starts the feeds again, adds one show per feed from the feeds
   list, waits 15 seconds, then measures for `--seconds`: CPU and memory of
   the station and of every process under it once a second, `show.stats` once
   a second, and every output checked by a receiver.
6. Stops the station with SIGTERM, counts any show or plugin still running 15
   seconds later, and writes the report.

`--mode auto`, the default, uses `show.add_many` when the station lists it in
`core.api`. On a station without it, auto falls back to `--mode legacy`. With
`--mode direct` on a station that has `show.add` but no `show.add_many`, each
row goes through `show.add` with the same body; if the station makes a
compositing show and ignores the input, the add stops and says to run with
`--legacy`.

## The feeds

The generator holds each clip in memory and sends feed `i` the clip `i` mod 4,
seven TS packets to a datagram, each datagram at the time its PCR says. On
every pass round the loop it carries the continuity counters on and moves PCR,
PTS and DTS forward by the length of the loop, so a receiver sees one stream
that never restarts: no continuity error, no PCR jump and no timestamp going
back. Each feed starts at a different place in its clip, so 200 keyframes do
not land in the same millisecond.

Feeds go to `udp://127.0.0.1:20000` and the 199 ports after it by default.
`--transport multicast` sends them to `239.77.0.1:5000` and the 199 groups
after it instead, leaving by the loopback interface, so nothing reaches the
network.

Part one writes the feeds list the bulk add reads, one line per feed:

```
name,input,program,output,format
feed-001,udp://@:20000,1,udp://127.0.0.1:30000,copy
feed-004,udp://@:20003,1,udp://127.0.0.1:30003,copy
feed-008,udp://@:20007,2,udp://127.0.0.1:30007,copy
```

For the two program clip the program alternates between 1 and 2 from one use
of the clip to the next, so both ways through the program filter are used.
Each show sends its one output to `udp://127.0.0.1:30000` and up, and the
format column is `copy` unless `--format` names a rendition preset.

## Reading the report

The top of the page says the machine, the version, the commit, the load
average before the run (other work on the machine moves every number), and
how many processes were still running 15 seconds after the station was
stopped, which should be 0.

| Row | Where it comes from | What good looks like |
|---|---|---|
| Shows added | `show.add_many` (or the fallback) and how long it took. The dry run's `plan` follows when there was one | every row added, the refused ones listed with their reason at the end of the page |
| Station, Direct host, Show processes, Plugin rows | the station's process and everything under it, sampled once a second; percent of one core | the direct host should carry 200 copies for a small fraction of what 200 show processes would |
| Feeds generator | the generator's own CPU and what it sent | the offered rate, no send errors |
| Feeds as sent | the checker on the inputs in part one | no CC errors, no PCR jumps, no GOPs dropped |
| Outputs received | the checker on the outputs in part two | the same, and no stream silent for a second |
| `show.stats`, once a second | how long one read for every show took, the health states and alarms it reported, what the station counted on the inputs | a read well under a second, alarms only where loss was injected |
| Compositing on and off | the longest gap on the first show's output while `--toggle` switched it | a gap short enough not to drop a GOP; the contract asks for the number, not a bar |

The checker counts per stream: datagrams, TS packets, continuity errors and the
packets they lost, PCR jumps (more than 100 ms between PCRs), the longest PCR
spacing, PCR jitter (the spread of arrival time against the stream's own
clock), keyframes, the typical GOP, GOPs dropped, and the longest silence. A
GOP counts as dropped when two keyframes are one and a half GOPs or more apart
by their PTS, or when the stream had fewer keyframes than its time allows,
which is how a stream that runs slow shows itself. A stream that stops part
way through has the rest of the window counted as silence, so an output that
dies cannot look healthy.

The worst ten streams are listed at the end, by GOPs dropped, then continuity
errors, then silence.

## The tools on their own

The harness is one Rust binary, `gmx-scale`, in `tools/scale`. It is a cargo
workspace of its own, so the main build never compiles it; its only
dependencies are `libc` and `serde_json`.

```sh
cargo build --release --manifest-path tools/scale/Cargo.toml
cargo test --release --manifest-path tools/scale/Cargo.toml
T=tools/scale/target/release/gmx-scale
```

Send feeds, with loss and jitter on some of them, and write the list:

```sh
$T feeds --clip dev/bench/media/hd1080.ts --clip dev/bench/media/sd.ts \
    --count 50 --to udp://239.1.1.1:5000 --seconds 300 \
    --impair 0-4:loss=1 --impair 5:jitter=20 --csv feeds.csv
```

`--loss` and `--jitter-ms` apply to every feed; `--impair` to a range of
feeds by index. A dropped datagram is dropped after its continuity counters
were written, so a receiver counts it as loss, as it would off a congested
switch. Ctrl+C stops the feeds and still prints the summary.

Check what arrives, on 50 groups or 50 ports:

```sh
$T check --from udp://239.1.1.1:5000 --count 50 --seconds 60
$T check --from udp://127.0.0.1:30000 --count 200 --seconds 60 --quiet --json out.json
```

Add shows from a list, sample a running station, call one method, and turn a
run folder into a page:

```sh
$T add --csv feeds.csv --station 127.0.0.1:8080 --json add.json
$T sample --pid 4242 --station 127.0.0.1:8080 --seconds 60 --csv samples.csv   # 4242: the station you started
$T call --station 127.0.0.1:8080 --method show.stats
$T report --dir /path/to/run --out report.md
```

`gmx-scale <command> --help` lists every option of each.

## Multicast on one Mac

Over loopback on macOS, multicast does not keep up at this rate. Measured on
an M4 Pro with 200 feeds (1.025 Gbit/s, about 97,000 datagrams a second), the
generator spent 1.3 to 1.5 cores and the checker received about two thirds of
what was sent, with continuity errors on every stream. The kernel dropped the
rest before any socket saw it: `netstat -s -p udp` counted fewer datagrams
received than were sent, and none dropped for full socket buffers. 100 feeds
arrived whole but up to 120 ms late. The same 200 feeds over loopback unicast
arrived whole with the generator at 0.6 to 0.7 of one core.

So the script uses unicast by default. On Linux, and on any machine where the
feeds come in on a real network interface, `--transport multicast` measures
what a headend would see.

## What it does not do

* Opening the monitoring wall is not scripted. To measure it, open the wall in
  a browser while the script says it is measuring; the station's CPU includes
  whatever the wall asks for.
* Thumbnails and alarms run at whatever the station's defaults are. The
  contract has alarms on by default for direct shows.
* The script runs on macOS and Linux. On Windows the tools build, but the
  script needs a POSIX shell and tmux.

## Results so far

Every run's page is in `dev/bench/results/`, named
`scale-<cpu>-<date>-<mode>.md`. The first ones were taken on the commit before
direct shows existed, so they are the baseline the wave 4 numbers will be
compared with. On an M4 Pro, with other builds running on the same machine:

* 200 feeds over unicast, 1.025 Gbit/s: the generator used 0.67 of one core
  and 28 MiB, and all 200 arrived with no continuity error, no PCR jump and no
  GOP dropped.
* 200 feeds over loopback multicast: about a third lost in the kernel, as
  described above.
* Eight compositing shows, one feed each, copied out over UDP: about 0.3 of
  one core and 370 MiB per show, plus two udp plugin processes at 0.9% and
  12 MiB each. With the machine that busy, every one of the eight outputs
  stopped part way through the recorded run, for 13 to 33 seconds, and two of
  eight in a second run: the show's output queue filled, the forced reconnect
  never finished, and the output still said `live`. A show process per feed
  would need about 72 GiB for 200 feeds, which is why wave 4 puts them in one
  direct host.
