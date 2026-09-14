# Web page sources


Adding a page by URL renders it in a real Chromium and puts what it draws and
plays on the canvas like any other source. No tricks with screen capture, no
encoder in between.

Three ways a page can reach the canvas, from most to least expensive:

1. **The sidecar renders the whole page.** What every `web+` source does when
   `godwinmix-browser` is found. Works everywhere, and a page playing video
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
godwinmix ctl source add - https://www.youtube.com/watch?v=aqz-KE-bpKQ --web --name YouTube
godwinmix ctl take youtube
curl -X POST localhost:8080/api/sources -H 'content-type: application/json' \
  -d '{"uri":"https://www.youtube.com/watch?v=aqz-KE-bpKQ","kind":"web","name":"YouTube"}'

# Let the mixer decode the page's own video where it can. See superimpose below.
godwinmix ctl source add game https://example.com/live-game --web --superimpose auto
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

`browser/` holds the renderer, `godwinmix-browser`. It embeds Chromium
through CEF with off screen rendering: Chromium paints each frame into memory
and hands the audio over as float PCM through its audio handler. The frames
leave the process as raw I420 and the audio as 48 kHz float, in a Matroska
stream on stdout, and the mixer reads that as an `exec:` source. The page's
content reaches the programme encoder sample for sample: on air the test
page measures black 16, white 235, exactly what it painted.

The mixer runs it for every `web+` source when it can find it: at
`browser.sidecar` in the config, else next to its own executable (as
`godwinmix-browser` on Linux, `godwinmix-browser.app` on macOS), else on
`PATH`. Without one, `web+` falls back to GStreamer's `wpesrc`, described
below. `[browser]` also takes extra `args` and `env` for the sidecar.

```toml
[browser]
# sidecar = "/opt/godwinmix/godwinmix-browser"
# args = ["--audio-offset-ms", "0"]
# env = { GMX_BROWSER_SWITCHES = "enable-gpu" }
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
source, `godwinmix ctl status` and `ctl source list` mark those sources
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
with `GMX_BROWSER_SWITCHES="a,b=c"` in the sidecar's environment.

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
  browser/dev/install-cef-dist.sh cef_binary_150.0.10+*_linux64_minimal.zip ~/.cache/gmx-cef
  cd browser && CEF_PATH=~/.cache/gmx-cef cargo build --release
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
  `GMX_SIDECAR_LOG=<file>` in `browser.env` keeps the sidecar's log). The
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
  (`GMX_BROWSER_SWITCHES="enable-features=VaapiVideoDecoder"` without
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
godwinmix ctl source add game web+https://example.com/live-game
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

