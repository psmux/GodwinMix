# Reference: every way in and every way out

One page for the question "can GodwinMix take this, and can it send that".
Each row says which kind does it, what it carries, where it is tested, and
what is missing. It was written from the code and from running the elements
on a Mac with GStreamer 1.28.7, not from the other pages, and it is kept in
step with the code by hand: a kind that lands without a row here is a bug in
this page.

Words used below:

* A kind is a `type` id such as `file/source` or `udp/output`. "Built in"
  means it is compiled into the core; anything else is a plugin under
  `plugins/`.
* "Copy" means the encoded bytes pass through untouched: no decode, no
  encode.
* "Any GStreamer decodes" means the core opens it with `decodebin`, so the
  codec list is the decode table at the bottom of this page.

## Ways in

### Pulled from somewhere, or read from a file

| Transport | Container | Video | Audio | Kind | Tested |
|---|---|---|---|---|---|
| File on disk or over HTTP | MP4, MOV, MKV, WebM, TS, MXF, AVI, FLV, Ogg, WAV, MP3, FLAC, M4A | any GStreamer decodes | any GStreamer decodes | `file/source` (built in) | `plugin/harness.rs` conformance; the decode table below by hand |
| RTMP, RTMPS pull | FLV | H.264 only | AAC only | `rtmp/source` (built in) | `crates/godwinmix/tests/live.rs` |
| HLS pull | TS or fMP4 segments | any | any | `hls/source` (built in, `uridecodebin`) | URI routing only (`plugin/source.rs`); no media test |
| DASH pull | fMP4 | any | any | `hls/source` (built in) | URI routing only |
| RTSP, RTSPS pull (a camera, an NVR) | RTP | any | any | `hls/source` (built in) | `plugins/rtsp/src/tests.rs` decodes an RTSP stream through the same `uridecodebin` |
| SRT caller, listener, rendezvous | MPEG-TS | any | any | `srt/source` (plugin `srt`) | `plugins/srt/src/source.rs` |
| UDP or RTP, unicast or multicast, one program of a multiplex | MPEG-TS | any | any | `udp/source` (plugin `udp`) | `plugins/udp/src/recv/tests.rs` |
| WebRTC pull (WHEP client) | RTP | VP8, VP9, H.264, AV1 | Opus | `whip/whep` (plugin `whip`) | settings tests only; no media test |
| NDI | NDI | NDI | NDI | `ndi/source` (plugin `ndi`) | needs the NDI runtime, not on this machine |
| A command's stdout | any container | any | any | `exec/source` (built in) | `plugin/harness.rs` |
| A web page | rendered | rendered | page audio | `browser/source`, `layered/source` (built in) | the sidecar's own tests under `browser/test` |
| Test pattern and tone | raw | bars, ball, black, snow | sine | `test/source` (built in) | throughout |
| RIST (Simple Profile), listening | RTP MPEG-TS | any | any | `hls/source` (built in) claims `rist://` as live | `plugin/outputs/rist_tests.rs`: `uridecodebin` on `rist://` decodes what `rist/output` sends |
| Still picture (PNG, JPEG, BMP, WebP, TIFF), file or HTTP | image | held live with `imagefreeze` | silence | `image/source` (built in) | `plugin/kinds/image_tests.rs`: the conformance harness every kind passes |
| Picture sequence (`frame%04d.png`) | images | played at `params.fps`, looping | silence | `image/source` (built in) | `plugin/kinds/image_tests.rs`: the conformance harness |
| IP camera MJPEG over HTTP or HTTPS, with its login | `multipart/x-mixed-replace` | MJPEG, copy | none | `ipcam/source` (plugin `ipcam`) | `plugins/ipcam/src/tests.rs` against a camera served by the test; by hand through the release core |
| IP camera snapshot URL, polled 1 to 30 times a second | JPEG | MJPEG, copy | none | `ipcam/source` (plugin `ipcam`) | `plugins/ipcam/src/tests.rs`, behind a basic login |
| ONVIF discovery of cameras on the LAN | WS-Discovery, SOAP | each profile's RTSP stream, opened by `hls/source` | as the camera sends | `ipcam/discover` (plugin `ipcam`) | `plugins/ipcam/src/onvif/tests.rs` against a simulated device checking the WS-Security digest; no real camera here |
| Internet radio (Icecast, SHOUTcast), or any audio stream over HTTP | MP3, AAC, Ogg, with ICY titles | none | MP3, AAC, Vorbis, Opus, copy | `icecast/source` (plugin `icecast`), live, the song title in health | `plugins/icecast/src/tests.rs` against a station run by the test |
| SDI or HDMI capture card (Blackmagic DeckLink) | SDI, HDMI | raw, the mode detected | embedded PCM | `decklink/source`, `decklink/devices` (plugin `decklink`), behind detection | `plugins/decklink/src/tests.rs`: the pipeline parsed and the no card path. Not tested on a card: none on this machine |

### Devices on this machine

| Device | Kind | Tested |
|---|---|---|
| USB or built in camera | `camera/source` (plugin `camera`) | `plugins/camera/tests` |
| A screen or a window | `screen/source` (plugin `screen`) | `plugins/screen/tests` |
| A microphone, line input or sound card | `audio-device/source` (plugin `audio-device`) | `plugins/audio-device/tests` |

### Pushed to the mixer (channels, plugin `ingest`)

A channel takes one stream name under one set of keys on every protocol it
has switched on. The hub carries what FLV carries.

| Transport | Port | Video | Audio | Tested |
|---|---|---|---|---|
| RTMP | one TCP port for every channel, opened by the first RTMP channel (1935) | H.264, and enhanced RTMP HEVC with its size read; AV1 relayed as bytes | AAC | `crates/godwinmix/tests/channel_protocols.rs`; `plugins/ingest/src/device_tests.rs` takes an ffmpeg HEVC publisher |
| RTMPS | a port a person chooses (443 offered) | as RTMP | as RTMP | `crates/godwinmix/tests/channel_protocols.rs` |
| SRT, one port for every channel by `streamid` | UDP, opened by the first SRT channel | H.264, and HEVC carried on as enhanced RTMP | AAC | `plugins/ingest/src/srt/tests.rs`, an HEVC caller included |
| WHIP, on the control port | the control port, and UDP for media | H.264 only (the answer offers nothing else) | Opus, made AAC | `plugins/ingest/src/whip_in.rs` |
| RTMP, one publisher on its own port | `ingest/rtmp` | H.264 | AAC | `plugins/ingest/src/source_tests.rs` |

### Into a direct show, with no compositor (plugin `ingest`)

A show with compositing off takes one input and copies it to its outputs.
Nothing is decoded: each input is demuxed and parsed into the hub's tags,
which carry AC-3, E-AC-3 and MPEG layer II as enhanced RTMP v2 audio bodies
besides what classic FLV carries. One video and one audio stream are taken
from a feed. `docs/reference/direct-inputs.md` has the params and the
numbers each one reports. Every input hands its streams to the same
parsers, so the codec columns say what those parsers take; the Tested column
says what each transport was tested with, and where it does not name a
codec, the test sent H.264 and AAC.

| Transport | Container | Video | Audio | How | Tested |
|---|---|---|---|---|---|
| UDP, unicast or multicast on a named interface, source specific where the OS allows, one program of a multiplex chosen by number, programs named from the SDT | MPEG-TS | H.264, HEVC copy | AAC, AC-3, E-AC-3, MP2, MP3 copy | `direct/input/ts_in.rs`, the udp plugin's probe counting continuity errors | `direct/input/tests/ts.rs`: multicast on `lo0`, two programs with names, 2% of datagrams dropped and counted; ffprobe decodes what came out |
| RTP wrapped TS (SMPTE 2022-2) | MPEG-TS | as UDP | as UDP | `ts_in.rs`, RTP gaps counted | `tests/ts.rs`: HEVC and 5.1 AC-3 from ffmpeg, both decoded back by ffprobe |
| SRT caller and listener | MPEG-TS | as UDP | as UDP | `ts_in.rs`, SRT's own losses read from `srtsrc` | `tests/ts.rs`: a caller dialling the input, and the input dialling a listener |
| RIST (Simple Profile), listening | RTP MPEG-TS | as UDP | as UDP | `direct/input/rist.rs` | `tests/ts.rs` against `ristsink` |
| RTSP pull over TCP or UDP | RTP | H.264, HEVC | AAC, AC-3, MP2 | `direct/input/rtsp.rs`, RTP losses read from the session | `tests/pull.rs` against a `gst-rtsp-server` camera, over TCP and UDP |
| HLS and DASH pull, paced to the clock | TS or fMP4 | H.264, HEVC | AAC, AC-3 | `direct/input/pull.rs` | `tests/pull.rs` against ffmpeg's HLS and DASH served over HTTP |
| RTMP, RTMPS pull from another server | FLV | H.264 | AAC, MP3 | `pull.rs` | `tests/pull.rs` against ffmpeg as the server (`-listen 1`) |
| A file, looped at its own pace | TS, MP4 | H.264, HEVC | AAC, AC-3, MP2 | `pull.rs` | `tests/files.rs`: a 2 s clip played for 5 s, time running on across the loop |
| A channel's stream (`channel:<app>/<stream>`) | hub tags | as the channel | as the channel | `direct/input/channel.rs` | `tests/files.rs`, across two publishers |

E-AC-3 is read and framed the same way as AC-3, but only its header parsing
is tested; no test sends an E-AC-3 stream through an input. MPEG-2 video
goes nowhere: no output of a direct show could carry it.

## Ways out

### From the programme (or a rendition of it)

| Transport | Container | Video | Audio | Kind | Tested |
|---|---|---|---|---|---|
| RTMP, RTMPS push | FLV, enhanced RTMP for HEVC | H.264 (`flvmux`), HEVC from a rendition (`eflvmux`); AV1 refused with the way out | AAC | `rtmp/output` (built in) | `crates/godwinmix/tests/live.rs`; `plugin/outputs/flv_tests.rs` sends HEVC to ffmpeg as the server |
| SRT caller or listener | MPEG-TS | H.264, HEVC, AV1 | AAC, Opus | `srt/output` (built in) | `plugin/outputs/srt.rs` |
| UDP or RTP, unicast or multicast, CBR | MPEG-TS | copy | copy | `udp/output` (plugin `udp`) | `plugins/udp/src/send/tests.rs`, frame hashes compared at an ffmpeg receiver |
| HLS, LL-HLS, DASH, served from the control port | CMAF | H.264, HEVC, AV1, one rung or a ladder | AAC, Opus | `hls/output` (built in) | `crates/godwinmix/tests/hls.rs`, `hls/tests.rs` |
| WHIP push | WebRTC | H.264 copy | Opus, from the programme's AAC | `whip/output` (plugin `whip`) | settings tests only |
| NDI | NDI | raw | raw | `ndi/output` (plugin `ndi`) | needs the NDI runtime |
| File | MP4 (fragmented), MKV | copy | copy | `record/output` (built in), `file-record/output` (plugin) | `crates/godwinmix-core/tests/recording*.rs`, `plugins/file-record/tests/records.rs` |
| WHEP playback, served from the control port (`/whep/<output>`) | WebRTC | copy of the programme or a rendition: H.264, and H.265, AV1, VP8, VP9 from a rendition | Opus, encoded once for every viewer | `whep/output` (built in) | `whep/tests.rs` against a real `webrtcbin` receiver; by hand with `whepsrc` and headless Chrome against the release core |
| RTSP server, for decoders, NVRs and players that pull, on a port chosen when added | RTP over UDP or TCP | H.264, H.265 copy | AAC, MP3, Opus copy | `rtsp/output` (plugin `rtsp`) | `plugins/rtsp/src/tests.rs`: ffmpeg over TCP and UDP, frame hashes equal to the encoder's; by hand against the release core |
| RIST (Simple Profile) | RTP MPEG-TS | copy (H.264, HEVC, AV1) | copy | `rist/output` (built in) | `plugin/outputs/rist_tests.rs`: a real `ristsrc` receiver decodes 60 frames and its RTCP marks the output connected |
| Icecast 2 or SHOUTcast 2 (sound only) | MP3, Ogg | none | MP3, Vorbis or Opus, encoded once | `icecast/output` (plugin `icecast`) | `plugins/icecast/src/tests.rs`: an Icecast server run by the test checks the login and decodes 3 s or more of what it got |
| SDI out | | | | nothing | |

### Preview and monitoring doors (control port, only while a client holds them)

| Route | What | Tested |
|---|---|---|
| `GET /mjpeg/{program, source}` | MJPEG picture | `crates/godwinmix/tests/preview.rs` |
| `GET /pcm/{target}`, `/opus/{target}` | WebSocket audio | `crates/godwinmix/tests/preview.rs` |
| local preview socket | raw frames over `unixfd` | `preview/local.rs` |

### From a channel or a direct show, with no compositor (plugin `ingest`)

A channel destination and a direct show's output take the same addresses.
Everything but RTMP is MPEG-TS from the plugin's own muxer, `tsmux/`
(`docs/reference/direct-shows.md` has the stream types).

| Transport | Video | Audio | How | Tested |
|---|---|---|---|---|
| RTMP, RTMPS | copy, whatever came in (enhanced RTMP bytes included) | AAC copy; AC-3, E-AC-3 and MPEG audio are not sent, the picture goes alone | `restream/rtmp_out.rs` | `plugins/ingest/src/restream/tests.rs` |
| SRT, caller or listener | H.264 or HEVC copy; AV1 not yet (MPEG-TS has no mapping the muxer writes) | AAC, AC-3, E-AC-3, MP2, MP3 copy | `restream/srt_out.rs` over `tsmux/` | `direct/tests_carriage.rs`: an `srtsrc` listener's recording decodes |
| UDP, unicast or multicast, `ttl` and `interface` | H.264 or HEVC copy | as SRT | `restream/udp_out.rs` over `tsmux/` | `direct/tests.rs`: decoded from the socket; 50 multicast outputs measured in `docs/explanation/direct-host.md` |
| RTP, payload type 33 | as UDP | as SRT | `restream/udp_out.rs` | `direct/tests_carriage.rs`: sequence numbers unbroken, the payload decodes |
| RIST (Simple Profile), sending | as UDP | as SRT | `restream/rist_out.rs`, `ristsink` | `direct/tests_carriage.rs` against `ristsrc` |
| A file, MPEG-TS | as UDP | as SRT | `restream/file_out.rs` | `direct/tests.rs`: a 3 s recording decodes; `tsmux/tests.rs`: AC-3 and MP2 decode |
| SRT to a player that calls in (`m=request`), on the channel's own SRT port | H.264 or HEVC copy | AAC copy | `srt/play.rs` | `plugins/ingest/src/srt/tests.rs`: `srtsrc` on the publisher's port decodes the stream |
| RTMP or SRT with a rendition | H.264 or HEVC in (AV1 in is decoded but its size is not read, so it is not planned yet); H.264, HEVC or AV1 out, HEVC and AV1 as enhanced RTMP | AAC | `transcode/` | `plugins/ingest/src/transcode/tests.rs` and `tests_hevc.rs`: HEVC to H.264 and H.264 to HEVC with real encoders |

## Decode table

Every file below was made with ffmpeg 9.0.1 and opened with `uridecodebin`,
the element `file/source` and `hls/source` use, on GStreamer 1.28.7 on an
Apple M series Mac. Two seconds at 25 fps is 50 frames.

| File | Video frames | Audio buffers |
|---|---|---|
| H.264, AAC, MP4 | 50 | 88 |
| HEVC, AAC, MP4 and TS | 50 | 88 |
| AV1, Opus, MKV and WebM | 50 | 101 |
| VP9, Opus, WebM | 50 | 101 |
| VP8, Opus, WebM | 50 | 101 |
| MPEG-2, MP2, TS | 50 | 77 |
| MPEG-2, AC-3, TS | 50 | 58 |
| H.264, E-AC-3, TS | 50 | 58 |
| ProRes 422, PCM, MOV | 50 | 50 |
| DNxHD, PCM, MXF | 50 | 50 |
| MJPEG, PCM, AVI | 50 | 87 |
| H.264, MP3, FLV | 50 | 78 |
| H.264, FLAC, MKV | 50 | 87 |
| MP3, WAV, FLAC, Opus, M4A (audio only) | none | 78 to 101 |
| PNG, JPEG | 1 | none |

## What this machine cannot do, and why

| What | Why |
|---|---|
| SDI capture tested on real hardware | there is no DeckLink card here. `decklinkvideosrc` is installed (the Blackmagic driver is not), so a kind can be built and its detection tested, but not a picture |
| AJA capture | `ajasrc` is not in this GStreamer build |
| NDI in either direction | the NDI runtime is not installed, and its licence forbids shipping it |
| AAC with `fdkaacenc` | not in this build; `avenc_aac` is used |
| AV1 over RTMP (enhanced FLV) | no GStreamer 1.28 muxer writes AV1 into FLV (`eflvmux` takes H.264 and HEVC only), and `flvdemux` cannot read it, so a channel refuses it and names SRT as the way out |
| AV1 in to a channel rendition | the decoder works, but its frame size is not read from the stream yet, so the planner cannot size a rendition |
| WebRTC (WHEP) with Homebrew's GStreamer alone | `nicesrc` and `nicesink` come from libnice's GStreamer plugin, which Homebrew's `gstreamer` does not install. Install `libnice-gstreamer`, or point `GST_PLUGIN_PATH` at a build of it |
| SDI out | no card here, and no output kind is written for it yet |
