# GodwinMix

[![build](https://github.com/psmux/GodwinMix/actions/workflows/build.yml/badge.svg)](https://github.com/psmux/GodwinMix/actions/workflows/build.yml)
[![quickstart](https://github.com/psmux/GodwinMix/actions/workflows/quickstart.yml/badge.svg)](https://github.com/psmux/GodwinMix/actions/workflows/quickstart.yml)

GodwinMix is a live video mixer. Several sources come in (cameras over RTMP,
files, HLS, SRT, a web page), one of them is on programme at a time, and the
programme goes out to one or more RTMP destinations without ever stopping, not
even while sources are added, removed, taken or dying.

It is one Rust binary on GStreamer with one HTTP port. The same binary is the
headless server, the command line client and the MCP server an AI agent talks
to, and the web UI it serves is a client of the same public API as everything
else.

## Start here

| If you want | Go to |
|---|---|
| A mixer on a server, nothing installed on the host | [Docker, headless](#docker-headless) |
| A window on your own machine | [the desktop app](#the-desktop-app) |
| To build it, change it, or read the code | [from source](#from-source) |

### Docker, headless

This puts a web page on air to an RTMP destination. It is timed in CI on every
push, on a runner with no GPU, and the measured number is printed in the
[quickstart job's summary](https://github.com/psmux/GodwinMix/actions/workflows/quickstart.yml).
The claim is under five minutes on a clean machine, and if it stops being true
that job fails.

```sh
git clone https://github.com/psmux/GodwinMix && cd GodwinMix

# The mixer, plus mediamtx as a local RTMP destination so this needs no
# stream key anywhere.
docker compose -f deploy/docker/docker-compose.yml up -d --build

export TOKEN=change-me   # what docker-compose.yml passes as GODWINMIX_TOKEN
api() { curl -sf -X "$1" "http://localhost:8080$2" -H "Authorization: Bearer $TOKEN" \
          -H 'content-type: application/json' ${3:+-d "$3"}; }

# Where the programme goes.
api POST /api/outputs '{"id":"primary","uri":"rtmp://rtmp:1935/live/program","policy":"own"}'

# A page as a source, then take it to programme once it is live (5 to 20s).
api POST /api/sources '{"id":"page","uri":"web+https://example.com/","name":"Page"}'
api GET  /api/status | jq '.sources[] | {id, state}'
api POST /api/take '{"source":"page"}'
```

Open <http://localhost:8080> for the UI and
<http://localhost:8888/live/program> to watch what went out. To send it
somewhere real, use your own RTMP address instead of the local one:

```sh
docker run -d --name godwinmix --shm-size 1g \
  -p 127.0.0.1:8080:8080 -e GODWINMIX_TOKEN="$TOKEN" \
  ghcr.io/psmux/godwinmix:latest
```

Then [deploy/README.md](deploy/README.md) for the token, a reverse proxy with
TLS, and the firewall, before that port is reachable from anywhere else.

### The desktop app

Download the installer for your platform from the
[latest release](https://github.com/psmux/GodwinMix/releases/latest): `.dmg` on
macOS, `.msi` on Windows, `.deb` on Debian and Ubuntu. Every tag builds all
three.

GStreamer is not bundled in the installer yet and has to be installed
separately (see [Platforms](#platforms)). Bundling a trimmed GStreamer inside
the app is planned, with a target of a 150 MB Windows installer; the stock
GStreamer runtime installer alone is 527 MB and should never be a user's
problem.

The app is a window onto the same web UI, local or remote, so there is no
second implementation to keep in step. It can point at a mixer on another
machine by changing the address.

### From source

```sh
brew install gstreamer          # or your distro's gstreamer + plugins base/good/bad/ugly/libav
cargo install --git https://github.com/psmux/GodwinMix godwinmix

godwinmix --probe                              # what codecs this machine will use
godwinmix --example-config > godwinmix.toml
godwinmix --config godwinmix.toml
```

`cargo install` leaves two binaries with the same code behind them: `godwinmix`,
the name a service unit and a package use, and `gmx`, the short one to type.
`gmx ctl status` and `godwinmix ctl status` are one command. A crates.io
release is planned; until then `--git` is the install.

Open <http://localhost:8080>. Click a cell to take that camera. Number keys 1
to 9 take directly, 0 or Escape cuts to black.

To work on it rather than run it, see [CONTRIBUTING.md](CONTRIBUTING.md) and
`dev/harness/up.sh`, which brings up an RTMP server, a synthetic camera and a
mixer in one command.

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

123 unit tests, including a regression for every bug found during that testing.
They build real GStreamer pipelines; there are no mocks.

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

## How it works

An RTMP output has to look like a single unbroken stream: monotonic timestamps,
no gaps, codec parameters that never change. So the output encoder is started
once and runs until the broadcast ends. Everything that changes during a
broadcast happens upstream of it, in raw video and raw audio, where switching
source is a property change on a compositor pad. The encoder cannot tell that
anything happened, so nothing downstream reconnects.

### The pipeline

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

## Platforms

Linux, macOS and Windows, the same code on all three. The mixer, its tests and
the desktop app are built on every push on all three by the GitHub Actions
workflow in `.github/workflows/build.yml`, and each run leaves the binaries
behind as artifacts. What differs by platform:

| | GStreamer | hardware codecs | browser sidecar | desktop app |
|---|---|---|---|---|
| Linux | distro packages | NVIDIA, VA | native, with H.264 from a prebuilt CEF ([codecs](docs/reference/web-page-sources.md)) | `.deb`, AppImage |
| macOS | `brew install gstreamer` | VideoToolbox | `.app` bundle from `browser/dev/mac-bundle.sh`, or the Linux one in a container | `.app` |
| Windows | the MSVC runtime and development MSIs from gstreamer.freedesktop.org, or `choco install gstreamer gstreamer-devel`; put `C:\gstreamer\1.0\msvc_x86_64\bin` on `PATH` | Media Foundation, NVIDIA | `godwinmix-browser.exe` next to the mixer, from `cd browser; cargo build --release` | `.msi`, NSIS |

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

## What it can mix

The protocol is worked out from the URL, so there is nothing to configure
beyond the address:

| URL | opened as |
|---|---|
| `rtmp://host/live/key`, `rtmps://…` | RTMP, demuxed explicitly so the client can be chosen |
| `https://host/stream.m3u8` | HLS |
| `https://host/manifest.mpd` | DASH |
| `rtsp://…`, `srt://…`, `udp://…`, `rtp://…` | continuous stream |
| `web+https://host/page` | the page rendered by a real browser, with its audio |
| `exec:<command line>` | whatever the process writes to stdout |
| a path, `file://…`, `https://host/clip.mp4` | file |

[docs/reference/sources.md](docs/reference/sources.md) is the detail: what
happens when a source stops delivering, how `exec:` sources work, and the
supervisor's restart and backoff rules.
[docs/reference/web-page-sources.md](docs/reference/web-page-sources.md) is the
browser, including `superimpose`, which hands a page's video to the mixer's own
hardware decoder and leaves the browser drawing only the page over it.

## Driving it

Everything the UI can do is one HTTP call, and `gmx ctl` is a thin client for
the same API so scripting it does not mean assembling JSON by hand.

```sh
gmx ctl status
gmx ctl take cam2                       # or: take   (with no id, cuts to black)
gmx ctl source add hls1 https://host/stream.m3u8 --name "Roof camera"
gmx ctl output add youtube rtmp://a.rtmp.youtube.com/live2/KEY --policy cdn
gmx ctl golive https://example.com/event/42 --rtmp rtmp://a.rtmp.youtube.com/live2/KEY
```

| | |
|---|---|
| `GET /api/status` | full snapshot |
| `POST /api/take` | `{"source": "cam1"}`, or `{"source": null}` for black |
| `POST /api/sources`, `DELETE /api/sources/{id}` | add and remove sources while live |
| `POST /api/outputs`, `DELETE /api/outputs/{id}` | add and remove destinations while live |
| `POST /api/adbreak` | roll a clip, immediately or on a given frame |
| `GET /api/agent/state` | the state a language model needs, compact |
| `GET /api/snapshot/sheet.jpg` | every source and the programme in one mosaic |
| `POST /api/golive` | page plus destination plus take, in one call |
| `GET /ws` | events and state, plus mosaic frames as binary messages |

The whole surface is in [docs/reference/http-api.md](docs/reference/http-api.md)
and every `ctl` subcommand is in [docs/reference/cli.md](docs/reference/cli.md).
A generated `protocol.md` and `openapi.json` are planned, from
`godwinmix --api-info`, so that reference is produced from the code rather than
typed twice.

## For developers

The point of this project is that other people can build on it, so the
commitments below are written down rather than implied.

**One executable, no interpreter.** The core, the CLI and the MCP server are
one binary. The only external dependency is the platform's GStreamer.

**One protocol, no private doors.** The web UI, `gmx ctl`, the MCP server and
the desktop app all use the public HTTP API. There is no faster internal path
that a third party cannot use, because a reference implementation that cheats
never grows an ecosystem.

**A compatibility promise.** `api_level` 1 is frozen for breaking changes.
Additions bump the level. `api_compatible` moves only at an announced major,
at most once a year, and the previous level is supported for a year after that.
That promise is here on the front page rather than in a changelog, because it is
the thing you are trusting when you write against this.

**Extension points, honestly labelled.** Today they are `exec:` sources (any
process that writes a container to stdout) and the browser sidecar protocol.
The full plugin model, where a plugin runs in process, beside the core or on
another machine without being rewritten, is being built: `gmx plugin new`,
`gmx plugin test` and `gmx plugin add` are the commands it lands as. Presets
(a scene layout and a set of defaults, shared as a file) and themes (the UI
restyled without forking it) are planned behind `gmx preset` and the UI's
theme directory. Nothing in this paragraph exists yet, and the pages under
[docs/](docs/) say so where they describe it.

Where to start reading: [CONTRIBUTING.md](CONTRIBUTING.md) for the build and
the house style, [docs/explanation/](docs/explanation/) for why the thing is
shaped the way it is, and `src/mixer.rs` for the pipeline everything else
exists to protect.

## For agents

An AI agent operates this mixer through the same API as everyone else, and
`godwinmix mcp` dresses that API as an MCP server over stdio:

```sh
claude mcp add godwinmix -- godwinmix mcp --url http://HOST:8080 --token TOKEN
```

`GET /api/agent/state` is the state cut down to what a director needs: what is
on programme, every source with its state, whether it has audio, how long since
its last frame, and a `motion` number from 0.0 to 1.0 for how much its picture
is changing. `GET /api/snapshot/sheet.jpg` is every source and the programme in
one labelled mosaic, so a model compares them in a single image rather than
paying for one image per source.

[docs/agents.md](docs/agents.md) is the playbook: the decision loop, the
timings, and the things an agent must not do. `examples/ai-director.py` is a
working director on the `anthropic` SDK.

## What this is not

Stated up front, because finding out later is worse.

* **No game capture on Windows.** Not in the first year. Capturing a fullscreen
  exclusive game is a hooking problem with a decade of OBS work behind it, and
  pretending otherwise would waste your afternoon. Desktop and window capture
  are planned; game capture is not.
* **A browser source with H.264 or AAC inside the page needs a codec enabled
  CEF build.** The official CEF binaries omit both. Linux has prebuilt ones;
  macOS and Windows do not, and the project is building and publishing one.
  Until it does, `superimpose` is the way round it on those platforms: the
  mixer decodes the page's video itself and the browser draws only the page.
* **No vertical and horizontal output at the same time.** One canvas, one
  encoder chain. A second canvas is a second compositor and a second encoder,
  and it is not in the first year.
* **No hosted service.** This is a program you run. There is no account.
* **No plugin marketplace yet.** The plugin protocol comes first. A directory
  that lists nothing is a trap the research on ecosystems named explicitly.

## Known limitations

Measured, not guessed. These are the things that will surprise you.

* **Superimpose cannot take over MSE or DRM playback**, and YouTube is MSE.
  Those pages fall back to full rendering, which is reported rather than
  failed. The page's own player UI freezes on pages it does apply to, because
  the browser's copy of each taken-over video is paused.
* **A superimposed source whose browser dies is rebuilt, not restarted.**
  Measured at 14 seconds from the browser being killed to the page back on air
  with sound; the programme shows the slate meanwhile. Every other kind of
  source restarts in place in about two seconds.
* **Page content in the key colour is treated as spill.** The page paints a
  near-pure magenta where each video sat and the sidecar removes it. A page
  element that is itself that magenta would go with it.
* **Reconnect takes about 2 seconds**, not milliseconds. Tearing down the
  output pipeline, swapping the proxy pair, rebuilding and completing a fresh
  RTMP handshake costs that much. A viewer with a normal buffer should not see
  it, and it has not been measured against a real CDN.
* **The mosaic carries no audio.** Programme audio meters are in the UI
  instead. WebRTC would fix it properly; `whepserversink` in GStreamer 1.28.6
  returned 405 on every method and path tried, so MJPEG is what ships.
* **Multiview cost scales with source count** on the decode side, not the
  encode side. Each source is decoded once and tee'd to a full resolution
  branch for programme and a small one for the mosaic.
* **An ad whose duration cannot be queried** (a live URI rather than a file)
  ends on end-of-stream instead, which truncates the tail. Files are fine.
* **A failed ad is reported over the event stream, not in the HTTP response.**
  `POST /api/adbreak` returns 202 as soon as the command is queued, so a
  missing file shows up as an alert in the UI rather than a 4xx. The programme
  is not disturbed either way.
* **Sources are assumed to be H.264 and AAC**, which is what RTMP carries in
  practice. Anything else is reported as a failed source rather than decoded.

**No footprint numbers are published yet.** CPU and memory on the reference
machines (a Raspberry Pi 4 and 5, an Intel N100, a laptop with no GPU, a
desktop with an NVIDIA card) have not been measured, so this README claims
none. `gmx bench` will print them per release with the command that produced
them. Until then, the honest answer to "will it run on my box" is to try it.

## Documentation

[docs/](docs/) is organised the way Diátaxis suggests, because "how do I" and
"why is it like this" are different questions and mixing them serves neither.

| | |
|---|---|
| [Tutorials](docs/tutorials/) | your first stream, with Docker or with the desktop app; your first plugin |
| [How to](docs/how-to/) | a headless server, a reverse proxy, a Raspberry Pi, a hardware encoder, ad breaks |
| [Reference](docs/reference/) | the HTTP API, the CLI, sources, web pages, every config key |
| [Explanation](docs/explanation/) | why the programme never stops, why plugins are processes, why the UI is a client |
| [docs/agents.md](docs/agents.md) | the playbook for an AI operator |
| [docs/friction-log.md](docs/friction-log.md) | every place someone got stuck, and what was done about it |

## Licence

Apache 2.0, in [LICENSE](LICENSE). Contributions are under the
[CLA](CLA.md) and the [code of conduct](CODE_OF_CONDUCT.md). Security reports
go to the address in [SECURITY.md](SECURITY.md), not to a public issue.

GodwinMix is not affiliated with, endorsed by or connected to the OBS Project.
OBS, OBS Studio, Open Broadcaster Software and the OBS Studio logo are
registered trademarks of Wizards of OBS LLC, and are used here only to describe
what this software reads.

Upgrading from LiveboxMix, which is what this was called until 0.2:
[docs/how-to/upgrade-from-liveboxmix.md](docs/how-to/upgrade-from-liveboxmix.md).
