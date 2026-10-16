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
Players that autoplay start on their own (the browser is told no gesture is
needed); a player that waits for a click shows its poster until somebody
clicks it from [Interact](#driving-a-page-by-hand). A YouTube `/embed/`
address works too: the renderer adds `autoplay=1` when the address says
nothing about autoplay, and sends the navigation with the referrer
`https://mix.godwin.desktop/`, the app identifier YouTube asks an embedding
app for. Without a referrer YouTube refuses the embed with "Video player
configuration error, Error 153".

The renderer's user agent is Chrome's, with `GodwinMix/<version>` on the end:

```
Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/154.0.0.0 Safari/537.36 GodwinMix/0.2.0
```

The platform part is the one Chrome sends on each system. It used to be
Chromium's default with `Chrome/150...` replaced by `GodwinMix/...`, and sites
that look for a browser they know took that for an unknown one. A page that
wants to know it is being captured can still look for `GodwinMix/`.

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
`godwinmix-browser` on Linux, `godwinmix-browser.app` on macOS), else where a
package puts it, else where a source checkout's own build leaves it
(`browser/target/release`), else on `PATH`. A mixer run from a checkout with
none of these builds it the first time a web page is added, and the page
starts by itself when it is ready; [setup.md](setup.md) lists every place and
every step. With no renderer and nothing to build it from, `web+` falls back
to GStreamer's `wpesrc`, described below. `[browser]` also takes extra `args`
and `env` for the sidecar.

On macOS the renderer runs Chromium with a mock keychain. Its profile is
private to one run, and the login keychain would otherwise raise a dialog
nobody can answer from a headless process, which holds every http and https
load until it is answered.

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

#### Codecs

Everything Chromium does: WebAudio, WebGL, canvas, HTML5 video
in VP8, VP9, AV1 and Opus. H.264 and AAC depend on which CEF binary sits
next to the sidecar. The official CEF binaries the crate downloads are built
without them, for patent licensing reasons, and a `<video>` with an H.264
file then reports `MEDIA_ERR_SRC_NOT_SUPPORTED` (checked with
`browser/test/video-mp4.html`). So GodwinMix builds CEF itself, with `proprietary_codecs=true
ffmpeg_branding=Chrome`, and the sidecar is built against that.

**Where the build comes from.** The scripts in
[psmux/godwinmix-cef](https://github.com/psmux/godwinmix-cef) run CEF's own
automated build pinned to the exact commit the `cef` crate binds, and publish
one release per CEF version, `cef-154.0.34` today, with the minimal archives
named and laid out like the official ones, their `.sha1` and `.sha256`, and
an `index.json` in the official format. The crate's own downloader takes
that release through `CEF_DOWNLOAD_URL` with no other change.
`browser/Cargo.toml` pins both `cef` and `cef-dll-sys` to `=154.5.0`, which
is CEF 154.0.34: the sys crate is the one that downloads and checks the
distribution, and with only `cef` pinned, cargo once picked a newer sys crate
and quietly fetched a newer CEF than the one built.
[Build the web page engine](../how-to/build-the-web-page-engine.md) says how
to make the build for a new version.

| Platform | CEF in the released app | H.264 and AAC |
|---|---|---|
| Windows x86_64 (`windows64`) | ours | yes |
| Linux x86_64 (`linux64`) | ours | yes |
| Linux arm64 (`linuxarm64`) | ours | yes |
| macOS, Apple silicon and Intel | official | no: no Mac has built ours yet |

`dev/bundle-browser.sh`, which the release workflow runs, fetches ours for
the platforms in `CEF_CODECS_PLATFORMS` in `dev/cef-codecs.sh` and fails if
one of them is missing from the release; for macOS it builds against the
official CEF and says so. A mixer run from a checkout asks our release
first and the official CDN second.

**Measured.** On Windows on 2026-10-16, with the sidecar built against our
`windows64` archive of CEF 150.0.10, the version pinned before 154.0.34: `MediaSource.isTypeSupported` answered true for
`avc1.4d401f`, `avc1.64002a` and `mp4a.40.2` as well as VP9, AV1 and Opus,
where the official build answers false for the first three. The Al Jazeera
English YouTube live stream, which shows "Your browser can't play this
video" with the official build, played on programme in a real core: no
`page.notice`, the picture changing from snapshot to snapshot, and the
source's meter at a median of -15 dBFS (peak -9.6) over 15 seconds, where
the official build left programme audio at silence.
**Against Google Chrome.** `browser/test/codec-matrix.html` asks
`MediaSource.isTypeSupported` and `canPlayType` for each type below, then
plays a real HEVC MP4 and a real AV1 MP4. Measured on Windows on 2026-10-16
(Intel Core Ultra 9 285H with its integrated GPU) with our `windows64` build
of CEF 150.0.10,
once as the mixer runs the sidecar (software: `disable-gpu`,
`disable-gpu-compositing`, SwiftShader) and once with the sidecar's `--gpu`
switch, against Google Chrome 155 on the same machine. Each cell is MSE,
then `canPlayType`.

| Type | Ours, as the mixer runs it | Ours with `--gpu` | Chrome 155 |
|---|---|---|---|
| `video/mp4; codecs="avc1.42E01E"` | yes, probably | yes, probably | yes, probably |
| `video/mp4; codecs="avc1.4d401f"` | yes, probably | yes, probably | yes, probably |
| `video/mp4; codecs="avc1.64002a"` | yes, probably | yes, probably | yes, probably |
| `video/mp4; codecs="hvc1.1.6.L93.B0"` | no, no | yes, probably | yes, probably |
| `video/mp4; codecs="hev1.1.6.L93.B0"` | no, no | yes, probably | yes, probably |
| `video/webm; codecs="vp8"` | yes, probably | yes, probably | yes, probably |
| `video/webm; codecs="vp09.00.10.08"` | yes, probably | yes, probably | yes, probably |
| `video/mp4; codecs="vp09.00.10.08"` | yes, probably | yes, probably | yes, probably |
| `video/mp4; codecs="vp09.02.10.10.01.09.16.09.01"` | yes, probably | yes, probably | yes, probably |
| `video/mp4; codecs="av01.0.05M.08"` | yes, probably | yes, probably | yes, probably |
| `video/mp4; codecs="av01.0.13M.10"` | yes, probably | yes, probably | yes, probably |
| `video/mp4; codecs="av02.0.05M.08"` | no, no | no, no | no, no |
| `audio/mp4; codecs="mp4a.40.2"` | yes, probably | yes, probably | yes, probably |
| `audio/mp4; codecs="mp4a.40.5"` | yes, probably | yes, probably | yes, probably |
| `audio/mp4; codecs="mp4a.69"` | no, probably | no, probably | no, probably |
| `audio/mp4; codecs="mp4a.6B"` | no, probably | no, probably | no, probably |
| `audio/mp4; codecs="mp3"` | no, probably | no, probably | no, probably |
| `audio/mpeg; codecs="mp3"` | no, probably | no, probably | no, probably |
| `audio/webm; codecs="opus"` | yes, probably | yes, probably | yes, probably |
| `audio/mp4; codecs="opus"` | yes, probably | yes, probably | yes, probably |
| `audio/webm; codecs="vorbis"` | yes, probably | yes, probably | yes, probably |
| `audio/mp4; codecs="flac"` | yes, probably | yes, probably | yes, probably |
| `audio/flac; codecs="flac"` | no, no | no, no | no, no |
| `audio/mpeg` | yes, probably | yes, probably | yes, probably |
| `audio/aac` | yes, probably | yes, probably | yes, probably |
| `audio/mp4; codecs="ac-3"` | no, no | no, no | no, no |
| `audio/mp4; codecs="ec-3"` | no, no | no, no | no, no |
| Play `hevc.mp4` (1280x720) | fails, `MEDIA_ERR_SRC_NOT_SUPPORTED` | plays | plays |
| Play `av1.mp4` (1280x720) | plays | plays | plays |

With `--gpu` our build answered exactly as Chrome on every row. The one
difference the mixer sees today is HEVC: Chromium has no software HEVC
decoder and plays it only through the GPU's, so with the GPU switched off
HEVC is not offered at all. No codec string exists for AV2 yet; the one
tried is refused everywhere. MP3 is accepted by MSE only as plain
`audio/mpeg` and by `canPlayType` in every form, which is how Chrome answers
too. AC-3 and E-AC-3 are not in
Chrome either.

CPU for a 1080p video on programme: the YouTube embed of Big Buck Bunny,
which YouTube served as AV1 1920x1080 at 60 fps with Opus, in a 1920x1080
core, measured over 30 s after 45 s of play, two runs each. The renderer's
processes used 95 and 123 percent of one core in software and 112 and 101
percent with `--gpu`; the core itself used 24 percent either way. On this
machine hardware decoding did not lower the renderer's CPU beyond run to run
noise, as far as these runs show; the frames still come back to the CPU to
reach the mixer.

**Hardware decoding, `params.gpu`.** A web page source takes one param of
its own, `gpu`, false unless set. True starts that source's renderer with
`--gpu`, so Chromium may use the GPU, which is the only way it plays HEVC.
Set it when adding the source (`{"uri": "web+https://...", "gpu": true}` to
`source.add`) or later (`{"id": "<source>", "params": {"gpu": true}}` to
`source.set`); the web page form calls it Hardware video decoding. The
source's status carries `gpu`, so a client shows what it has. Changing it
builds that one source again: its renderer restarts and reloads the page,
and nothing else in the mixer is touched. A source on programme comes back
on programme by itself; measured on Windows on 2026-10-16 with an HEVC clip,
the programme kept running (the core answered `core.status` 268 times in
75 s, never more than a second apart) and showed the slate for under a
second while the renderer restarted. With `gpu` off, a page whose video
fails to play while HEVC is not offered says so in `page.notice`, naming
`params.gpu`. On a machine whose GPU cannot decode HEVC the switch changes
nothing for HEVC, and the notice says that instead. It is off by default
because it bought no CPU on the 1080p run above, and a Raspberry Pi or a
laptop with no usable GPU gains nothing from it.

LINUX_RESULT

**Before our build, for the record.** With the official CEF on Windows on
2026-10-15, `avc1.4d401f` and `mp4a.40.2` were not supported; two YouTube
recordings played with sound, because YouTube offers VP9 or AV1 with Opus to
a browser that asks for them, and a news live stream whose formats list held
VP9 video and only `audio/mp4` sound showed the error. That is still what
the macOS app does.

**Containers, for a Mac that needs the codecs today.** Arch Linux's `cef`
package is also built with proprietary codecs. `browser/dev/Dockerfile.arch`
and `browser/dev/linux-arch-codecs.sh` build against it, and
`Dockerfile.arch-runtime` plus `sidecar-docker.sh` package the result as a
container the mixer runs through Docker (the wrapper forwards the stop
signal; the container is gone 0.9 s after `source remove`;
`GMX_SIDECAR_LOG=<file>` in `browser.env` keeps the sidecar's log).
`browser/dev/Dockerfile.runtime` builds the same kind of container from any
Linux minimal distribution, ours included. The wrapper runs the container
with `--log-driver none`, and that is not optional: Docker's default log
driver copies everything a container writes to stdout into a JSON file on
disk, so each sidecar's 41 MB/s of raw video was also being written to the
VM's disk. It filled 30 GB in minutes, the players inside stalled on the
full disk, and the symptom on air was a frozen picture with silent audio.
Give the Linux VM CPU: on this laptop a 4 vCPU VM starved the sidecar under
a 720p30 H.264 page (irregular frames, 21 audio dropouts in 15 s); with 8
vCPUs the same page went to air clean, six of six flashes and beeps, audio
-28 ms, no dropouts. The sidecar logs every dropout as `audio re-anchored`,
so a starved box shows itself.

**Why not Karere's Linux builds.** Karere publishes CEF 150.0.10 for Linux
with the same codec flags, and an earlier version of this page used them.
They carry Karere's own patch to Chromium's GLib message pump, which lives
in a GPL 3.0 repository with no licence of its own, so they are not used.

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

### Driving a page by hand

An operator can use a page as they would in a browser: the **Interact** view
in the Sources panel ([how to](../how-to/use-a-web-page-by-hand.md)), or these
two methods from any client. Both need the `operate` scope, and both refuse a
source that is not a page drawn by the renderer, naming its type.

| Method | REST | MCP tool | What it does |
|---|---|---|---|
| `source.page.input` | `POST /api/v1/sources/{id}/page/input` | `use_web_page` | Pointer, wheel and key events, oldest first, at most 64 a call |
| `source.page.navigate` | `POST /api/v1/sources/{id}/page/navigate` | `navigate_web_page` | `go` to a `url`, `back`, `forward` or `reload` |

`source.page.input` takes `{"id", "events": [...]}`. Each event has a `type`:

| `type` | Fields | |
|---|---|---|
| `move` | `x`, `y` | The pointer moved. With a button held it is a drag |
| `down`, `up` | `x`, `y`, `button` (`left`, `middle`, `right`; default left), `count` (2 for a double click; default 1) | A button pressed or let go |
| `wheel` | `x`, `y`, `dx`, `dy` | A scroll in CSS pixels; a positive `dy` scrolls down, as a browser's `wheel` event has it |
| `leave` | | The pointer left the picture |
| `key` | `code`, `key` | A key pressed and let go. `code` is the browser's `keyCode` (13 Enter, 8 Backspace, 37 to 40 the arrows); `key` is the character it typed, when it typed one |
| `text` | `text` | Text typed into the focused field as one piece, up to 2000 characters: what a phone keyboard or an input method hands over |

`x` and `y` are shares of the page, 0.0 at the left or top and 1.0 at the
right or bottom, measured on the picture and not on any border around it. A
point outside that range is refused with the rule. `move`, `down`, `up` and
`key` also take `shift`, `ctrl`, `alt` and `meta`, true when held.

The answer is `{"id", "sent", "dropped"}`. Nothing waits on the page: the
events go into a queue of 256 that a thread of the source's own writes to the
renderer, and an event that finds the queue full is dropped and counted. A
call while the source's renderer is restarting is refused with "try again".

`source.page.navigate` takes `{"id", "action", "url"}`. `action` is `go`
(the default), `back`, `forward` or `reload`. For `go`, `url` is an http or
https address, and a bare host such as `example.com` is taken as https; any
other scheme (`file:`, `javascript:`, `data:`) is refused, since an address
typed into a shared desk is otherwise a way into the mixer's machine. The new
address is kept as the source's own: the runtime store and a saved project
open it. The answer is `{"id", "action", "url"}`, with the address the source
keeps.

Every page source drawn by the renderer carries `page` in its status:

```json
"page": {"url": "https://www.youtube.com/watch?v=aqz-KE-bpKQ", "can_back": true,
         "can_forward": false, "loading": false}
```

`url` follows the page, links and redirects included, and is what the source
is saved with. `notice`, when present, says in a sentence why the page cannot
play what it is showing; today that is only the codecs below, and it starts
with `codecs:`. It goes away when the page changes. The status is not pushed
when only `page` changes, so a client showing it while somebody uses the page
asks `source.get` again (the Interact view does once a second while open).

Underneath, the renderer runs with `--control` and reads one JSON line per
event on its stdin, exactly as the protocol serialises them, plus
`{"type":"go","url":...}`, `{"type":"back"}`, `{"type":"forward"}` and
`{"type":"reload"}`. It writes `[browser] page {json}` on stderr when the
address, the loading state or the notice changes. Nothing is written to the
renderer until somebody touches the page; the mixer holds the pipe and starts
the writing thread on the first event.

### Transparent pages

`params.transparent = true` draws a page over the picture with its alpha
kept: an overlay somebody else hosts, an OGraf graphic (the scene sets it for
those), a page of your own with a transparent background.

```sh
gmx ctl source add alerts https://example.com/overlay --web --param transparent=true
```

The renderer runs such a page in graphic mode (`--graphic`). Nothing is paced
and nothing is muxed: a frame leaves only when Chromium painted one, and only
the box around what is not fully transparent, already in AYUV, so a page that
holds still sends nothing at all. The mixer reads those boxes into a layer
the overlay board blends after the compositor, as it does a text or a
transparent PNG. The page has no sound in this mode, and `superimpose` does
not apply.

Measured on a Core Ultra 9 285H laptop at 1080p30, the renderer's whole
process tree: a blank transparent page sent as a stream took 125 percent of
one core, the same page in graphic mode about 2 to 25 percent (most of it
Chromium's own idle work, which varied with the machine's load). A lower
third that moves all the time costs what its moving part costs; the numbers
are in [graphics for agents](graphics-for-agents.md#what-it-costs).

HTML templates (`html:<name>`) use the same mode, with the network cut off
(`--offline`), their fields and their way in and out told on stdin; see
[graphics for agents](graphics-for-agents.md).

**WebGL without a GPU.** The renderer is started with `--disable-gpu` and
`--enable-unsafe-swiftshader`, so WebGL is drawn on the CPU: by WARP (the
Microsoft Basic Render Driver) on Windows, checked; by SwiftShader on Linux
and macOS, not checked here. `--gpu` on the renderer's command line
(`[browser] args = ["--gpu"]`) lets Chromium use the GPU instead.

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

