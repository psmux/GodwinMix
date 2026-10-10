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
| `rtsp://…`, `rtspt://…`, `srt://…`, `rist://…`, `udp://…`, `rtp://…` | continuous stream; RTSP is below |
| `web+https://host/page`, `web://host/page` | the page rendered by a real Chromium, with its audio, see the browser sidecar |
| `exec:<command line>` | whatever the process writes to stdout, including `tools/browser-source.sh` |
| a path or URL ending `.png`, `.jpg`, `.jpeg`, `.bmp`, `.webp`, `.tif`, `.svg` | a still picture, held on screen (`image/source`); with transparency it is drawn over the scene |
| `text:Some words` | words rendered by the mixer, no browser (`text/source`); see [text sources](text-sources.md) |
| `ticker:Some words` | words crawling across a bar, or credits rolling up (`ticker/source`) |
| a numbered pattern, `/slides/f%04d.png` | the pictures played in order at `params.fps` (25), looping (`image/source`) |
| a path, `file://…`, `https://host/clip.mp4` | a clip (`file/source`), which does what `params.at_end` says at its end; see below |

The distinction that matters is continuous versus finite rather than the
protocol. A continuous source is re-timed onto programme time and restarted when
it drops; a finite one is expected to end.

Sources added or removed from the UI are written to `<config>.runtime.toml`
beside the config file. Once that file exists it is the authoritative list:
merging it with the config's own `[[sources]]` would mean a source deleted in the
UI reappearing at the next restart. Delete the file to go back to the config.

### When a source does not start

A source that cannot be built when the show starts (a camera whose plugin did
not answer `start` within five seconds, a file not there yet) stays in the
runtime list and in `source.missing` with its error, and is tried again by
itself. The first three tries follow the restart delay, half a second growing
towards ten; after `stall.rebuild_attempts` (3) failures the wait is
`stall.rebuild_backoff_secs` (30), doubling to `stall.rebuild_backoff_max_secs`
(300). It reads `failed` in `event/source.state` between tries, and the log
says `the source did not start; trying it again later` with the count and the
wait. A source waiting on a piece being set up, a plugin or the browser
renderer, is not on this clock: it starts when the piece is ready. Removing
the source stops the tries.

### When a clip reaches its end

A clip (`file/source`) does one of three things when its last frame has gone
out to the programme, set per clip with `params.at_end`:

| `at_end` | What happens |
|---|---|
| `hold` (the default) | The last frame stays up until the clip is scrubbed or restarted. |
| `repeat` | The clip plays again from the start: a seek, the same one the scrubber makes, with no reconnect and no gap. |
| `leave` | The last frame is held, and if the clip is on air the programme moves off it (below). |

Set it when adding the clip or on a clip that is running. Changing it does not
restart the clip, and a clip held at its end that is set to `repeat` starts
again at once:

```sh
gmx tool set_source '{"id": "intro", "params": {"at_end": "repeat"}}'
```

The tile in the Sources panel has a **Repeat** toggle; with Repeat off, the
clip's settings drawer chooses between holding the last frame and leaving the
scene. A clip from the Graphics gallery is added with `repeat`, since those
are made to go round; one imported from OBS with Loop ticked is too. In 0.2.2
and before every clip went round, by a restart (below), and a clip in a
runtime file from then that says nothing now holds. `loop = true`, which the OBS
import used to write, still reads as `repeat`.

A held clip sends no more frames, so a picture made after it came to rest (a
mosaic built when a page opens, the Studio preview of its scene, a take that
draws it in a new place) would have nothing of it to draw. Each of those has
the clip seek to its last frame once and send it again. That costs one decode
from the keyframe before it, and it is not a new end: it says nothing on the
event bus and a clip set to leave does not leave again.

Whatever it does, the clip reads `live` the whole time and never `connecting`
or `stalled`. Its row in `source.list` and in the status carries `at_end`, and
`ended: true` while it is holding its last frame. Each end is said once in
`event/source.ended`, `{"source": "intro", "at_end": "hold"}`.

**Leaving the scene.** A clip set to `leave` that ends while it is on air,
alone or in the scene on air, has the programme taken to:

1. the scene armed in Preview, in Studio mode, unless it is the scene on air
   or also shows the clip;
2. otherwise what was on air before the clip's scene, a scene or a source on
   its own, as long as it does not show the clip either.

The take is a `program.take` made by the control plane when it hears
`event/source.ended`, with that scene's own transition, under the same safety
rules as any take, and credited in the history to whoever made the last take.
Nothing in the media path waits for it. If there is nowhere to go, or the take
is refused, the clip holds its last frame on air and an alert says why. The
programme is never cut to black for a clip. A clip that ends off air only
holds.

**How the end is found.** The clip's own pipeline reads the file to its end
while the last second or so of it is still in its programme queues, so the
end is taken from the far side of those queues, when the last frame has gone
to the compositor. In 0.2.2 and before, the end of the file was treated like a source
that had stopped: the pipeline was restarted, the source read `connecting`
for half a second, the tail still queued was thrown away (933 ms of picture
and a second of sound, measured), and an eight second clip went round every
5.7 seconds. A clip over HTTP from a server that refuses range requests
cannot be seeked, so `repeat` on one still restarts it, after its tail has
played. A clip with transparency is drawn by the overlay board without the
timeline aligner the seek needs, and repeats the same way.

### RTSP cameras

Every scheme `rtspsrc` knows is a continuous stream (`hls/source`):

| Address | Transport |
|---|---|
| `rtsp://host/path` | UDP first; TCP when nothing arrives over UDP within five seconds |
| `rtspt://host/path` | TCP only, interleaved on the RTSP connection |
| `rtspu://host/path` | UDP only |
| `rtsph://host/path` | RTSP tunnelled over HTTP |
| `rtsps://`, `rtspst://`, `rtspsu://`, `rtspsh://` | the same four over TLS |

Two params, read only for an RTSP address:

| Param | Default | What it does |
|---|---|---|
| `transport` | `auto` | `auto`, `tcp` or `udp`, for an `rtsp://` or `rtsps://` address. A scheme that names its transport (`rtspt://`) wins over it. Choose `tcp` for a camera across a firewall, a VPN or NAT |
| `latency_ms` | `200` | The jitter buffer, 0 to 10000. Raise it for a camera on a jittery network |

Anything else is refused when the source is added, with the choices in the
message.

The mixer sets `rtspsrc` up for a camera that has to come back by itself.
`tcp-timeout` and `timeout` are five seconds, so a connection attempt to a
camera that does not answer fails in five seconds rather than twenty, and the
restart below tries again. `teardown-timeout` is 200 ms, so stopping a camera
behind a pulled cable does not wait for an answer to TEARDOWN that will never
come. Keep alives are on, and a frame later than the jitter buffer is dropped
rather than queued.

An RTSP camera's timeline is started at zero at its first frame, as an RTMP
camera's and a clip's are. `rtspsrc` stamps each packet with when it arrived on
the programme's clock, so without this a camera added to a programme that had
been up for minutes had every frame placed minutes in the future. In 0.3.1 an
`rtspt://` camera went live and was judged stalled about six seconds later,
every time: the address also fell through to the clip kind. Both are fixed in
0.3.2. The log says `an RTSP stream's timeline was started at zero` once per
connection.

A pulled cable on TCP looks like a camera that has gone quiet: nothing arrives
and, for a while, no error comes. Measured on loopback through a relay that
stops forwarding without closing anything, against MediaMTX 1.9.3:

* A cable out for 20 seconds: the connection survives, and the picture comes
  back on it when the cable goes back in, with no restart.
* A cable out for three minutes: the stream ended about 30 seconds in and the
  pipeline was restarted, in half a second, without waiting on the camera.
  Each try after that failed in five seconds and the next came at most ten
  seconds later, by the restart delay below. The camera was live again about
  six seconds after the cable went back in.

### When a source stops delivering

The supervisor watches every source's own output. A source that has produced
nothing for `stall_timeout_secs` reads as stalled; one that stays stalled for
`stall.restart_after_secs` (10 by default) has its pipeline restarted, and a
superimposed source, which cannot be restarted in place, is built again from
nothing: page probe, clip fetch, browser start, about ten seconds.

For a network source (RTMP, HLS, RTSP, SRT, RIST, UDP) "produced" means a frame
that arrived from the network. The `livesync` element in its chain repeats the
last frame through any gap, and until 0.3.2 those repeats counted: an RTMP pull
through a relay whose cable was pulled for two minutes read `live` the whole
time and was never restarted. The frames going into `livesync` are now counted
as well, and the longer of the two waits is the one judged, so a pulled cable
reads `stalled` after `stall_timeout_secs` and is restarted on the schedule
below whatever the catch up guard (further down) has decided about it.

`livesync` paces against the programme's clock, which a source pipeline
shares, so the picture is moved onto that clock on its way in and moved back
by the same amount on its way out. The picture then leaves on the source's own
timeline, beside its sound, and both are placed on the programme once. Until
this was done a stream stamped from zero, which is every RTMP feed and every
RTSP camera, was handed to `livesync` as if it were minutes late. It repeated
frames until it had caught the clock, came out stamped on the clock, and the
aligner added the programme's running time to that a second time. The sound
was placed right and the picture as far ahead as the show was old. At a first
start that was under a second of lip sync. After a cable pull on 2026-10-10 the
restarted RTMP pull came back 35 seconds into the show, read live for three
seconds and was judged stalled again for fifteen, its queues full of frames due
half a minute later; an RTSP camera whose cable healed by itself had its
repeated frames placed 3.2 seconds early, and the catch up guard gave up on it.
The log says `moved this source's picture onto the clock for livesync and back
after it`, with `shift_ms`, once per connection.

### When a source never delivers

A source that pulls its feed from a server (an `rtmp://` or `rtmps://` address,
an HLS or DASH playlist, an RTSP session, an SRT caller) and has delivered
nothing `stall.connect_timeout_secs` (15) after it was started or restarted is
restarted, on the restart delay (half a second growing to ten), and again
after every further deadline, for as long as it takes. Nothing posts an error
for a server that accepts the connection and never answers, so before 0.3.2
such a source sat on `connecting` until somebody restarted it by hand. A source
that waits to be sent to is never held to this: an SRT listener (`srt://:9000`
or `mode=listener`), `udp://`, `rtp://`, `rist://`, and every plugin source,
such as an ingest source waiting for a phone or an encoder to publish. Zero
turns the deadline off.

The restart armed for this waits out its delay, and the attempt it was armed
against can connect in the meantime, which is what a pull hung in a dead relay
does when the cable goes back in. A source that has delivered and reads `live`
when that restart comes due is left alone, and the log says `the source came
live while its restart waited; not restarting it`.

### What the programme shows meanwhile

A source on programme that stops delivering keeps its last frame on air for up
to 45 seconds after it was last live, through the stall, the restart and the
reconnect, and the picture comes back on its own when frames do. A restart in
place sends a flush across to the programme, and that flush is stopped at the
branch's queue before the compositor, so the compositor's pad still has the
frame and the hold costs nothing. Past 45 seconds the programme shows the
slate under that item until the source is live again. A source that never
delivered a picture has nothing to hold and shows the slate from the start.
`stall.hold_last_frame = false` turns the hold off.

### When a restart does not finish

Restarts run on a thread of their own. One still running after 30 seconds (a
teardown parked in a queue nobody reads, a client connecting inside its state
change) is left to its thread, and the source is built again from nothing
beside it, with an alert. The old pipeline is never started again if its
thread does come back. The librtmp client (`rtmp_client = "librtmp"`, or the
fallback `auto` swaps to) is given a ten second `timeout` rather than its own
120, because it connects inside that state change. The swap to it now runs off
the mixer thread too, and only for a source that has never shown a picture: one
that worked and then lost its network keeps the client it worked with. On a
cable pull on 2026-10-10 the swap happened mid outage and every restart after
it waited out librtmp's whole timeout.

Measured on this laptop with mediamtx behind a relay whose cable was pulled, an
RTMP pull with the defaults read `stalled` two seconds after the pull, was
restarted ten seconds later and then about every fifteen seconds while the
cable stayed out, and read `live` again within a second of it going back in,
for a 20 second outage and for a three minute one. An RTSP camera on the same
relay read `live` again within three seconds. Both stayed live afterwards with
their programme queues empty; before the `livesync` change above, the RTMP
pull read live and then stalled again three seconds later, and both sources'
`pgm-vq-<id>` sat full at a second of frames due in the future.

### When a plugin says it is failing

A plugin source that declares the `health` capability is asked `health` every
two seconds, on a thread of its own, never while it is busy in another call.
`degraded` and `failing` each raise an alert once when they start. A plugin
that answers `failing` for `stall.restart_after_secs` is restarted, and that
counts as a strike in the same way a stall does (below).

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
cleared once the source has stayed live for a minute after its last restart,
and it is cleared when the source is removed, because the ids are reused: a
director alternating `event-a` and `event-b` one per match must not have one
match's failures charged to the next.

**A stall restart that does not hold makes the next one wait longer.** Every
restart for a stall is a strike against the source, and each strike doubles
how long it may stay stalled before the next one: `stall.restart_after_secs`,
then twice that, four times, eight, and sixteen times at most, so 10, 20, 40,
80 and 160 seconds with the defaults. While a source has strikes, the delay
before an in place restart (half a second, growing to ten) and the rebuild
count above are kept too. All of it is forgiven once the source has stayed
live for 60 seconds. A source that is really dead is still restarted, a few
times an hour rather than a few times a minute, and it reads `stalled` the
whole time. A restart for an end of stream or a pipeline error is not a
strike. A clip's end is not a restart at all; see below. Each strike is written to the log with
`strikes` and `next_stall_limit_secs`, and `the source has stayed live since
its last restart` when a source is forgiven.

**A starved machine is not a dead source.** On a tick where the programme
itself made fewer than half the frames the canvas rate asks for, a stalled
source's time counts a quarter towards that limit. The machine is short of
CPU, and restarting sources then adds work rather than removing it.

Both rules come from 2026-10-05, when the desktop app ran 16 hours without a
fault and then, with the machine loaded by other work, judged its sources
stalled 290 times in under two hours. The page in it came back for a second
or two after each rebuild, which cleared its backoff every time, and it was
rebuilt 43 times. Its camera and screen, sidecars on the `container`
transport, were read into an `appsrc` that never made the reader wait: with
the decode behind it short of the camera's frame rate, every frame the camera
wrote was queued in the mixer, its pictures reached the programme 47 minutes
late, and the mixer grew to about 12 GB and died on an allocation of one raw
1080p frame. That element now blocks at 16 MB, so the pipe fills and the
sidecar's own leaky queue drops what it cannot send.

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

These running times are the source's own, before the aligner's shift, so for
a source the aligner places (an `exec:` source, a container sidecar, a web
page) they say where the source's clock had got to and not how long its frames
wait. That second number is the lead, measured on every frame entering
`pgm-vq-<id>` and `pgm-aq-<id>`. When every frame of a live source has waited
more than 200 ms for two seconds, the mixer moves the source earlier by the
least lead it saw, less 40 ms, and logs it at info:

```
a live source was running behind its own frames; dropped to the newest
    source=cam behind_ms=1476 caught_up_ms=1436 total_ms=1436
```

`total_ms` is how far the source has been moved since its offset was decided;
a restart or a seek sets it back to zero. A source that fills its queues again
straight after a catch up is not live, and is left alone with a warning, `this
source filled its queues again straight after a catch up, so it is not live`.
A source that can be scrubbed is never moved. At debug level
(`godwinmix_core::mixer::catch_up`) each source's least lead is printed every
half second. See [How late the picture is](../explanation/how-late-the-picture-is.md).

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

