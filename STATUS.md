# Where GodwinMix stands

## Graphics that move, 2026-10-06

An audit of what could carry motion and transparency, then the fixes.

**What there was.** Text, tickers, SVG templates, PNG, WebP and SVG stills
with alpha and clips with alpha (WebM VP8 or VP9 alpha, ProRes 4444) were
already drawn over the programme by the overlay board. A numbered picture
sequence plays but flat. Web pages and OGraf graphics were not transparent at
all: the sidecar could render a page with alpha, but only `layered/source`
used it, and every `browser/source` went through the compositor opaque.
`docs/reference/graphics.md` said so. And a page as a stream cost a camera
whatever it showed: on this laptop (Core Ultra 9 285H, 1080p30) a blank
transparent page took 125 percent of a core, an animated lower third 180,
a WebGL cube 213. WebGL itself works with no GPU: Chromium draws it with WARP
(the Microsoft Basic Render Driver) under `--disable-gpu`.

**What changed.** The renderer has a graphic mode: a frame leaves only when
Chromium painted, only the box around what is not transparent, in AYUV, with
patches for changes inside that box; a design that covers the picture is
sent whole as I420 and goes to the compositor like a camera. `html/graphic`
draws HTML templates (`html:<name>`) that way, with their fields and their
way in and out sent on stdin, `Capability::Cue` tells them when the programme
takes them, and the new `hold` exit keeps the item drawn while the page plays
its way out. `browser/source` takes `transparent: true`, and OGraf placements
use it. `template.check` reads a template and says what to fix. Fifteen HTML
starter designs and two SVG set foregrounds ship in the binary.
`docs/reference/graphics-for-agents.md` has the rules, the pack and the costs.

**What it costs now.** A held lower third: about 5 percent of a core in the
renderer. A crawl, a clock or a countdown: 10 to 30. The WebGL logo: about 50.
Full screen moving designs: 33 to 87 (the backgrounds at half size 33 to 52).
The mixer adds a few points for each. Allowing the GPU did not help on the
Intel Arc here (readback cost more than it saved).

**Not done.** Run on Windows only; macOS and Linux have the same code and
have not run it. A source's renderer runs while the source exists, on air or
not. `cue` follows the programme only, not the preview. There is no three.js
offline; the 3D designs are plain WebGL. Each renderer is a Chromium of its
own (about 6 processes), so many HTML graphics at once cost memory.

## Two phones that aborted the show, 2026-10-05

In an end to end test of the installed 0.2.1 app, two headless Chrome phones
published their fake cameras on `/join/`, both went live, both became sources,
and within seconds the show process aborted, three times in a few minutes
(0xc0000409 twice, and a 0xc0000374 that was the Quick Sync plugin's own heap
fault at start). Its stderr ended in hundreds of `gst_segment_to_running_time:
assertion 'segment->format == format' failed` and then `gstvideoaggregator.c:1898:
gst_video_aggregator_fill_queues: assertion failed: (start_running_time != -1
&& end_running_time != -1)`. The lines just before named the element:
`slot-q-0:sink Got data flow before segment event`, then the same for
`slot-crop-0`, `slot-flip-0` and `vmix:sink_1`.

**What it was.** Not the trimmed GStreamer and not the release build. It came
back on a debug build with the full GStreamer install the first time two
phones joined a show of eight test patterns with one on air, and as a unit test
with nothing but test patterns. The show had more sources than the compositor
has slots (eight). A source being added takes a slot as it is added, and with
the pool full it took the first slot not holding itself, which was the one on
air. The next apply put the source on air back on that slot. Each move flushes
the slot's chain, and the flush was sent into the slot's queue, under the
valve at its head. A flush takes the segment off every pad it passes, so the
queue lost it, but the valve's own pad was never told. The source coming back
brought the very same segment, the valve did not send a segment it believed it
had already sent, and its next frame reached the compositor with none. Before
the SRTP fix no phone ever sent a frame, so no source was ever added, and
nothing crashed.

**What changed.** The slot is flushed from the valve's own pad
(`gstutil::wake_below`), so the valve sends the segment again whatever comes
next. A source being added never takes a slot that is on the canvas. The valve
shuts before the flush: a frame meeting a flushing slot came back `FLUSHING`
through the source's one pad tee and paused the source's queue for good, which
left a test pattern black on air after a round of takes (this was there before
too, with the flush sent into the queue). And two guards, so a source can never
take the show down this way again: each slot's queue sends a segment down if a
frame arrives without one (`mixer::slot_guard`), and each source's proxy sink
holds back a segment that is not in time and any buffer with no segment or no
timestamp, posts an error from that source's pipeline, and the mixer restarts
that source alone (`input::boundary`).

**Measured.** `mixer::tests::full_pool` binds a source back to the slot it has
just left: it aborted three runs in three with the old flush and passes with
the new one. `mixer::tests::flush_window` counts a source's frames in the second
after a flush: 30 with the valve shut first, 0 with it open, by either route.
`mixer::tests::odd_segment` pushes a byte segment into a source's proxy sink
and the source is restarted while the programme keeps 30 frames a second. End
to end on a debug core bound to the LAN address, eight test patterns with one
on air and then two Chrome phones on `/join/`: the build before this aborted
the show as each phone joined (0xc0000409, the same lines as the app's log).
The build after kept both phones live for three minutes, twice: once on the
full GStreamer install (6527 programme frames in 216.6 s) and once on the
installed app's own trimmed bundle, read in place with a registry of its own
(6344 frames in 210.5 s). Each phone was taken to air at the start and at the
end and its picture was there. No `CRITICAL`, no show restart after the first
start, and none of the new guards' warnings in either log.

**Not done.** On a debug core with twenty sources, after rounds of takes had
moved phones between slots, a phone taken to air was sometimes black for some
seconds, and once a test pattern stayed black until it was restarted. The
second is what the valve now shuts for (`flush_window` shows the mechanism),
though the round of takes that showed it was not run again on the final build;
the first is not pinned down, since test patterns in the same rounds
were drawn and the same phones were drawn a minute later. The trimmed bundle
was not rebuilt; the installed app's bundle already has the FLV and Matroska
demuxers, `h264parse`, `aacparse`, the libav, OpenH264 and Opus decoders and
typefinding, so nothing was added to `dev/gst_trim.py`. Quick Sync still
corrupts the heap at about one show start in four on this laptop (0xc0000374),
which the station restarts; that is the driver, not this.

## The desktop mixer that grew to 12 GB, 2026-10-05

The installed app's mixer ran for 16 and a half hours without a fault. At
08:00 UTC the machine filled up with other work, and from then on its sources
were judged stalled 290 times: the webcam 176, the YouTube page 76, the
screen 31 and a still 7, 246 of them in the last hour. At 09:44 it died with
`memory allocation of 3110409 bytes failed`, at about 12 GB, and took the
browser sidecar with it.

**Where the memory went.** The camera and the screen are sidecars on the
`container` transport, and on Windows a thread of ours reads their stdout and
pushes it into an `appsrc`. The sidecar host built that `appsrc` with
GStreamer's defaults, and a default `appsrc` never makes the pusher wait: past
its `max-bytes` it says "enough data" and keeps everything it is given. With
the machine loaded, the demuxing and converting behind it fell short of the
camera's frame rate, so every frame the camera wrote stayed queued in the mixer. The camera's
pictures reached the programme later and later, 60 seconds behind at 08:38
and 47 minutes behind at 09:44, which is a queue of frames growing without
end. And 3,110,409 bytes is one Matroska block carrying one raw 1080p I420
frame (3,110,400 bytes and a 9 byte header), so the allocation that failed was
one more camera frame. The mixer the app started again at 09:52 showed the same
thing on a smaller scale an hour later: its screen 64 seconds behind and
staying there, a backlog the load had built and nothing drained, and the
process steady at about 1.5 GB with four sources. The `exec:` source path
already built this element to block at 16 MB; the sidecar host now uses the
same one (`input::pipe_source`).

Reproduced with a debug build and the shipped screen plugin forced onto
`videotestsrc`, a sidecar producing faster than the mixer consumes, which is
the same mismatch from the other side: the mixer reached 7.8 GB in ten
seconds and 11.5 GB in fifteen before the harness stopped it. With the fix the
same run sat between 325 and 343 MB for a minute. A test pushes raw 1080p
frames into the element with its downstream blocked: before, all 200 frames
went in (622 MB queued); now the writer waits after a handful.

**Why it kept restarting.** Two things in the restart policy let a loaded
machine feed itself. The backoff was cleared by the first frame after a
restart, so the page, which came back for a second or two after each rebuild,
was always on the fast path: rebuilt 43 times, roughly every 70 seconds. And
the stall limit was the same ten seconds on the fortieth restart as on the
first. Now every stall restart is a strike; each strike doubles
how long the source may stay stalled before the next (10, 20, 40, 80, then 160
seconds), and the strikes and the backoff are cleared only after a minute
live. An end of stream or a pipeline error is not a strike, so a clip that
loops still starts again half a second after it ends. A tick on which the
programme itself made under half its frames counts a quarter, since a starved
machine is not a dead source. In a test that
pauses a source three times, the restart came after 2.5, 2.4 and 2.5 seconds
before and after 2.5, 3.8 and 6.8 seconds now. A source that is really dead is
still restarted, a few times an hour, and reads `stalled` throughout.

**So the next one is in the log.** Every five minutes the mixer writes
`mixer memory` with its resident size and how many sources and outputs it
has, and warns when the size has risen at every sample for half an hour by
512 MB or more. It reads one number on the existing tick, on a blocking task,
and costs nothing between.

No per restart leak was found in the mixer's own code. Private bytes of a
1080p debug core, before and after: fifty in place restarts of a test pattern,
175.9 to 178.5 MB; of a still, 176.1 to 171.6 MB; of a clip decoded in
software, 814 to 818 MB; thirty restarts of a raw sidecar on the `container`
transport, 208.4 to 207.6 MB. Two tests now count the elements and pads in the
programme and in the source's pipeline, and what holds the source, across
twenty restarts in place and ten rebuilds, and assert they do not move.

One thing grows that is not ours: a clip decoded by Quick Sync
(`qsvh264dec`) adds about 4.4 MB a restart while it is being restarted, 815 to
1037 MB over fifty, and gave most of it back five seconds after the restarts
stopped (753 MB). The same clip in software is flat. A clip restarts at every
end of stream, so a short one looping on this hardware is worth watching with
the new memory line; none of the incident's sources decoded anything. The
incident's growth was the queue, not the restarts.

**The webcam that never came up.** After the restart the installed app's USB
webcam was missing from every show. Each boot logged `failed to add source`
with `the plugin did not answer start within 5 s`, then stopped the plugin
with `the source was removed`, and nothing asked again: the camera sat in the
saved config and in `source.missing` for good. Two things were wrong. The
camera plugin opened the device inside `start`, and on this laptop that is
slow: the device monitor's first look took about 4 s in a fresh process
(2.4 s of it Media Foundation's first probe), and a `gst-launch-1.0` run of
`mfvideosrc` to ten frames took 3.5 s, and 7.5 s once. And the core
kept a source that failed at boot as unstarted without ever trying it again.

The open now runs on a thread of its own, written once in capture-common and
used by the camera and the screen, so `start` answers at once and the picture
follows. `health` says the camera is opening, then counts frames. A failed
open is tried again by the plugin after 1 s, then 2, 4, up to 30, with the
reason in `health`. A second open waits for the first to let go of the
device, and a stop during an open hands the device back when the open
finishes. On Windows the camera is now asked for through Kernel Streaming
first. Measured on this webcam, a whole `gst-launch-1.0` run each time: to ten
frames, 1.1 to 1.3 s with `ksvideosrc` against 3.5 s with `mfvideosrc`; to 30
frames of 1080p, 1.8 to 2.1 s against 4.6 s. Listing the cameras through
Kernel Streaming alone took 40 ms. With another app holding the camera it
said `device already in use` after 0.4 s, where `mfvideosrc` took 6.9 s to say
`Internal data stream error`. A time limit on `mfvideosrc` would still have
spent the 2.4 s probe and the 3 s open on every good start, so the order was
changed instead. Media Foundation stays behind it, for a camera Kernel
Streaming cannot see and for a first frame that has not come within 3 s. A
saved `device` id is a Media Foundation path; it is matched to the Kernel
Streaming device by the instance path both share. Kernel Streaming lists this
camera's 1080p raw mode at 5 frames a second ahead of its Motion JPEG at 30,
so the caps now ask for at least the canvas rate when the device can do it.

In the core, a source that could not be started when the show was built is
tried again, half a second, then 0.9, 1.6, and after three failures the
rebuild backoff, 30 seconds doubling to 300. It reads `failed` between tries
and leaves the unstarted list when it starts. `crates/godwinmix-core/tests/late_start.rs`
starts a mixer with a Python plugin whose first `start` sleeps seven seconds:
the source fails at boot and is live 6.1 s after the mixer started, with
nobody asking.

Run on this laptop with a debug core from the branch and the camera plugin
staged beside it, with the webcam's saved Media Foundation id: the core
answered the camera's `start` 2.2 s after it began adding the source (process
start and handshake included) and the first picture was linked 1.1 s later,
through `ksvideosrc`. With a `gst-launch-1.0 ksvideosrc` holding the camera,
the source read `connecting` while it was held and went live 3.1 s after the
holder let go, with nobody touching it. One thing seen on the way: the log
lines a mixer-held plugin sends as `log` notifications do not reach the
core's log, so the camera's `would not start: another app is using it` is
read through `health` and not in `mixer.log`.

## Backgrounds, agents and releases, 2026-10-04

**A new background with or without a screen.** `matte/filter` cuts a person
out of the camera with no green or blue screen, by a matting model through
ONNX Runtime loaded at run time: DirectML on Windows, CoreML on macOS, CUDA or
OpenVINO where a runtime carries them, and the CPU on any machine, a Raspberry
Pi included. Two Apache 2.0 models ship. MediaPipe selfie segmentation took
5.6 ms a frame on this laptop's CPU and MODNet 29 ms on its GPU; `auto` picks
by what the machine has. The model runs on a thread of its own and the
frame's thread spends about 2 ms on a 1080p frame laying the newest mask over
it, so the programme never waits for the model. A cutout is drawn by the board
exactly as the chroma key is. Tested through a real pipeline on the CPU and on
DirectML. The presenter layout now takes `screen`: green, blue or none, and
New scene offers it. The installers and the server image carry the models
and the runtime; `dev/fetch-models.sh` fetches them pinned and checked.
Robust Video Matting, from StudioBackground, is finer still but GPL 3.0, so it
is not shipped; its recurrent form is not read yet.

Virtual set is no longer an option of its own. It was a layout behind a
button beside New scene; it is now a way to start a new scene, and
`scene.virtual_set` folded into `scene.create_from`.

**Agents.** Help > Connect an AI agent shows the lines for Claude Code,
opencode, pi, Codex, Gemini CLI and any MCP client, written with the mixer's
own path. `godwinmix mcp` and every `gmx` command find the desktop app's
mixer and its token by themselves. `gmx tool NAME JSON` runs any MCP tool
from a shell, for pi, which has no MCP, and `gmx ctl upload` puts an agent's
own pictures and templates in the library. `gmx skill install` writes for
opencode and pi too.

**Found and fixed on the way.** The installed `godwinmix.exe` could not start
outside the app at all, its GStreamer DLLs being nowhere Windows looks; they
are delay loaded now and the binary points itself at the bundled copy, and the
macOS release does the same with install names. Every media upload in the
desktop app failed with access denied, the relative media folder having been
resolved against the install folder. The release's macOS job deleted its own
`.app` before checking it, its Windows job ran the NDI SDK installer for two
hours until it timed out, and its Linux jobs lacked the RTSP server's
development package. The docs index had 29 pages missing.

The background plate stayed black behind the cutout in the installed app, for
three reasons stacked on each other. The trimmed GStreamer had lost
`imagefreeze`, `rsvgdec`, `multifilesrc` and several more that the code makes
by name, so the trimmer now reads the element names out of the source. A
library picture's Windows path, `\\?\C:\...`, was read as an address with a
query and became a clip that played once. And a still added while a show was
running was shifted onto the programme's timeline a second time: `imagefreeze`
already stamps with the programme's running time, so the picture sat as far
in the future as the show was old, its queues filled and it was restarted as
stalled. A picture present at startup worked only because both numbers were
near zero. A still now declares `programme-timeline`, as the test pattern does.

Still open: the station link test and the transition frame rate test fail on
a loaded run here and pass alone. The app's mixer exited once with code 1
right after an upgrade and has not done it again; there was nothing in its
log to say why.

## Windows, 2026-10-03

Built, run and installed on a Windows 11 laptop (Intel Arc 140T, driver
32.0.101.8860) with GStreamer 1.28.6. The mixer picks Quick Sync for encode
and decode by itself. `dev/smoke.ps1` passes every step it runs; the five
plugin steps skip because Windows refuses shell plugins.

The installers build from `release.yml`'s steps and stay well inside the
budget: the `.msi` was 59 MB and the NSIS `.exe` 39 MB before the renderer
and the plugins went in. The trimmed runtime was 117 MB. Each installs, passes `--headless-check` with the system
GStreamer taken off `PATH`, loads its four device plugins, installs over an
older copy as an upgrade, and uninstalls leaving nothing in Program Files.

What this found and fixed. The Quick Sync runtime on this GPU corrupts the heap
of the process that loads it in about one load in four, and calibration loaded
it inside the station, which then died with every show; calibration now runs
in a child the station can lose and retry, and a show loads its codec plugins
before anything is on air, so the same crash costs one restart at start up.
First party plugins could not build from a checkout on Windows. The process
sampler never read CPU there, so the governor counted a mixing show as free.
`bundle-plugins.sh` refused ingest on every platform. A busy UDP port was not
explained on Windows. `cargo test --workspace` did not compile on Windows, and
many tests wrote Windows paths into launch descriptions.

Later the same day. The installers now carry the web page renderer and every
first party plugin that runs on Windows. The four device plugins install on
first launch; the rest install themselves, in a moment and with nothing
downloaded, the first time somebody picks their feature. A YouTube page went
live in the installed app in about two seconds, with sound. The webcam's tile,
the scene preview and the programme all move; before the timeline fix the
programme showed one camera frame in fifteen. A camera is listed once, not
once per Windows capture API, and a second source of the same camera is
refused with the name of the source that has it.

Output plugins run on Windows. The core serves the programme to them on a
named pipe where Unix uses a FIFO. file-record wrote a seven minute H.264 and
AAC recording through it. WHIP, NDI and RTSP read the same pipe. Hardware
encoders sent nothing on an output started on demand, because the encode chain
was joined to the tee before it was running: 3 in 5 starts failed, and after
the fix 8 in 8 worked. Icecast no longer needs `shout2send`, which GStreamer
for Windows does not have; the plugin speaks the source protocol itself.

The NDI runtime ships in the Windows installer, beside the ndi plugin, on the
NDI SDK's terms. The installer's licence carries the NDI end user terms and
every NDI form links ndi.video with the trademark line. The GStreamer and
Chromium licence files travel with them now; before, neither did.

Each output was then driven from the installed app, against the LAN address
because of the loopback problem below. NDI out and back in as a source carried
picture and sound, with no gap over 70 ms in 30 seconds; before three fixes it
carried one or the other, and the receiver panicked on its bandwidth setting.
Icecast fed a stand in server 87 seconds of stereo 128 kbit/s MP3 that
decodes. RTSP served 1080p30 H.264 that `rtspsrc` played back, 261 frames.
UDP sent MPEG-TS that decoded to 186 frames in ten seconds.
Three things were wrong in the core and are fixed. Installing a plugin dropped
the output types of every plugin installed before it, so after adding NDI the
UDP, Icecast and RTSP outputs were refused as not in this build. A plugin the
installer shipped was labelled custom, unreviewed. And a source whose restart
failed was never restarted again: the screen capture, whose start took up to
ten seconds because Direct3D 11 takes five to load on this GPU, sat on
connecting until it was restarted by hand. It now answers `start` at once and
opens behind it.

An upgrade now carries the plugins with it. The app had replaced its own copies
of the four device plugins, but a plugin the mixer installed on first use, NDI
or Icecast say, stayed at the version it was installed from. The app now
compares each copy that came from its own folder with what it carries and
replaces the ones that differ; a plugin installed from anywhere else is left
alone. Checked by installing over the previous build: all five came up to the
new one at launch, every output type was registered, and the webcam, the
screen and the YouTube page came back live.

Still open on that machine. A VPN client there drops loopback UDP and resets
loopback TCP once a client starts streaming, so the SRT, RIST, RTP, ONVIF and
two HTTP hook tests cannot pass on it, and the Icecast output test passes only
when pointed at the LAN address. Test binaries that load Quick Sync sometimes
end with `0xc0000374` after their tests pass. The UI's browser test harness
did not finish under Edge on Windows. NDI was not received from a real sender,
only from the mixer's own. The screen source drifts behind the programme over
long idle stretches, minutes in half an hour, until it is judged stalled and
restarted; the restart now comes back, but the cause is not found. The
transition frame rate test fails on a loaded run and passes alone.

## Current implementation, 2026-09-20

The broadcast workspace now has draggable split panes, grouped tabs, keyboard
layout controls, saved workspaces and JSON import and export. Narrow screens
stack the panes without replacing the saved desktop arrangement. Trusted panels
can preserve local controls while suspending preview work through the documented
`setWorkspaceActive` hook. Moving or resizing a pane keeps its instance alive.
Stream resolution changes wait 150 ms after resizing settles; geometry moves
immediately.

Sources are selected from each scene through a large + tile. Its chooser can
reuse existing sources, preview one on demand, or create a new source. Cameras,
screens, microphones and media are direct categories. Browse files opens the
browser file selector, uploads video, audio or images with progress, and inserts
each source into the chosen scene. The first upload creates the media directory;
concurrent uploads cannot overwrite existing files. There is no All sources tab. Removing a source from a scene keeps the mixer source for
reuse. Scenes and Sources share a client model independent of dock visibility.

The studio surface has Preview and Programme, Cut and timed Fade controls, a
dockable audio desk, destination status and local MP4 or Matroska recording.
Recording uses the existing encoder through the public output contract. A new
file is reserved on each start, and normal shutdown waits for file finalization.
Real media tests cover decoding completed files and adding or removing a
recording while the mixer continues running.

The source restart path now restores programme frames after recovery. Retired
compositor slots no longer impose the reproducible roughly 50 ms stall. Preview
queues are bounded by frame count; ten minute stress runs have stable resident
memory, thread counts and descriptor counts. A second timestamp offset that
stalled later test sources is fixed, as is a preview backdrop teardown error
seen when resizing. The integrated three minute run completed 36 rounds with
0.05 percent RSS growth, flat thread and descriptor counts, and a slowest
preview subscription of 317 ms. The strict timing gate still fails at
34.028375 ms against 34 ms. One negotiation warning remains. Full one hour and
cross platform endurance acceptance are outstanding; the exact evidence is in
[the reliability record](bench/results/reliability-2026-09-19.md).

This is a stronger foundation for an OBS alternative, but it is not yet a claim
of production parity with OBS, Wirecast or vMix. Native installer and endurance
validation must finish before a release is recommended for unattended shows.
Trimmed runtime checks pass at 84.5 MiB on macOS, 127.1 MiB on Linux and
111.7 MiB on Windows. These are runtime measurements, not installer sizes;
[the footprint record](docs/explanation/footprint.md) names the tested versions.
The desktop bundles are not published. The capture, graphics and other product
limitations documented below and in the README still need to be considered for
a particular show.

See [Customize the workspace](docs/how-to/customize-the-workspace.md),
[Workspace layout](docs/reference/workspace-layout.md), and
[Recording](docs/reference/recording.md) for the shipped contracts.

## Earlier handoff and investigation record

Written 2026-09-15 at the v0.2.0 tag. Read this first if you are picking the
work up fresh. The product requirements live outside this repository, in the
private `~/workspace/modulargodwinmix` (remote `psmux/modulargodwinmix`), and
must never be copied into this one.

## Built and merged

Everything in the plan's phases 0 to 6 except the items listed below. The
protocol and generated clients, the plugin host with fourteen first party
plugins, the SDK and templates, scenes with the composer, safety and the agent
surface, presets and themes, hooks, session replay and evals, transitions,
nodes, the WASM tier, OGraf graphics, the desktop app with a bundled
GStreamer, the Docker image and the docs beside each of them.

Verified on an Apple M4 Pro: `cargo test --workspace`, `cargo clippy
--workspace --all-targets -- -D warnings`, `dev/smoke.sh`, `dev/ui-tests.sh`,
the three client suites and `python3 clients/gen/generate.py --check`.

## Known open items, in the order they matter

1. **The mixer command loop can wedge.** A core left running for hours answers
   `/api/v1/core/info` (which does not touch the mixer) and hangs forever on
   `/api/status` and `/metrics` (which do). The web UI then loads and draws
   nothing. Seen after a long idle run with an output reconnecting to an RTMP
   server that was not there. The suspect is the watchdog rebuild holding the
   loop until the 256 command queue fills: `crates/godwinmix-core/src/mixer.rs`
   around the supervisor tick, written up as Codex finding 12 with the
   measurement that a shorter `BLOCK_TIMEOUT` made removals seventeen times
   worse (`crates/godwinmix-core/src/mixer/slots.rs:95`). Reproduce by running
   a core overnight with an output pointing at a closed port. This is the one
   defect that stops somebody using the product.

   One wedge is found and fixed, on 2026-09-16, by a Linux runner: the
   preview teardown took a tile's queue to NULL while its streaming thread
   was parked inside the compositor's sink pad and released that pad
   afterwards, so the join never returned. The pad is released first now,
   in the preview and in the two mosaic paths with the same order. Whether
   the overnight wedge with a reconnecting output is the same thing is not
   known; the soak still needs running. What is new either way: every call
   that waits on the mixer thread has a five second deadline and answers
   with the command holding the loop and for how long, `/api/status` and
   `/metrics` answer 503 with that instead of hanging, and a watchdog on the
   supervisor's timer logs it once. A wedge is now a diagnosable error
   rather than a silent hang.

   The second wedge, found by the soak on 2026-09-17 and reproducible in
   under a minute with `dev/soak.sh`, was the one that matters: removing a
   source took its branch's proxy source to NULL while a serialized
   allocation query was still travelling down that branch, and the query's
   tail was a slot thread parked in a compositor pad nobody had woken.
   Deactivating the pad needs the stream lock the query holds, so the mixer
   thread waited for a thread that was waiting for it, for as long as two
   minutes. A tee handing out or taking back a pad makes the source
   renegotiate, so every add and remove sent a fresh query down a live
   branch. A flush at the slot's queue on unbind and at the branch's video
   queue on detach frees the parked thread before any state change. After
   it, thirty six soak rounds in three minutes: no unanswered call, every
   removal inside 200 ms, threads and descriptors flat. Two bars still
   fail, both older: the programme stall gauge keeps one early 50 ms
   hiccup for the whole run, and resident memory grows about 3 MB a round
   with threads and descriptors flat, which reads as retained buffer pool
   heap rather than a leaked pipeline. Both records are in `bench/results`.
2. **CI ran for the first time on 2026-09-15** and found what had only ever
   been built on Apple silicon: a test helper not gated to Unix (Windows did
   not compile), `c_char` hardcoded as `i8` (aarch64 Linux did not compile),
   a race in the WASM worker that marked an instance spent after answering,
   and wall clock timing tests that a shared runner cannot hold. All four are
   fixed on 2026-09-16; the hosted jobs set `GODWINMIX_TIMING_SLACK=3` and
   nightly keeps the strict 34 ms. The second Windows run found thirteen
   more, all fixed the same day: a stderr reader joined before its child
   was killed (a hang), Windows paths written into pipeline descriptions,
   `unixfdsink` asked for on a platform without it, and a CRLF checkout
   inflating the UI byte budget. The Linux runners then exposed a real
   defect: a thumbnail branch was linked to a live tee before it was brought
   up, so a buffer could meet a flushing pad and pause the source's loop for
   good with nothing on the bus. The same order is now used for the output
   feeds and the audio tap. macOS is green end to end; Linux and Windows are
   one push from it as this is written. The release installers are unproven.
3. **A core exited silently once during mosaic teardown**, unattributed. One
   cause is now known: a source pipeline dropped without `stop` was disposed
   while PLAYING, which GStreamer answers with a critical and, with many
   pipelines in one process, a segmentation fault. Seen locally when a test
   failed an assertion and let its mixer go; `InputPipeline` now takes its
   pipeline to NULL on drop, and eight runs of the core suite in a row were
   clean after it. The second cause, seen on macOS runners as an audio
   monitoring branch disposed while PLAYING with no test failing first, is
   the bin's own state walk putting a freshly NULLed element back up before
   it was removed; sixty of sixty runs under load reproduced it and zero
   with the fix. Every branch teardown now locks the element's state before
   NULL, the way `Encoder::detach` always did. The third cause, found by the
   ten minute soak on 2026-09-17 at round 61 with a crash report to prove it:
   the compositor's aggregate thread walks a pad's control bindings without
   the object lock (GStreamer's own source says so), and a settling
   transition removed a binding from the mixer thread under that walk. No
   binding is removed while the pipeline runs any more: one binding per pad
   and property is made the first time a transition drives it and kept for
   the pad's life, a transition rewrites the curve behind it, and settling
   collapses the curve and disables it. Two ten minute soaks after that ran
   to the end with the core up, descriptors and threads flat.

   The stall gauge's 50 ms was real, and is fixed: hiding a slot that still
   had a source bound to it shut its valve, and the programme compositor
   then waited out its whole upstream latency, one second, for a pad that
   had gone quiet, on every take that took a slot off air. A bound slot
   keeps feeding while hidden now and the gauge reads 34.3 ms, against a
   bar of 34; the last 0.4 ms is the source add and remove phase. Resident
   memory still grows, about 1 MB a second while the mosaic is up, and is
   measured to be GStreamer buffer pools on the programme path ratcheting
   their high water mark each time the mosaic taps and untaps the tee, not
   a leak: nothing is unreachable and no element, pad or pool outlives its
   round. Bounding the raw path's queues by buffers as well as by time
   would cap it and changes backpressure on the programme tee, so it waits
   for its own tests. Every record is in `bench/results`, and `dev/soak.sh
   --skip` bisects by phase.

   The scene preview on a Linux runner never produced a frame inside the
   API's two seconds, and once on a macOS runner. The mosaic's tile
   branches start at a proxysrc that answered no latency of its own, so
   the query crossed into the programme and came back with the programme
   compositor's one second budget: the mosaic held its first frame 1.26 s,
   the preview 450 ms, and a tile tee linked straight to a full mosaic pad
   starved the preview branch beside it. The runners' GStreamer 1.24 has
   no clock based start for a force live aggregator, so there the preview
   could not produce at all. The proxy now answers the latency itself, as
   every other boundary in the core does; the mosaic and preview declare
   375 ms and 250 ms and a test holds both under 500 ms.

   The first nightly soak on a hosted Linux runner, sixty minutes on
   2026-09-17: 720 rounds, the core up at the end, no panic, no call
   unanswered, descriptors and threads flat. Two bars missed: one 51 ms
   stall at round 495, forty minutes in, and resident memory up 52 percent
   over the hour, which is the buffer pool ratchet above. The overnight
   wedge this item opened with has not been seen since the fixes above.
4. **Alpha graphics key to black**, because the graph is I420 throughout. The
   four edits needed are listed in `docs/reference/graphics.md`.
5. **Smaller gaps**, each with its file and line in the git history: only
   sources take a `node:` placement; `source.set {place}` is remove then add
   rather than a pad swap; `gmx plugin new --kind graphic` has no template;
   the composer's picker does not offer graphics; `SidecarFilter` refuses the
   container transport; `ext.preview = "full"` composites at thumbnail detail.

## Running it

    cargo build --release -p godwinmix
    ./target/release/godwinmix --example-config > godwinmix.toml
    GODWINMIX_TOKEN=secret ./target/release/godwinmix --config godwinmix.toml

Then http://127.0.0.1:8080/ and the token. `dev/smoke.sh` is the end to end
check, `dev/soak.sh` the long one, `docs/README.md` the index.
