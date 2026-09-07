# LiveboxMix

A live RTMP video mixer. Several RTMP sources come in, one of them is on
program at a time, and the program feed goes out to one or more RTMP
destinations without ever stopping. Switching source is instant and does not
disturb the outgoing stream.

Written in Rust on GStreamer. One binary, one port. Runs with a GPU or without.

## The idea in one paragraph

An RTMP output has to look like a single unbroken stream: monotonic timestamps,
no gaps, codec parameters that never change. So the output encoder is started
once and runs until the broadcast ends. Everything that changes during a
broadcast happens upstream of it, in raw video and raw audio, where switching
source is a property change on a compositor pad. The encoder cannot tell that
anything happened, so nothing downstream reconnects.

## Architecture

```
 source 1 ──┐ own pipeline                      program pipeline
 source 2 ──┤ rtmp2src → decode → normalise ──▶ compositor ─┬─▶ encoder ─▶ tee ─┐
 source N ──┘ (proxysink/proxysrc boundary)     audiomixer  │                   │
                                                            │                   │  proxy
 slate (black) ────────────────────────────────▶ always on  │                   │  boundary
 silence ──────────────────────────────────────▶ always on  │                   │
                                                            └─▶ mosaic ─▶ JPEG  │
                                                                   │            ▼
                                            control server ◀───────┘   output pipeline(s)
                                            HTTP + WebSocket            flvmux → rtmp2sink
```

Three isolation boundaries, each a separate `GstPipeline`, because a pipeline is
the unit of error propagation in GStreamer:

* **Every source.** A camera that drops, errors or sends garbage cannot post a
  bus error into the program pipeline.
* **Every output.** A failing RTMP sink returns a flow error that would
  otherwise travel back through the tee and tear down the program pipeline's
  streaming threads. One dead CDN would take the whole broadcast with it.
* **The multiview.** A preview encoder problem cannot touch program.

Two more things stop the output ever stalling:

* The compositor and audiomixer are built with `force-live`, so they emit black
  and silence on schedule even with every input dead.
* A slate sits permanently at the bottom of the compositor's z order. Losing a
  source reveals black rather than freezing on its last frame.

The outage buffer (`queue_secs`, default 5s of encoded data) lives on the
program side of the proxy boundary, so it survives a reconnect that rebuilds the
output pipeline. It leaks downstream rather than blocking, so a slow destination
cannot apply backpressure to the encoder that every other output shares.

## What has been verified

Tested against two independent RTMP servers, **mediamtx** and **node-media-server
v4**, with 720p30 sources. Sources were generated with ffmpeg and the output was
measured with ffmpeg, so the verification does not share a code path with the
mixer's own GStreamer stack.

| | result |
|---|---|
| takes on both servers | program output stayed at exactly 30 frames per second, min and max both 30, no gap at any take |
| the server's own record | 1 publish event for the program path, **0 disconnects**, across repeated takes |
| cut to slate | true black (uniform luma 16 video range) |
| output connection killed repeatedly | one reconnect each, recovered every time, about 2s |
| whole RTMP server killed for 15s | 12 retries with backoff, no storm; output live 5s after the server returned, sources 10s |
| throughout all failures | 0 program pipeline errors, 0 panics, process survived |
| both cameras dead | program output still live and decodable, showing the slate |
| ad break, immediate and scheduled | full 8s file played (240/240 frames), 30 fps in every second, auto-return to the right camera |
| scheduled cue accuracy | +4 ms against the requested running time (one frame is 33 ms) |
| audio through a break | measured by spectral centroid: 440 Hz camera, 1000 Hz ad, 0 A/V mismatches across every take and break |
| ad file missing | break refused, programme untouched, later breaks still work |
| HLS source added at runtime | live with audio, taken to programme and back with no gap, 30 fps throughout |
| source list across a restart | runtime additions reload from the sidecar and come back live |
| rejected requests | duplicate id, empty id, empty URL, reserved id and missing ad file all answer 400 with the reason |
| adding and removing outputs while live | second destination carried H.264 + AAC; programme stayed at 30 fps, largest gap 34 ms (one frame) |
| full headless operation | every take, source, output, ad and media operation driven from `ctl` with no desktop app running |
| adding or removing a source at runtime | no gap, largest inter-frame interval 34 ms (one frame at 30 fps) |
| multiview over WebSocket | 8.0 fps, 22.7 KB/frame, about 1.5 Mbit/s |

63 unit tests, including a regression for every bug found during that testing.

* **Superimpose on air, indistinguishable from the whole page.** A demo page
  with three videos (a lead clip with sound and two muted sidebar clips) was
  taken to programme with `superimpose = "auto"`. All three played in place,
  the header, clock, captions, sidebar labels and body text stayed visible over
  them, and the lead clip's sound came through at its own level. On this Mac the
  browser side of that page fell from about 130% CPU rendering the whole page to
  about 28% superimposed, with the three decodes moving to the hardware decoder.
* **Browser source at full quality.** The CEF sidecar's raw frames arrive on air
  sample for sample (background Y 35, orange 146, white 235 in capture and
  programme), after fixing the colorimetry drift described under Colour.

### RTMP client interoperability

GStreamer ships two RTMP client implementations and they do not work with the
same servers. Against node-media-server, `rtmp2src` connects, is accepted by the
server, and then delivers **nothing at all**, with no error: 0 buffers where the
librtmp based `rtmpsrc` delivered 250 from the same stream. ffmpeg reads that
same stream fine, so it is a client problem, not a server one.

Because the failure is silent, `rtmp_client = "auto"` starts with the modern
client and switches once after six seconds without media. Pin `rtmp2` or
`librtmp` per source to skip the wait. The publish direction (`rtmp2sink`) works
with both servers and is not affected.

## Quick start

```sh
brew install gstreamer          # or your distro's gstreamer + plugins base/good/bad/ugly/libav/rs
cargo build --release

./target/release/liveboxmix --probe            # what codecs will be used here
./target/release/liveboxmix --example-config > liveboxmix.toml
./target/release/liveboxmix --config liveboxmix.toml
```

Open `http://localhost:8080`. Click a cell to take that camera. Number keys 1
to 9 take directly, 0 or Escape cuts to black.

## Platforms

Linux, macOS and Windows, the same code on all three. The mixer, its tests and
the desktop app are built on every push on all three by the GitHub Actions
workflow in `.github/workflows/build.yml`, and each run leaves the binaries
behind as artifacts. What differs by platform:

| | GStreamer | hardware codecs | browser sidecar | desktop app |
|---|---|---|---|---|
| Linux | distro packages | NVIDIA, VA | native, with H.264 from a prebuilt CEF (see Codecs) | `.deb`, AppImage |
| macOS | `brew install gstreamer` | VideoToolbox | `.app` bundle from `browser/dev/mac-bundle.sh`, or the Linux one in a container | `.app` |
| Windows | the MSVC runtime and development MSIs from gstreamer.freedesktop.org, or `choco install gstreamer gstreamer-devel`; put `C:\gstreamer\1.0\msvc_x86_64\bin` on `PATH` | Media Foundation, NVIDIA | `liveboxmix-browser.exe` next to the mixer, from `cd browser; cargo build --release` | `.msi`, NSIS |

Two things are worth knowing on Windows. Sources whose media arrives on a
pipe (the browser sidecar, `exec:` sources) are read by a thread of the mixer
into an `appsrc` rather than by `fdsrc`, because there is no file descriptor
GStreamer could read; the bytes and the timing are the same. And the official
CEF build has no H.264 or AAC, on Windows as on macOS, which is exactly the
case `superimpose` is for: the mixer decodes the page's video with Media
Foundation and the browser draws only the page. The development scripts under
`dev/` are bash; `dev/desktop.ps1` is the Windows launcher for the desktop app,
and the test rig (mediamtx, synthetic camera) runs under WSL if you want it.

## Hardware

The same binary picks its codecs at startup and works with or without a GPU.

| | decode | encode |
|---|---|---|
| NVIDIA | `nvh264dec` | `nvh264enc` |
| Intel / AMD (VA) | `vah264dec` | `vah264enc` |
| Windows | `d3d11h264dec` | `mfh264enc` |
| macOS | `vtdec_hw` | `vtenc_h264_hw` |
| software | `avdec_h264` | `x264enc` |

Decode and encode are chosen independently, because a machine with NVDEC but no
usable NVENC is a real configuration. Set `hardware.decode` or `hardware.encode`
to a specific backend to make startup fail loudly if it is missing, which is
what you want on a server you control. Every property is set defensively:
backends disagree about names, units and integer widths, and a property that
does not exist on one of them is a logged warning rather than a crash.

## HTTP API

| | |
|---|---|
| `GET /api/status` | full snapshot |
| `GET /api/media` | ad clips found in the configured library |
| `POST /api/take` | `{"source": "cam1"}`, or `{"source": null}` for black |
| `POST /api/sources` | add a source at runtime: `{"id","uri","name","kind","superimpose"}` |
| `DELETE /api/sources/{id}` | remove one |
| `POST /api/adbreak` | `{"uri": "/path/to/ad.mp4"}`, optional `at_running_time_ms` and `return_to` |
| `POST /api/adbreak/end` | cut the ad short and return early |
| `GET /api/outputs` | destinations and their state |
| `POST /api/outputs` | add a destination: `{"id","uri","policy"}` |
| `DELETE /api/outputs/{id}` | stop sending to one |
| `POST /api/outputs/{id}/reconnect` | force a reconnect |
| `POST /api/shutdown` | stop the mixer; what the desktop app's "Quit and stop the mixer" sends |
| `GET /ws` | JSON events and state, plus mosaic JPEGs as binary frames |

A scheduled take takes `at_running_time_ms`, armed on the pipeline clock so it
lands on the intended frame rather than whenever the request happened to arrive.

## Sources

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

Sources added or removed from the UI are written to `<config>.sources.toml`
beside the config file. Once that file exists it is the authoritative list:
merging it with the config's own `[[sources]]` would mean a source deleted in the
UI reappearing at the next restart. Delete the file to go back to the config.

### Anything as a source

`exec:` makes a source out of a command line. The process writes a container to
stdout, MPEG-TS being the usual choice, and the mixer demuxes and decodes it
through the same hardware-aware path as everything else. That means an `exec:`
source is GPU accelerated on a machine with a GPU and falls back to software on
one without, with no change to the command.

```sh
liveboxmix ctl source add gen \
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
liveboxmix ctl source add site \
  "exec:/opt/liveboxmix/tools/browser-source.sh https://example.com/page 1280 720 30"
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

### Web pages as sources: the browser sidecar

Adding a page by URL renders it in a real Chromium and puts what it draws and
plays on the canvas like any other source. No tricks with screen capture, no
encoder in between.

Three ways a page can reach the canvas, from most to least expensive:

1. **The sidecar renders the whole page.** What every `web+` source does when
   `liveboxmix-browser` is found. Works everywhere, and a page playing video
   costs about a CPU core, because Chromium decodes the video in software and
   repaints the whole page around it thirty times a second.
2. **A renderer inside GStreamer, or a screen grab.** Without a sidecar the
   mixer falls back to `wpesrc`, and there is the older `browser-source.sh`
   under `exec:`. Both Linux only. See the sections above and below.
3. **Superimpose.** The sidecar finds the video the page is playing, the mixer
   decodes that itself on the GPU, and the browser draws only the page over it,
   transparent where the video was. Opt in, per source, and the cheap one. See
   "Handing the video over" below.

In the UI, "Add a source" starts on **Website**: paste the address as it is
in your browser's address bar and click Add. The type lights up by itself
from what was pasted (an `rtmp://` or `.m3u8` address switches to "Camera or
stream"), the name defaults to the site's host, and the id is derived from
the name. Nobody types a prefix. The same holds for the API and the CLI:

```sh
liveboxmix ctl source add - https://www.youtube.com/watch?v=aqz-KE-bpKQ --web --name YouTube
liveboxmix ctl take youtube
curl -X POST localhost:8080/api/sources -H 'content-type: application/json' \
  -d '{"uri":"https://www.youtube.com/watch?v=aqz-KE-bpKQ","kind":"web","name":"YouTube"}'

# Let the mixer decode the page's own video where it can. See superimpose below.
liveboxmix ctl source add game https://example.com/live-game --web --superimpose auto
curl -X POST localhost:8080/api/sources -H 'content-type: application/json' \
  -d '{"uri":"https://example.com/live-game","kind":"web","superimpose":"auto"}'
```

`kind: "web"` (or `--web`) says "this is a website"; `id` may be omitted and
is then derived from the name or host, made unique with a suffix. The
`web+https://…` form still works everywhere and is what the mixer stores.
Two things about sites: players that autoplay start on their own (the
browser is told no gesture is needed); a player that waits for a click shows
its poster. And YouTube's `/embed/` URLs refuse to load as a top level page
(Error 153); paste the normal `/watch?v=` address.

`superimpose` is described further down. It is `"off"` unless you ask for it,
and a source that is not a website accepts the field and never looks at it.

`browser/` holds the renderer, `liveboxmix-browser`. It embeds Chromium
through CEF with off screen rendering: Chromium paints each frame into memory
and hands the audio over as float PCM through its audio handler. The frames
leave the process as raw I420 and the audio as 48 kHz float, in a Matroska
stream on stdout, and the mixer reads that as an `exec:` source. The page's
content reaches the programme encoder sample for sample: on air the test
page measures black 16, white 235, exactly what it painted.

The mixer runs it for every `web+` source when it can find it: at
`browser.sidecar` in the config, else next to its own executable (as
`liveboxmix-browser` on Linux, `liveboxmix-browser.app` on macOS), else on
`PATH`. Without one, `web+` falls back to GStreamer's `wpesrc`, described
below. `[browser]` also takes extra `args` and `env` for the sidecar.

```toml
[browser]
# sidecar = "/opt/liveboxmix/liveboxmix-browser"
# args = ["--audio-offset-ms", "0"]
# env = { LBX_BROWSER_SWITCHES = "enable-gpu" }
```

#### Finding the media a page is playing

`--detect-media` injects a small script that watches the page and reports what
it is really playing, on stderr, once per change:

```
[browser] media {"found":true,"count":3,"tag":"video",
                 "src":"http://host/lead.mp4","usable":true,"mse":false,
                 "drm":false,"paused":false,"rect":{"x":50,"y":80,"w":890,"h":501},
                 "intrinsic":{"w":960,"h":540},"viewport":{"w":1280,"h":720},
                 "media":[ {"index":0,"src":"http://host/lead.mp4","muted":false,...},
                           {"index":1,"src":"http://host/side1.mp4","muted":true,...},
                           {"index":2,"src":"http://host/side2.mp4","muted":true,...} ]}
```

The top-level fields describe the first video, for readers that only want one;
`media` is the full list, one entry per `<video>` on the page, each with its own
`rect`, `usable`, `muted` and the rest. `superimpose` acts on the whole list.

`usable` is the field that matters: it says a decoder outside the browser could
open this URL. That is the case for a plain `<video src>` and for an HLS or DASH
address, and it is not the case for the two things worth knowing about:

* **Media Source Extensions.** The page feeds segments to the decoder from
  JavaScript and the element's `src` is a `blob:` URL that only exists inside
  that renderer. YouTube works this way, and reports `"mse":true`.
* **Encrypted Media Extensions.** Frames are decrypted inside the browser and
  by design never leave it. Reports `"drm":true`.

`rect` is where the element sits in the viewport and `intrinsic` is the coded
size the decoder would produce, both of which the mixer needs to put a directly
decoded picture exactly where the page had it.

#### Handing the video over: `superimpose`

`--detect-media` only reports. `superimpose` is the per-source option that acts
on the report, in the config file, over the API, or from the CLI:

```toml
[[sources]]
id = "game"
uri = "web+https://example.com/live-game"
superimpose = "auto"   # "off" is the default and is what every web source did before
```

`auto` means: when the page's media has an address a decoder can open, the
mixer decodes it on the GPU like any other source and the browser draws only
the page over the top, transparent where the video was. `off` renders the
whole page in the browser.

Every `<video>` on the page is handed over, not just one. A page with a lead
clip and two sidebar clips has all three decoded by the mixer, each placed at
the rectangle the page gave it, each looped or streamed on its own, and the
lead clip's sound mixed in while the muted ones stay muted, exactly as the
page had them. The page is drawn over all of them at once.

What the page draws over its videos survives the hand-over. A caption, a
lower third, a logo, a headline, an animation: anything the page paints on top
of a video, at any transparency, comes through as it looked in the browser.
The page paints a key colour where each taken-over video sat, and the sidecar
both removes that key and recovers the real colour of whatever the page
blended over it, so a caption's dark gradient or a control's soft edge is not
lost with the key. The one thing it cannot keep is page content that is itself
the key colour, a near-pure magenta, which is taken for the key; nothing else
is affected. The intent is that a viewer cannot tell a superimposed page from
the same page rendered whole.

The saving is the reason to bother. A page playing video costs about a whole
CPU core. Chromium decodes every frame in software, repaints the page around
it, and the result crosses to the mixer as raw frames, which is three jobs to
put one video on the canvas. Decoding that video on the GPU and painting a
nearly static page over it is a fraction of the same work.

It does not always apply, and `auto` falls back rather than failing:

* Media Source Extensions. The page feeds its player from JavaScript and the
  element's `src` is a `blob:` URL that exists only inside that renderer, so
  there is no address to hand over. **YouTube is MSE**, and so are most
  streaming sites. Those keep rendering in the browser and cost what they
  always cost.
* DRM. Frames are decrypted inside the browser and by design never leave it.
* A page with no media element at all, which is most pages, and which is why
  `off` remains the default.

The trade-off where it does apply is the page's own player UI. The element the
mixer takes over is paused as well as hidden, because hiding alone leaves
Chromium decoding every frame into a surface nobody looks at and the decode is
the whole cost. Paused means the page's progress bar stops filling and its
running time stops counting, while the video itself, now the mixer's, plays
normally. On a page whose player chrome is part of what you are broadcasting,
leave this `off`.

How it happens, because three of the details show:

* **Adding takes a few seconds longer.** The page is loaded once first, just to
  ask what it plays, and the source is built only when the answer is in. That
  took 1 to 6 seconds against the local pages when the video was found, less
  when the page said straight away that its video cannot be handed over (MSE,
  DRM), and up to 20 when there was nothing to find. It runs on its own
  thread, so the API and the UI keep answering meanwhile; only the add call
  itself waits.
* **The page's video is looped by the mixer.** A page that loops a background
  video does it in the browser, and the browser's copy is now paused, so the
  mixer loops its own copy. A clip is fetched once to a temporary file when
  the source is added, so going round again costs an open and a decoder start
  rather than a connection and an index read, and the join does not show. A
  stream (HLS, DASH, or anything still arriving after 60 seconds) is played
  from its address and never loops; one that ends leaves the page over its
  last frame.
* **The page draws at `browser.overlay_fps`, default 10,** not the canvas rate.
  It is drawing chrome, not video, and its frames now carry an alpha channel at
  4 bytes a pixel against I420's 1.5: a 720p page at 30 fps measured 110 MB/s
  down a pipe that carries 41 MB/s for an ordinary source, and 36.9 MB/s at 10.
  The compositor holds the last page frame between updates, so the output still
  leaves at the canvas rate with the video moving underneath. Raise it for a
  page with real animation in it, and expect to pay for that.

If the sidecar runs in a container through `browser/dev/sidecar-docker.sh`,
the mixer must be able to open the media address the page reports. That is
automatic when both run on one machine, which is the production arrangement.
On a laptop with Docker in a VM, a page reached as `host.docker.internal` hands
back a `host.docker.internal` media URL that the host itself cannot resolve; use
an address both sides can reach.

Since the fallback is silent, the mixer reports what actually happened rather
than what was asked for. `GET /api/status` carries `superimposed` on every
source, `liveboxmix ctl status` and `ctl source list` mark those sources
`(superimposed)`, and the UI puts a green `direct` badge on the row. A website
source set to `auto` with no badge is working normally; it simply had nothing
to give.

Building it. Linux: `cd browser && cargo build --release`, which downloads the
CEF distribution and stages it next to the binary (`CEF_PATH` picks where the
download is cached); the binary finds the libraries and resources next to
itself, no `LD_LIBRARY_PATH`. It needs an X display to start against even
though nothing is drawn on it, `Xvfb :99` is enough, and nothing else: no
PulseAudio, no sound card. macOS: `browser/dev/mac-bundle.sh` builds the app
bundle CEF requires there (framework plus one helper app per Chromium process
type, ad hoc signed). The mixer launches the binary inside the bundle.

Runs with or without a GPU. Chromium rasterises in software by default here
(`--disable-gpu`); the switches Chromium is started with can be extended
with `LBX_BROWSER_SWITCHES="a,b=c"` in the sidecar's environment.

**Codecs.** Everything Chromium does: WebAudio, WebGL, canvas, HTML5 video
in VP8, VP9, AV1 and Opus. H.264 and AAC depend on which CEF binary sits
next to the sidecar. The official CEF binaries the crate downloads are built
without them, for patent licensing reasons, and a `<video>` with an H.264
file then reports `MEDIA_ERR_SRC_NOT_SUPPORTED` (checked with
`browser/test/video-mp4.html`). So the sidecar is built against a CEF that
has them:

* **Linux, x86_64 and arm64: a prebuilt distribution with codecs.** The
  [Karere](https://github.com/tobagin/karere/releases) project publishes
  standard CEF minimal distributions built with `proprietary_codecs=true`,
  for `linux64` and `linuxarm64`, and the `cef` crate has a release for the
  same CEF version. `browser/Cargo.toml` pins both `cef` and `cef-dll-sys` to
  `=150.0.0`, which is CEF 150.0.10; `browser/dev/install-cef-dist.sh` puts
  the downloaded archive where the crate looks, in the layout the crate's own
  downloader produces, and `cargo build` then links against it instead of
  fetching the official one. Nothing else changes: same binary layout, same
  Debian based image, same `exec:` source.

  ```sh
  gh release download cef-150.0.10-proprietary-codecs -R tobagin/karere --pattern '*linux64*'
  browser/dev/install-cef-dist.sh cef_binary_150.0.10+*_linux64_minimal.zip ~/.cache/lbx-cef
  cd browser && CEF_PATH=~/.cache/lbx-cef cargo build --release
  ```

  Verified on the plain Debian image (arm64): the H.264/AAC `<video>` page
  plays, six flashes and five beeps found in a 12 s capture, audio +29 ms
  (+18 to +38), and the other pages unchanged (WebAudio +9 ms, VP9 video
  −3 ms). Shutdown on SIGTERM still 500 ms with nothing left behind.

  Both crates are pinned because the sys crate is the one that downloads and
  checks the distribution, and its own version metadata names the CEF
  version; with only `cef` pinned, cargo picked a newer sys crate and quietly
  fetched the codec-less 150.0.14.
* **Linux x86_64, alternative: Arch Linux's `cef` package**, also built with
  proprietary codecs. `browser/dev/Dockerfile.arch` and
  `browser/dev/linux-arch-codecs.sh` build against it, and
  `Dockerfile.arch-runtime` plus `sidecar-docker.sh` package the result as a
  container the mixer runs through Docker (the wrapper forwards the stop
  signal; the container is gone 0.9 s after `source remove`;
  `LBX_SIDECAR_LOG=<file>` in `browser.env` keeps the sidecar's log). The
  wrapper runs the container with `--log-driver none`, and that is not
  optional: Docker's default log driver copies everything a container writes
  to stdout into a JSON file on disk, so each sidecar's 41 MB/s of raw video
  was also being written to the VM's disk. It filled 30 GB in minutes, the
  players inside stalled on the full disk, and the symptom on air was a frozen
  picture with silent audio and a container using a fifth of a core. With the
  driver off the same two sites play with sound, zero dropouts, 65 percent of
  a core each.
  `browser/dev/Dockerfile.runtime` builds the same kind of container from the
  drop-in distribution above, which is what a macOS workstation uses to get
  the codecs today. Give the Linux VM CPU: on this laptop a 4 vCPU VM starved
  the sidecar under a 720p30 H.264 page (irregular frames, 21 audio dropouts
  in 15 s); with 8 vCPUs the same page went to air clean, six of six flashes
  and beeps, audio −28 ms, no dropouts. The sidecar logs every dropout as
  `audio re-anchored`, so a starved box shows itself. It also shows the
  ceiling of a VM on a laptop: the YouTube watch page (VP9 or AV1 at 60 fps,
  decoded in software) rendered and played with sound in that 8 vCPU VM, but
  with 32 dropouts in 25 s and a stuttering picture. That is CPU, not the
  sidecar: the same page on a Linux server runs natively with the whole
  machine, and with a GPU Chromium can be given hardware decoding
  (`LBX_BROWSER_SWITCHES="enable-features=VaapiVideoDecoder"` without
  `disable-gpu`, untested here). On a Mac, the native codec-less `.app` plays
  YouTube smoothly, because YouTube does not need H.264. Verified the
  same way: the H.264/AAC page on air through the mixer at 30.0 fps, six of six
  flashes and beeps, audio +17 ms, black 16, white 235. It is more moving
  parts than the drop-in distribution, and its `libcef.so` links Arch's
  system libraries, so it only runs in an Arch based image.
* **macOS and Windows: build CEF with codecs.** Nobody publishes those.
  `browser/dev/build-cef-codecs.sh` is CEF's own automated build pinned to the
  crate's exact commit with `proprietary_codecs=true ffmpeg_branding=Chrome`.
  It is a Chromium build: hours, 100 GB of disk. Until then a workstation
  runs the containerised Linux sidecar through Docker, which is what the Arch
  route above was measured with, from this Mac.

Shipping a software H.264 or AAC decoder is what the licensing is about;
that is a decision for whoever distributes the build, not something the code
can settle.

**Sync.** Video and audio are stamped against one clock in the sidecar: a
frame with the pacer tick that sends it, an audio packet with the presentation
time Chromium attaches to it. Getting there took measuring: the obvious
scheme, arrival time for audio and the previous tick for video, put audio
60 ms late. `browser/test/sync.html` flashes and beeps together every two
seconds, scheduled on the page's audio clock, `video-webm.html` plays a
`<video>` with the pattern baked in, and `browser/dev/measure-sync.py` reads
where the flashes and beeps land in any capture.

Measured, sidecar alone: Linux, WebAudio page, mean +14 ms (range −29 to
+79, one frame); Linux, VP9 video, −4 ms (−13 to +2); macOS, WebAudio page,
−10 ms (−20 to +30); macOS, VP9 video, +5 ms (−5 to +15). Positive is audio
late. `--audio-offset-ms` exists for a page where a measurement says
otherwise.

On air through the mixer the first result was +44 ms, and a known good file
through an `exec:` source gave the same, so the mixer was adding it. It was
the AAC encoder: fdkaacenc leaves its 2048 samples of priming delay in the
timestamps (43 ms), avenc_aac 1024 (21 ms), measured in
`browser/dev/tail-offset.sh` with every video encoder and with and without
the mixers in the path. The mixer now holds video back at the programme
encoder by the delay of the AAC encoder in use, `program.av_offset_ms`
overrides it, and the test page measures −4 ms on air. Repeat runs land
anywhere within one frame of that (the VP9 page +37 ms, a synced file
through `exec:` −22 ms): the compositor shows a source frame on whichever of
its own 33 ms ticks covers it, so where a flash lands depends on the phase
between the source and the canvas. A 30 fps canvas cannot do better than a
frame; a 60 fps one halves it.

One more thing the page found. Every `exec:` source used to go through
`livesync`, and the sidecar's frames all came out black on air while the
audio played: the sidecar stamps time from its own start, half a second
after the pipeline reading it, and livesync judged every frame late against
the mixer's clock, dropped it, and repeated the first one. A process paces
its own output, so exec sources skip livesync now; the pad offset and the
compositor's latency place them.

Shutdown is clean: on `source remove` the mixer signals the process group,
the sidecar quits its message loop, and no Chromium process is left behind
(checked on both platforms). The sidecar also stops itself when the mixer
goes away and its stdout closes.

### Web pages as sources (wpesrc)

The fallback when no sidecar is installed. A page is rendered by WPE WebKit
inside the mixer's own process:

```sh
liveboxmix ctl source add game web+https://example.com/live-game
```

* It needs GStreamer's `wpesrc` (WPE WebKit): `gstreamer1.0-wpe` on Debian and
  Ubuntu, `gst-plugins-bad` built with `wpewebkit` elsewhere. **There is no
  macOS build of it.**
* Audio needs a recent enough `wpesrc`; older builds expose video only.
* **It needs OpenGL, not just a CPU.** `wpesrc` draws into GL memory, and the
  mixer downloads it from there. On a headless box that means an EGL capable
  stack (Mesa's llvmpipe will do without a GPU, but it must be present).
* It has not been verified end to end here: the element negotiates and the
  chain builds, but the machines this was developed on had no WPE with EGL.
  The sidecar has, on both platforms, which is why it comes first.

## Ad breaks

Clips live in a directory on the machine running the mixer, set by `media.dir`.
The UI lists them with their durations and marks any without an audio track;
click one to arm it, double-click to roll it straight away.

The library is server side on purpose. A browser file picker returns a file from
the operator's own machine with no usable path, and the mixer needs something it
can open itself, so a picker would only ever work when the operator happened to
be sitting at the server. Listing a directory works identically over the network.

Interrupt the programme with a file, then rejoin live:

```sh
curl -X POST -H 'Content-Type: application/json' \
  -d '{"uri": "/path/to/ad.mp4"}' http://localhost:8080/api/adbreak
```

There is no time shift buffer, by design. The source keeps running behind the ad
and the mixer rejoins it live, so whatever played during the break is not shown.

An ad is just another source as far as the mixer is concerned. It is decoded and
normalised to the same canvas contract as a camera, which is why it reuses the
same take, the same audio crossfade and the same slate behaviour, and why
switching to and from it costs the output nothing.

Five things had to be handled to make this work inside a live programme:

* **A shared clock.** Every source pipeline is put on the program pipeline's
  clock and base time. Live RTMP inputs get away without it because their timing
  comes from arrival, but a file's timestamps start at zero.
* **Rebasing.** The ad's pads are offset onto programme time when it rolls, with
  the same offset on video and audio so it stays in lip sync.
* **Ending on the clock, not on end-of-stream.** EOS arrives while more than a
  second of already-decoded ad is still in flight through the queues. Returning
  to live at that moment truncates the ad: an eight second file played for 6.6
  seconds. The return is scheduled from the file's own duration instead, and now
  plays all 240 frames.

* **Aligning every source's timeline.** Each input lives in its own pipeline, so
  its segment starts when that source starts and its buffers carry running times
  beginning near zero while the programme may be hours in. A compositor hides
  this by reusing the frame it holds, so video looked right; an audiomixer
  cannot place samples it has no valid position for and discarded every one.
  Cameras appeared perfectly live and carried **no sound at all**. Each source's
  mixer pads now take an offset, computed from its first segment and shared
  between video and audio so lip sync is preserved.
* **Declaring the mixers' upstream latency up front.** Attaching a branch to a
  running aggregator otherwise makes the whole pipeline recalculate its latency,
  and output pauses while it settles. Rolling an ad cost about a second of
  programme that way.

Pass `at_running_time_ms` to place the break on a specific frame. A scheduled
break only arms a timer: its pipeline is built shortly before the cue, because
one held paused for six seconds rolled to black for its whole duration.

## Command line

The daemon is headless and controlled entirely over HTTP. `liveboxmix ctl` is a
thin client for that same API, so scripting it does not mean assembling JSON by
hand. Point it elsewhere with `--url` or `LIVEBOXMIX_URL`.

```sh
liveboxmix ctl status
liveboxmix ctl take cam2                 # or: take   (with no id, cuts to black)
liveboxmix ctl source add hls1 https://host/stream.m3u8 --name "Roof camera"
liveboxmix ctl source remove hls1
liveboxmix ctl output add youtube rtmp://a.rtmp.youtube.com/live2/KEY --policy cdn
liveboxmix ctl output list
liveboxmix ctl ad /srv/ads/spot.mp4 --return-to cam1
liveboxmix ctl media
```

Requests answer with the mixer's own reason for refusing rather than a bare
status code:

```
$ liveboxmix ctl source add cam1 rtmp://host/live/x
Error: 400 Bad Request: source cam1 already exists
```

Nothing here needs the desktop app; it is only a window onto the same API.

## Desktop app

The UI is a plain web app served by the binary, so a Tauri shell is a webview
pointed at the same URL, local or remote. There is no second implementation to
keep in step, and remote operation is the default case with the address changed.

To open it:

```sh
dev/desktop.sh              # starts the test rig if no mixer answers on 8080, then the app
dev/desktop.sh mine.toml    # same, but the mixer runs on your own config
```

Two ways out, as buttons at the top right of the window and as items in the
application menu. **Close window** (or the window's close button, or Quit)
leaves the mixer running: the stream does not live in this window and closing
it by accident must not take the programme down. **Stop everything** asks
twice, then has the mixer shut down over `POST /api/shutdown` and exits with
status 2, which `dev/desktop.sh` takes as the cue to stop the rest of the rig
as well: mediamtx, the camera and the page server. Nothing is left running.
The buttons appear only inside the desktop shell, which announces itself in
the user agent; in a browser the same page does not show them.

The window has nothing to show until a mixer answers on `localhost:8080`,
which is why the script starts one first. It opens the bundle at
`tauri-app/target/release/bundle/macos/LiveboxMix.app` when one has been
built (`cd tauri-app && cargo tauri build --bundles app`), else the bare
binary from `cargo build --release` in `tauri-app/`.

## Known limitations

* **Superimpose cannot take over MSE or DRM playback**, and YouTube is MSE.
  Those pages fall back to full rendering, which is reported rather than
  failed. The page's own player UI freezes on pages it does apply to, because
  the browser's copy of each taken-over video is paused; and a superimposed
  live stream that ends leaves the page over its last frame rather than
  restarting the source.
* **A superimposed source whose browser dies is rebuilt, not restarted.** The
  page is probed again, its clips fetched again and a new pipeline built, and
  the source goes back on programme if it was there. Measured at 14 seconds
  from the browser being killed to the page back on air with sound; the
  programme shows the slate meanwhile. Every other kind of source restarts in
  place in about two seconds. The difference is deliberate: brought back in
  place, the layered pipeline did not recover reliably.
* **Page content in the key colour is treated as spill.** The page paints a
  near-pure magenta where each video sat, and the sidecar removes it. A page
  element that is itself that magenta would be removed with it. Nothing else is
  affected, and no ordinary page uses that colour, but it is the one thing that
  does not survive the hand-over.

* **An ad whose duration cannot be queried** (a live URI rather than a file)
  ends on end-of-stream instead, which truncates the tail as described above.
  Files are fine; streams as ad sources are not really supported.
* **A failed ad is reported over the event stream, not in the HTTP response.**
  `POST /api/adbreak` returns 202 as soon as the command is queued, so a missing
  file shows up as an alert in the UI rather than a 4xx. The programme is not
  disturbed either way.
* **Reconnect takes about 2 seconds**, not milliseconds. Tearing down the output
  pipeline, swapping the proxy pair, rebuilding, and completing a fresh RTMP
  handshake costs that much. Viewers with a normal player buffer should not see
  it, but it is not instant and I have not measured it against a real CDN.
* **The mosaic carries no audio.** Program audio level meters are on the UI
  instead. WebRTC would fix this properly; `whepserversink` in GStreamer 1.28.6
  returned 405 on every method and path I tried, so MJPEG is what ships.
* **Multiview cost scales with source count** on the decode side, not the encode
  side. Each source is decoded once and tee'd to a full resolution branch for
  program and a small one for the mosaic.
* **H.264 and AAC in the browser sidecar need a CEF build that has them.**
  Linux has prebuilt ones (Karere's releases, Arch's package); macOS and
  Windows need your own build. The official binaries omit them. See Codecs.
* Sources are assumed to be H.264 and AAC, which is what RTMP carries in
  practice. Anything else is reported as a failed source rather than decoded.
