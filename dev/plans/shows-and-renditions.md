# Shows, channels and renditions: the plan

Status: proposed, 2026-09-30. Nothing here is built yet unless it says so.
This is the design for three things that belong together: several
independent programmes on one machine (shows), every way a feed can arrive
under one idea (channels), and converting any format to any number of other
formats without wasting a single cycle (renditions). It is written to be
argued with. Change it here before the code changes.

## What we are protecting

These are the reasons someone picks this over OBS, vMix or Wirecast, and every
decision below is judged against them.

1. **It runs on a CPU.** No dedicated GPU needed, a five year old laptop or a
   Raspberry Pi is a real target, and a GPU when present is a bonus the
   software uses by itself. Any feature that only works well with a GPU is
   designed so it degrades, never refuses.
2. **It is fast, and we can prove it.** Benchmarks are published and repeated
   on every release (see "Benchmarks" below). A change that makes a number
   worse needs a reason in its commit.
3. **The programme never stops.** One show failing, one encoder stalling, one
   destination dying costs that one thing and nothing else.
4. **It is a desktop app and a web app at once.** The same page runs inside
   the Tauri shell on a laptop and in any browser against a headless server.
   Nothing in this plan may work in one and not the other.
5. **Few open ports.** By default one TCP port for the page and the API, and
   one more per ingest protocol only when a channel actually uses it. Many
   feeds share a port; a second listener is something a person asks for, not
   something we hand out.
6. **Click through.** Every part of this is done from the page. No file to
   edit, no command to run.

## Vocabulary

Three words, each meaning one thing.

* **Channel**: a place a feed arrives. Today it is an RTMP app on the shared
  RTMP port. It widens to every ingest protocol: a channel called
  `sunday-service` can take RTMP at `rtmp://host:1935/sunday-service/main`,
  SRT at `srt://host:9000?streamid=sunday-service/main`, WebRTC (WHIP) at
  `https://host:8080/whip/sunday-service/main`, a pulled RTSP camera, a UDP or
  multicast MPEG-TS feed, an HLS pull. One channel, one set of keys, many
  protocols. A channel's live streams are what shows use as sources.
* **Show**: one independent programme, with its own scenes, its own sources,
  its own programme encode and its own outputs. What one GodwinMix process is
  today. A machine can run several: the Sunday service, a 24/7 loop, a second
  room.
* **Rendition**: one concrete encoded form of a picture and its sound, such as
  "H.264 1080p30 6 Mbit/s AAC 160k" or "HEVC 720p". Outputs ask for
  renditions; the planner decides how few encoders produce all of them.

Word we drop: "programme" stays as the name of a show's on air picture, as it
is on every broadcast desk. The Programme panel keeps its name.

## Architecture

```
                         one process: the station
  ┌──────────────────────────────────────────────────────────────────────┐
  │  control port (HTTP, WebSocket, the page, WHIP and WHEP, HLS files)  │
  │  ingest hub: RTMP port, SRT port, WHIP; channels, keys, streams      │
  │  supervisor: starts, watches and restarts shows                      │
  │  resource governor: one budget for CPU, GPU sessions, memory         │
  └──────────────┬──────────────────────────┬────────────────────────────┘
                 │ shared memory or unix     │
          ┌──────▼──────┐             ┌──────▼──────┐
          │  show A     │             │  show B     │   one process each
          │  scenes     │             │  scenes     │
          │  programme  │             │  programme  │
          │  renditions │             │  renditions │
          └──────┬──────┘             └──────┬──────┘
                 ▼                            ▼
          outputs (RTMP, SRT, HLS, UDP, record, WHIP...)
```

**Why a show is its own process.** Rule 3. A show that leaks, deadlocks or
crashes is restarted by the supervisor while every other show stays on air.
The same code that runs one show today runs each show tomorrow, so every
panel, every scene method and Studio mode work unchanged inside a show. The
plugin host already works this way for sources (one sidecar per source), and
the node bridge already places work on other machines, so a show can later
live on a second box with no new concept.

**What the station owns.** The one control port, the page, the ingest hub
(so all shows share one RTMP port and one SRT port), the supervisor, and the
resource governor. It does no media work of its own beyond what the hub does
today (receiving and fanning out bytes).

**How a show reads a channel stream.** Through the hub, as a mixer source
does now: no decode in the station, bounded queues, a slow show loses GOPs
and never slows a publisher. Two shows using the same camera read the same
bytes; each decodes once for itself.

**Single show mode stays the default.** Someone who opens the desktop app
gets one show and never sees the word. A second show appears when they ask
for it. Nothing about a one show setup gets heavier: the supervisor and the
governor cost nothing measurable when there is one show.

**Migration.** Today's single process becomes "a station with one show" with
no config change: the existing config file is that show's config, and a
station section is added only when a second show exists.

## Channels across protocols, on few ports

| Protocol | In today | Port model | Plan |
|---|---|---|---|
| RTMP, RTMPS | yes, `ingest` channels | one TCP port (1935) for every channel | keep; add RTMPS on the same port by sniffing TLS, or 443 only when asked |
| SRT | yes, `srt/source` listener per source | one UDP port per listener | one UDP port for all channels, routed by `streamid` (`<channel>/<stream>`), keys as SRT passphrase or in the streamid |
| WebRTC (WHIP in, WHEP out) | yes, `ingest/whip` on its own port 8889 | TCP 8889 plus UDP | move WHIP and WHEP onto the control port's HTTP; media on one UDP port with ICE mux |
| RTSP | yes, `rtsp/source` (pull) | none, we dial out | keep as a channel "pull" stream; add discovery of ONVIF cameras on the LAN |
| UDP MPEG-TS, unicast and multicast | `rtp/source` covers RTP only | one UDP port per feed, by nature | add `udp/source` for raw MPEG-TS, multicast groups joined by address, so many multicast feeds need no new ports at all |
| HLS | yes, `hls/source` (pull) | none | keep as a pull |
| Files (MP4, MOV, MKV, TS) | yes, `file/source` | none | keep |
| NDI | yes, plugin | NDI's own | keep |

Outputs, from a show or straight from a channel:

| Protocol | Out today | Plan |
|---|---|---|
| RTMP, RTMPS | yes | keep |
| SRT (caller and listener) | yes | listener outputs share the station's SRT port by streamid |
| WHIP (push), WHEP (serve) | WHIP yes | WHEP from any rendition, on the control port |
| HLS and LL-HLS, DASH | no | new: served from the control port's HTTP, no new port; segments in memory with a small disk spill; ABR ladders as multi variant playlists |
| UDP MPEG-TS, unicast and multicast | no | new `udp/output`, with TTL and interface choice for multicast |
| RTSP server | no | later, only if asked: it needs its own port |
| Record (MP4 fragmented, MKV, TS) | yes | keep; any rendition can be recorded |
| NDI | yes | keep |

**Ports by default.** The control port (8080) and nothing else. The RTMP
port opens when the first RTMP channel is made; the SRT port when the first
SRT channel or listener output is made; the WebRTC media port when the first
WHIP or WHEP is used. The page shows which ports are open and why, in one
place, with a switch to close each.

## Renditions: the planner

Each output declares what it needs, as a rendition request:
container, video codec, width, height, frame rate, bitrate or quality,
keyframe interval, audio codec, channels, sample rate, bitrate. Anything left
out means "whatever the source has". A preset (YouTube 1080p, Facebook 720p,
"ABR ladder, 4 steps") fills the fields.

The planner turns every request in a show (and every channel destination)
into the smallest graph that satisfies all of them, in this order:

1. **Copy when nothing changes.** If the source already has the codec,
   size, rate and a bitrate within the output's range, the output is a remux.
   No decode, no encode. This is what channel destinations do today, at about
   1% of one core.
2. **Decode once per source.** However many renditions use it.
3. **Scale and convert once per distinct size and rate.** 1080p to 720p is
   done once and shared by everything that wants 720p.
4. **Encode once per distinct rendition.** Five outputs that want the same
   H.264 720p 3 Mbit/s share one encoder and get five copies of its bytes.
5. **Align keyframes across a ladder.** Every rendition of one source uses
   the same keyframe times, so an ABR player switches cleanly and HLS
   segments line up.
6. **Audio is planned the same way**, and far cheaper: one AAC encode shared
   by every output that wants AAC at that rate.

The plan is recomputed when an output is added, removed or changed, and the
change is applied without touching encoders that did not change: adding a
480p rung to a running ladder starts one encoder and leaves the others alone.
Removing the last output that used an encoder stops it.

**Any format to any format.** Input is whatever GStreamer can decode (it can
decode nearly everything used in broadcast: H.264, HEVC, AV1, VP8, VP9,
MPEG-2, ProRes, DNxHD, AAC, MP3, Opus, AC-3, PCM). Output codecs are what the
codec catalogue (`codecs.toml`) can encode on this machine: H.264, HEVC and AV1
in hardware where present, H.264 and HEVC (x264, x265) and AV1 (SVT-AV1) in
software, VP8 and VP9 for WebRTC, AAC and Opus for audio. The page offers only
what this machine can actually do and says why the rest is missing.

**One to many and ABR.** Both are just several rendition requests on one
source. An ABR ladder is a named group of renditions with an HLS or DASH
output reading all of them.

## The resource governor

One budget per station, shared by every show and every channel. It answers
one question before anything starts: can this machine do it without dropping
a frame of what is already on air?

1. **Measure, do not guess.** A short calibration on first run (and on demand)
   times a one second encode of each rendition shape on each encoder this
   machine has. `gmx bench` already has the harness for this. Results are
   stored per machine and refreshed when the hardware changes.
2. **Hardware first, within its limits.** A GPU encoder is preferred when it
   has room. The governor counts sessions (NVIDIA consumer cards cap them),
   memory and measured load per device, and moves to the next best encoder,
   down to software, when one is full.
3. **Software that fits.** On CPU only, it picks the x264 preset from the
   calibration that keeps the machine under the ceiling, rather than a fixed
   `veryfast`. A Pi gets `ultrafast` at 720p and is told so; a desktop CPU
   gets a better preset for the same load.
4. **A ceiling, and a refusal that helps.** The default ceiling is 75% of CPU
   and 85% of each GPU encoder's capacity, settable in Mixer settings. A
   request that would cross it is not started. The person is told what it
   would cost and offered what fits: "a 1080p60 HEVC rendition needs about
   180% of this CPU. 1080p30 H.264 fits, or 720p60 HEVC." Nothing silently
   drops frames because one more output was added.
5. **Live wins.** When the machine runs short while on air (a thermal
   throttle, another app), the governor sheds in a stated order: preview and
   thumbnails first (they are already on demand), then the lowest ABR rung,
   then lowers software presets, and never the programme encode or the
   highest rung of a live output. Every shed is an alert that says what was
   dropped and why.
6. **Copy is free and never counted against the ceiling** beyond network.
   The governor also watches outbound bandwidth, because a fan out of copies
   is limited by the uplink, not the CPU.

## Performance: the rules and the benchmarks

Rules every piece of this follows:

* No copy of a video frame between stages in the same process: GStreamer
  buffers are shared, and between processes frames go through shared memory
  (`shmsink` and `shmsrc`, or `unixfd` on Linux and macOS as the core already
  uses), never through a socket as raw bytes.
* Nothing blocks a streaming thread or the bus handler. A slow consumer is
  behind a bounded queue that drops whole GOPs, as the hub does.
* Hardware decode where the machine has it, for the same reason as encode.
* Colour conversion and scaling in one element per shape, and on the GPU
  (`glcolorconvert`, VA, VideoToolbox) when a GPU path exists end to end;
  otherwise in software with SIMD.
* Every new element on a hot path gets a number in the benchmark suite before
  it merges.

Benchmarks, run by `gmx bench` on every release, on three machines: a CPU only
laptop (the reference), a Raspberry Pi 5, and a desktop with a GPU. Each
result is published with the machine, the version and OBS Studio's number for
the same job where OBS can do that job:

| Job | Measure |
|---|---|
| Idle, one show, nobody watching | CPU and memory |
| One camera to one RTMP output, 1080p30 | CPU, memory, glass to glass latency |
| One source, three platforms, copy | CPU, memory |
| One source, ABR ladder of four, HLS out | CPU per rung, keyframe alignment |
| Eight channels, sixteen destinations, copy | CPU, dropped GOPs |
| Four shows, each 1080p30 out | CPU, isolation: kill one, the others drop nothing |
| Transcode HEVC 4K in to H.264 1080p and 720p out | real time factor on CPU and on GPU |
| Take and transition under load | frames late at the output |

A benchmark regressing by more than 5% fails the release check.

## Desktop and web

* The desktop shell starts the station, which starts the first show. Closing
  the window stops them all unless "keep running in the background" is on,
  which it is not by default.
* Everything is reachable in the page, so a headless server and the desktop
  app are the same product. The desktop app adds only what a browser cannot:
  starting the station, local file pickers, the menu bar.
* The show switcher sits in the top bar: the current show's name, a menu of
  the others with their on air state, and "New show". With one show it shows
  just the name.

## Modules

Each is its own crate or module with a small public surface, so any of them
can be replaced, tested alone, or left out of a build.

| Module | Where | Public surface |
|---|---|---|
| Channels (all ingest protocols) | `plugins/ingest`, widened | channel.*, the hub's `subscribe` |
| Rendition planner | new crate `godwinmix-render` | `plan(requests, sources, caps) -> Graph`, pure, no GStreamer, fully unit testable |
| Graph builder | `godwinmix-core`, new module | builds and edits GStreamer elements from a `Graph` diff |
| Resource governor | new crate `godwinmix-govern` | `admit(cost) -> Admit`, `calibrate()`, `shed()` |
| Show supervisor | `godwinmix`, new module on the existing plugin supervisor | show.* methods |
| New transports | plugins: `udp`, `hls-out`, SRT and WHIP widened | the plugin protocol, unchanged |

The planner is deliberately pure: it takes what is asked and what the machine
can do and returns a graph, so its decisions (copy or encode, which encoder,
what shares with what) are tested in milliseconds without a camera or a GPU.

## Phases

Each phase ships something a person can use and has its own benchmarks.

1. **Renditions in one show.** Rendition requests on outputs, the pure
   planner, copy when possible, shared encoders, the governor with
   calibration and the ceiling. Existing outputs keep working with no change.
2. **ABR and HLS out.** Keyframe aligned ladders, HLS and LL-HLS served from
   the control port, WHEP from any rendition.
3. **Channels on every protocol.** SRT on one port by streamid, WHIP on the
   control port, UDP and multicast MPEG-TS in and out, ONVIF discovery for
   RTSP cameras, the ports panel.
4. **Channel transcoding.** A channel destination may ask for a rendition,
   planned by the same planner, so a feed can be converted and fanned out
   without a show at all.
5. **Shows.** The supervisor, shared memory transport from the hub, the show
   switcher, per show config, migration from a single process.
6. **Benchmarks published.** The suite above on three machines, compared with
   OBS, in the docs and on the release page.

Phases 1 and 3 can run in parallel; 2 and 4 need 1; 5 needs 3 for shared
ingest.

## Questions to settle before phase 1

1. RTMPS on 1935 by sniffing TLS, or only on 443 when asked?
2. The default CPU ceiling: 75% leaves room for the desktop app's own page on
   the same machine; a headless server could use 90%. One default, or one per
   install kind?
3. Does a channel destination (no show involved) count against the same
   budget as shows? The plan says yes: one machine, one budget.
4. Show isolation costs one decode per show per shared camera. Acceptable, or
   should two shows on one machine share decoded frames through shared
   memory? Sharing saves CPU and couples the shows.
