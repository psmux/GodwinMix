# Run on a Raspberry Pi

A Pi is the cheapest machine that can run a service, an assembly or a club
match, and it is the machine this project has to be good on. It is also tight
enough that the difference between "on" and "asked for" decides whether the
stream holds.

The Pi numbers in the footprint table are not measured yet. This page is the
configuration to start from, not a promise about frame rates. The measured
column for `pi4` and `pi5` fills in when the boards are in the CI lab; until
then, run `gmx bench --machine pi4` on your own board and read your own table.

## Which board

The Pi 4 is the easier target and the Pi 5 is the harder one, which surprises
people.

* **Pi 4, 2 GB or better.** Has an H.264 encoder in hardware (`v4l2h264enc`).
  1080p30 is within reach and 720p30 has room to spare.
* **Pi 5, 4 GB.** Faster CPU, HEVC decode in hardware, and **no H.264 encoder
  at all**. Everything is x264 in software, competing with the compositor for
  the same four cores.

If you are buying a board to stream H.264, buy the Pi 4.

## The configuration

Write this to `godwinmix.toml`. It turns off everything a Pi does not need and
sets the output to what a domestic uplink can actually carry.

```toml
[canvas]
# 720p30. Every source is scaled to this once, and the compositor works at this
# size, so it is the single biggest lever on CPU.
width = 1280
height = 720
fps = 30
sample_rate = 48000
channels = 2

[program]
# 2,500 kbit/s is the default preset and it is chosen for real uplinks.
# YouTube asks for 6,000 at 720p30; a great many connections cannot give it,
# and a stream that holds at 2,500 beats one that buffers at 6,000.
video_bitrate_kbps = 2500
audio_bitrate_kbps = 128
keyframe_interval_secs = 2

[hardware]
# On a Pi 4, "auto" finds v4l2h264enc. On a Pi 5 there is nothing to find and
# this falls back to x264 on its own. Set encode = "software" if you want the
# fallback to be the only possibility.
decode = "auto"
encode = "auto"

[multiview]
# The mosaic is a second compositor and a second encoder. On a Pi it is the
# first thing to go. With this false there is no mosaic pipeline and no
# thumbnail branch on any source, and the UI falls back to the icon gallery
# rather than live tiles.
enabled = false

[snapshot]
# Stills are cut out of the mosaic, so with multiview off there is nothing to
# cut. Leaving this false as well makes the refusal honest rather than a 404
# from a subsystem that is still running.
enabled = false
```

If you want the mosaic on a Pi anyway, leave `[multiview] enabled = true` and
make it small and slow rather than absent:

```toml
[multiview]
enabled = true
width = 640
height = 360
fps = 4
jpeg_quality = 50
linger_secs = 2
```

It still costs nothing while nobody is watching: the pipeline is built when the
first client subscribes and taken down two seconds after the last one leaves.
The two second linger is there so that reloading the page does not rebuild it.

## What you lose by turning them off

* No live mosaic in the web UI. The source gallery shows icons and labels, and
  taking a source to programme still works exactly as it did.
* No `/api/snapshot/*.jpg`. Those routes answer 404 with a message naming the
  switch that turned them off.
* `agent.state` still works: an agent still sees what is live, what is
  connected, what has gone idle and what the outputs are doing. It reports
  `motion: null` and no snapshot URLs, because both come from the mosaic.

Everything else is unchanged. The programme output never depended on any of it.

## Checking your own board

```
cargo build --release
./target/release/gmx bench --machine pi4 --budget
```

That prints the footprint table for your board, writes it to
`bench/results/pi4-<date>.md`, and exits non zero if a row is over the target
the plan commits to for a Pi 4. Expect it to take about five minutes: each row
is measured over thirty seconds of steady state after a five second warm up.

Two rows to look at first on a Pi:

* **Core idle CPU.** GodwinMix builds and plays the programme pipeline,
  encoder included, from boot. On a Pi 5 with no hardware encoder that means
  x264 is running on a black slate before you have added anything.
* **Two live sources with programme encode.** This is the shape of a real
  broadcast. If it is near a whole core on a Pi 4, or near two on a Pi 5, you
  have no headroom for a camera reconnecting.

## Other things that help

* Run headless. The mixer needs no display, and the web UI is served from the
  same port whether you drive it from the Pi or from a laptop on the same
  network.
* Prefer wired Ethernet. A dropout on the uplink is the most common cause of a
  broken stream, and it is not a CPU problem.
* Use a good power supply and a heatsink or fan. A throttled Pi drops frames,
  and the drop looks like a software fault.
* Keep the source count down. Every source is a decode and a scale before it
  reaches the compositor, and that cost is per source whether or not it is on
  air.
