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
| Still image (PNG, JPEG) | image | one frame | none | `file/source` | by hand: the source goes `stalled` after its one frame |
| Image sequence (`frame%04d.png`) | images | | | nothing | |
| IP camera MJPEG over HTTP | `multipart/x-mixed-replace` | | | nothing (`file/source` sits at `connecting`) | by hand |
| IP camera snapshot URL, polled | JPEG | | | nothing (one frame, then EOS) | by hand |
| ONVIF discovery of cameras on the LAN | | | | nothing | |
| Internet radio (Icecast, SHOUTcast) | MP3, AAC, Ogg over HTTP | none | MP3, AAC, Vorbis, Opus | `file/source`, finite rather than live | not tested |
| SDI or HDMI capture card (Blackmagic DeckLink) | raw | | | nothing | |

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
| RTMP | one TCP port for every channel, opened by the first RTMP channel (1935) | H.264; enhanced RTMP HEVC and AV1 are recognised and relayed as bytes | AAC | `crates/godwinmix/tests/channel_protocols.rs`, `plugins/ingest/src/rtmp` |
| RTMPS | a port a person chooses (443 offered) | as RTMP | as RTMP | `crates/godwinmix/tests/channel_protocols.rs` |
| SRT, one port for every channel by `streamid` | UDP, opened by the first SRT channel | H.264 only; HEVC is refused with a message | AAC only | `plugins/ingest/src/srt/tests.rs` |
| WHIP, on the control port | the control port, and UDP for media | H.264 only (the answer offers nothing else) | Opus, made AAC | `plugins/ingest/src/whip_in.rs` |
| RTMP, one publisher on its own port | `ingest/rtmp` | H.264 | AAC | `plugins/ingest/src/source_tests.rs` |

## Ways out

### From the programme (or a rendition of it)

| Transport | Container | Video | Audio | Kind | Tested |
|---|---|---|---|---|---|
| RTMP, RTMPS push | FLV | H.264 only (`flvmux`) | AAC | `rtmp/output` (built in) | `crates/godwinmix/tests/live.rs` |
| SRT caller or listener | MPEG-TS | H.264, HEVC, AV1 | AAC, Opus | `srt/output` (built in) | `plugin/outputs/srt.rs` |
| UDP or RTP, unicast or multicast, CBR | MPEG-TS | copy | copy | `udp/output` (plugin `udp`) | `plugins/udp/src/send/tests.rs`, frame hashes compared at an ffmpeg receiver |
| HLS, LL-HLS, DASH, served from the control port | CMAF | H.264, HEVC, AV1, one rung or a ladder | AAC, Opus | `hls/output` (built in) | `crates/godwinmix/tests/hls.rs`, `hls/tests.rs` |
| WHIP push | WebRTC | H.264 copy | Opus, from the programme's AAC | `whip/output` (plugin `whip`) | settings tests only |
| NDI | NDI | raw | raw | `ndi/output` (plugin `ndi`) | needs the NDI runtime |
| File | MP4 (fragmented), MKV | copy | copy | `record/output` (built in), `file-record/output` (plugin) | `crates/godwinmix-core/tests/recording*.rs`, `plugins/file-record/tests/records.rs` |
| WHEP playback, served from the control port (`/whep/<output>`) | WebRTC | copy of the programme or a rendition: H.264, and H.265, AV1, VP8, VP9 from a rendition | Opus, encoded once for every viewer | `whep/output` (built in) | `whep/tests.rs` against a real `webrtcbin` receiver; by hand with `whepsrc` and headless Chrome against the release core |
| RTSP server, for decoders, NVRs and players that pull, on a port chosen when added | RTP over UDP or TCP | H.264, H.265 copy | AAC, MP3, Opus copy | `rtsp/output` (plugin `rtsp`) | `plugins/rtsp/src/tests.rs`: ffmpeg over TCP and UDP, frame hashes equal to the encoder's; by hand against the release core |
| RIST (Simple Profile) | RTP MPEG-TS | copy (H.264, HEVC, AV1) | copy | `rist/output` (built in) | `plugin/outputs/rist_tests.rs`: a real `ristsrc` receiver decodes 60 frames and its RTCP marks the output connected |
| Icecast or SHOUTcast (audio only) | MP3, Ogg | | | nothing | |
| SDI out | | | | nothing | |

### Preview and monitoring doors (control port, only while a client holds them)

| Route | What | Tested |
|---|---|---|
| `GET /mjpeg/{program, source}` | MJPEG picture | `crates/godwinmix/tests/preview.rs` |
| `GET /pcm/{target}`, `/opus/{target}` | WebSocket audio | `crates/godwinmix/tests/preview.rs` |
| local preview socket | raw frames over `unixfd` | `preview/local.rs` |

### From a channel, with no show at all (plugin `ingest`)

| Transport | Video | Audio | How | Tested |
|---|---|---|---|---|
| RTMP, RTMPS | copy, whatever came in (enhanced RTMP bytes included) | copy | `restream/rtmp_out.rs` | `plugins/ingest/src/restream/tests.rs` |
| SRT | H.264 copy only (`flvdemux` into `mpegtsmux`) | AAC copy | `restream/srt_out.rs` | `plugins/ingest/src/restream/tests.rs` |
| RTMP or SRT with a rendition | H.264 encode only; the input must be H.264 | AAC | `transcode/` | `plugins/ingest/src/transcode/tests.rs` |

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
