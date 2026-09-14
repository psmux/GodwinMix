# Sources

Every source runs in its own GStreamer pipeline, so one dying cannot disturb
the programme. This page is the reference for what a source can be and what
the mixer does when one stops delivering. Web pages have their own page:
[web page sources](web-page-sources.md).


Add and remove sources from the UI, or over the API. The protocol is worked out
from the URL, so there is nothing to configure beyond the address:

| URL | opened as |
|---|---|
| `rtmp://host/live/key`, `rtmps://…` | RTMP, demuxed explicitly so the client can be chosen |
| `https://host/stream.m3u8` | HLS |
| `https://host/manifest.mpd` | DASH |
| `rtsp://…`, `srt://…`, `udp://…`, `rtp://…` | continuous stream |
| `web+https://host/page`, `web://host/page` | the page rendered by a real Chromium, with its audio, see the browser sidecar |
| `exec:<command line>` | whatever the process writes to stdout, including `tools/browser-source.sh` |
| a path, `file://…`, `https://host/clip.mp4` | file |

The distinction that matters is continuous versus finite rather than the
protocol. A continuous source is re-timed onto programme time and restarted when
it drops; a finite one is expected to end.

Sources added or removed from the UI are written to `<config>.runtime.toml`
beside the config file. Once that file exists it is the authoritative list:
merging it with the config's own `[[sources]]` would mean a source deleted in the
UI reappearing at the next restart. Delete the file to go back to the config.

### When a source stops delivering

The supervisor watches every source's own output. A source that has produced
nothing for `stall_timeout_secs` reads as stalled; one that stays stalled for
`stall.restart_after_secs` (10 by default) has its pipeline restarted, and a
superimposed source, which cannot be restarted in place, is built again from
nothing: page probe, clip fetch, browser start, about ten seconds.

Two things about that, both learned on air on 2026-09-11 and 2026-09-12, when a
superimposed source started coming up dead and was rebuilt 485 times one night
and 1174 the next.

**The programme keeps its picture.** A rebuild removes the source, and removing
the source that is on programme used to take the programme to None, so the
output sat on the slate for as long as the rebuild took. The branch is now left
in the programme pipeline with its compositor pad still at alpha 1, under the
programme layer and over the slate: a compositor keeps drawing a pad's last
buffer for as long as the pad is there, so that is a freeze frame with no
element added and no picture copied. Measured on a Mac with the mosaic's
programme cell: before the kill the picture read mean luma 39 and changed every
frame; through the whole rebuild it read exactly 22.35, byte for byte the same
JPEG each time; when the hold ran out it dropped to 16, which is video black.
The hold is released as soon as the replacement is taken, or after 45 seconds,
after which the programme does go to the slate and an alert says so.

**The loop ends.** Rebuilding is free for the first `stall.rebuild_attempts`
consecutive failures (3) and then waits `stall.rebuild_backoff_secs` (30),
doubling to `stall.rebuild_backoff_max_secs` (300), with an alert on the UI for
each wait. Two hours of that is under 40 attempts rather than 600. The count is
cleared the moment the source delivers a frame, so a source that recovers is
back on the fast path at once, and it is cleared when the source is removed,
because the ids are reused: a director alternating `event-a` and `event-b` one
per match must not have one match's failures charged to the next.

Every rebuild used to leak. The browser sidecar names its private profile
directory after its own pid and removes it when its message loop ends, which a
killed sidecar never reaches: 1084 directories and 18 GB of a container's /tmp.
The mixer removes it now, wherever it kills a sidecar. Two descriptors a build
went the same way: the read end of the child's stdout, handed to `fdsrc` with
`into_raw_fd` and never closed again, and the read end of its stderr, held by a
reader thread that never saw an end of file because Chromium's helper processes
inherit the write end and outlive the kill. And when the mixer is PID 1 in its
container it inherits every orphan on the box, which was 19,138 zombies after
two hours; it reaps them itself now, so the container is correct with or
without an init.

When a source is judged stalled, again just before it is rebuilt, and once when
its first picture arrives, the mixer writes down where that source's last
buffers sat on the programme's timeline: their running time, the programme's
own, the difference, and the fill of the programme-side queues `pgm-vq-<id>`
and `pgm-aq-<id>`. A probe on each proxy sink keeps the last running time in an
atomic, so nothing is logged per buffer. `video_behind_ms` is the number to
read: positive means the buffer was behind the programme, which is ordinary,
and negative means it was in the programme's future, which is the fault, since
a compositor holds what it is not ready to consume, the pad queue then fills and
the push into it never returns. Measured here on a Mac, a healthy build and a
blocked one side by side:

```
why="first picture"    video_behind_ms=70     vq_buffers=0   vq_time_ms=0
why="about to rebuild" video_behind_ms=-2427  vq_buffers=30  vq_time_ms=1000  aq_buffers=100
```

A source that has merely stopped producing looks different again: behind by
seconds and with nothing queued at all, which is what a suspended browser gives.

Measured on a Mac over 30 add and remove cycles of a superimposed web page:
open descriptors, pipes, regular files, cached clips and profile directories
all flat, with memory steady. Two caveats found while measuring, both macOS
only. VideoToolbox's decoder leaks a pipe pair per instance, which shows as two
descriptors a build for any source it decodes, a plain mp4 file included, and
goes away with `hardware.decode = "software"`. And every input pipeline used to
make its own `GstGLDisplay` and never free it, 31 `gldisplay-event` threads
after 34 cycles; the bus watch now answers `need-context` with the first
display any pipeline made, which is what GStreamer says an application hosting
several pipelines should do.

### Anything as a source

`exec:` makes a source out of a command line. The process writes a container to
stdout, MPEG-TS being the usual choice, and the mixer demuxes and decodes it
through the same hardware-aware path as everything else. That means an `exec:`
source is GPU accelerated on a machine with a GPU and falls back to software on
one without, with no change to the command.

```sh
godwinmix ctl source add gen \
  "exec:ffmpeg -re -f lavfi -i testsrc2=size=1280x720:rate=30 \
   -f lavfi -i sine=frequency=440 -c:v libx264 -preset veryfast -tune zerolatency \
   -c:a aac -ar 48000 -ac 2 -f mpegts -"
```

This is the escape hatch for everything GStreamer does not do well. ffmpeg
filters, a capture tool with no GStreamer element, a Python script, a
purpose-built browser binary: if it can write to a pipe, it is a source. A
command that exits is restarted, so a finite input loops.

**It is off by default and must be enabled deliberately:**

```toml
[security]
allow_exec_sources = true
```

An `exec:` source is arbitrary code execution for anyone who can reach the
control port, which is a far bigger grant than "can switch cameras". Only turn
it on when that port is on a network you trust.

Two things this got wrong during development, both worth knowing if you write a
similar bridge. Do not set `do-timestamp` on `fdsrc`: the process is writing a
container, and stamping buffers with their arrival time before the demuxer sees
them destroys the timing the container carries, which showed up as a source that
ran for a few seconds and then stalled. And kill the child on stop and restart,
or a rebuilt pipeline leaves an orphan writing into a pipe nobody reads.

### Browser capture

A real Chromium renders the page on a virtual display while its audio plays into
a null sink that gets recorded alongside. Everything Chrome can do works,
including DRM players, WebGL and WebAudio, which is the reason for using a whole
browser rather than a lighter renderer.

```sh
godwinmix ctl source add site \
  "exec:/opt/godwinmix/tools/browser-source.sh https://example.com/page 1280 720 30"
```

Needs `xvfb`, `chromium`, `pulseaudio` and `ffmpeg`, and `security.allow_exec_sources`.
Linux only, which is where production runs; workstations drive it remotely.

It costs one encode in the script and one decode in the mixer. That round trip
is the price of the process boundary, and it buys the thing that matters here:
a browser crash cannot reach the encoder, exactly like a dead camera cannot.

**GPU is optional and needs no change.** Chromium falls back to software
rendering by itself, and decoding in the mixer goes through the usual
hardware-aware path. Where a GPU is present, pass it through:

```sh
CHROME_FLAGS="--enable-gpu --use-gl=egl" VIDEO_BITRATE=8000k browser-source.sh ...
```

Tuning knobs, all environment variables: `CHROME_BIN`, `CHROME_FLAGS`,
`VIDEO_BITRATE`, `X264_PRESET`, `BROWSER_SOURCE_SETTLE` (seconds to let the page
lay out before the first frame), `BROWSER_SOURCE_DISPLAY`.

Verified in a Debian container with no GPU: valid MPEG-TS on stdout, H.264
1280x720 with AAC 48 kHz, 413 video frames, and an audio spectral centroid of
673 Hz against the test page's 660 Hz WebAudio tone, so the sound is genuinely
the page's rather than silence. Signalling the process group leaves zero
chromium and zero Xvfb processes behind.

Two bugs found getting there, both worth knowing if you write a similar script.
Do not `exec` the final ffmpeg: it replaces the shell and discards the cleanup
trap, so the browser and X server outlive the source and every add or remove
leaks a Chromium. And `Child::kill` sends SIGKILL to one process, which a script
cannot trap and which orphans its children, so the mixer starts exec sources in
their own process group and signals the group.

