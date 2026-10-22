# Headend mode: GodwinMix in place of Flussonic

Status: plan and work in progress on `feat/headend`. Started 2026-10-23.

## Why

The ISP box in Holmes's building ran Flussonic for the BTN36 TV channels:
51 programmes off eight ATSC multicast groups (224.2.2.2, ports 2001 to
2008), each decoded on the RTX 3060, scaled to 1280x720, encoded with NVENC
at 1500 kbit/s and served as HLS, plus one RTMP feed (FAITHIQ) passed
through as it came. On 2026-10-22 at 08:04 UTC Flussonic's licence server
reported the licence deleted, Flussonic stopped every stream, and the
channels went off air.

GodwinMix already does most of that job. A show with compositing off is a
direct show: one input, no compositor, no programme encode, no process of
its own, a row in the ingest plugin's direct host. It already takes
multicast MPEG-TS with a program number, converts through the planner and
the governor, and serves HLS. So the headend is a mode of GodwinMix, not a
second product. A mixer user never sees any of it; a headend user turns
mixing off and gets a channel server.

What it does not do yet is what this branch adds.

## What Flussonic did there, exactly

From `/etc/flussonic/flussonic.conf` on the box (a scrubbed copy is in
`crates/godwinmix-flussonic/tests/fixtures/btn36.conf`):

```
http 8090;
edit_auth <user> <password>;
stream CH117 {
  disabled false;
  input mpts-udp://eno1@224.2.2.2:2001?program=7;
  transcoder deviceid=0 gop=50 hw=nvenc deinterlace=false deinterlace_rate=frame vb=1500k vcodec=h264 size=1280x720:fit:#000000 ab=128k;
}
stream FAITHIQ {
  input rtmp://127.0.0.1:1935/live/program;
}
```

The sources, probed on 2026-10-23: MPEG-2 video at 1920x1080i 29.97,
1280x720p 59.94 and 704x480i 29.97, and AC-3 sound, two to four tracks per
programme, some 5.1.

What reads Flussonic today, and so what must keep working unchanged:

| Reader | What it uses |
|---|---|
| `btn36-gateway` nginx on the box (127.0.0.1:8096) | `GET /<name>/index.m3u8`, `/<name>/<x>.m3u8` and `/<name>/tracks-<x>/<anything>` on port 8090, no auth |
| CloudSouth's cache (tv.btn36.com) | the same paths through the gateway; segments are cached ten minutes by path, so a segment name must never be reused, not even across a restart |
| `btn36-dashboard` (the channel wall, :8095) | `GET /streamer/api/v3/streams` with Basic auth from `edit_auth`, reading `name`, `title`, `inputs[].url`, `transcoder`, `stats.status`, `stats.bitrate`, `stats.input_bitrate`, `stats.lifetime`, `stats.media_info.tracks[]` (`content`, `codec`, `width`, `height`, `fps`, `bitrate`, `channels`, `language`, `profile`, `level`, `last_gop`, `bframes`, `sample_rate`), `stats.inputs[].media_info.tracks[]` with `pid`, `stats.inputs[].stop_events[].code` (`scrambled`), `stats.input.error_rate`, `stats.input.num_sec_no_data`, `stats.transcoder.restarts`; and the log `/var/log/flussonic/flussonic.log` for scrambled and GPU session errors |
| `btn36-gateway-sync` | the `http` port in `flussonic.conf` |
| watch.btn36.com's player | `hls.audioTracks`, so every sound track must be an `EXT-X-MEDIA` rendition |

## What changes in GodwinMix

Five pieces, each on its own branch off `feat/headend`, merged back here.

### 1. Carriage: MPEG-2 video and every sound track (`wip/headend-carriage`)

The hub carries tags with one video and one audio. Two additions, both
invisible to every existing reader:

* `MediaTag.track: u8`. 0 is the one stream every reader has always had.
  Sound tracks after the first are 1, 2, 3 in PMT order. Every existing
  reader keeps reading only track 0; a reader that wants the others asks
  (`Wanted`/hub reader option `all_tracks`). So a copy to RTMP, SRT or UDP
  is byte for byte what it was.
* MPEG-2 video as a tag: an enhanced body with the FourCC `mp2v` (ours, never
  sent on a wire; a copy output that meets it says it cannot carry MPEG-2
  and needs a rendition), packet type coded frames, one picture per tag as
  `mpegvideoparse` frames it, keyframe on a picture that carries a sequence
  header. No sequence start tag: like AC-3, each frame says what it is, and
  `transcode::caps::caps_for` reads the caps off it.
* The input carries every sound track, with its language from the PMT,
  and MPEG-2 video, when the show has a rendition that converts video.
  Without one, MPEG-2 stays left out as today, named in the error.
* `event/channel.stream` and `direct.input` gain `tracks`: one entry per
  carried track, `{track, kind, codec, channels, sample_rate, lang, pid}`.

### 2. Conversion on the GPU (`wip/headend-transcode`)

* Planner and catalogue: MPEG-2 decode (`nvmpeg2videodec` on NVIDIA,
  `avdec_mpeg2video` on the CPU), H.264 and HEVC decode on NVDEC, scale in
  CUDA memory (`cudaconvertscale`), encode on NVENC, with the frames never
  leaving the GPU between decode and encode.
* `VideoWant` gains `fit` (`pad` with a colour, `crop`, `stretch`; pad is
  `cudaconvertscale add-borders`), `deinterlace` (`off`, `on`, `auto`),
  `keyframe_frames` (Flussonic's `gop` is in frames), `preset` and
  `bframes`. `AudioWant` gains `tracks` (`first` or `all`): with `all`,
  each sound track gets its own decode and AAC encode at the same channel
  count, published as its own track number.
* Governor: a consumer NVIDIA card's session cap is a probed number. The
  box's driver is patched past it (see the box notes), so the probe must
  find the real limit rather than stop at 8: `[governor] nvenc_session_probe`
  up to 128, and a manual `nvenc_sessions` override.
* Cost: one CUDA context per process, shared by every decoder and encoder
  (GStreamer's `GstCudaContext` sharing), and decoder surfaces sized for
  MPEG-2 (no more than its references need), so 51 channels fit in far less
  than Flussonic's 11.5 GB of video memory.

### 3. HLS for every player (`wip/headend-hls`)

Done on the branch; `docs/reference/hls-output.md` has the surface.

* An HLS output carries every sound track as its own rung and its own
  `EXT-X-MEDIA` rendition, `NAME` and `LANGUAGE` from the PMT's ISO 639
  code (`English` and `en` for `eng`; the code when it is unknown; `Track N`
  when there is none), the first `DEFAULT=YES`. The packager reads every
  track off the relay with `tracks=all`; the languages come from
  `direct.input`'s `tracks`.
* MPEG-TS segments beside CMAF: `segments = "cmaf" | "ts" | "both"`. Made
  in Rust by a copy of the ingest plugin's `tsmux` (the plugin is a binary,
  so it cannot be linked), cut where `cmafmux` cut the CMAF picture and
  given its numbers. `main` carries the picture and the first sound in one
  stream, each further sound is a TS rendition of its own. A segment is
  named by its UTC start in ms, so a name is never reused, not across a
  restart either. The station's `ts_master`, `ts_media` and `ts_segment`
  hand the facade the same as data, without a viewer key, and
  `headend::ts_bridge` answers the facade's `Segments` from them.
* Keyed and open: `access = "keyed" | "open"`. Open answers players on
  loopback and private networks with no key or token, for an output behind
  a gateway that checks a signature of its own; anyone else still needs the
  key unless `open_public = true`. CMAF, LL-HLS, DASH and TS alike.
* Measured on one 720p output: TS beside CMAF costs about 0.1% of one core
  and 9 MiB (`docs/explanation/hls-output.md`).

### 4. Flussonic compatibility (`wip/headend-flussonic`)

New crate `godwinmix-flussonic`:

* The config language: a lossless parser and writer, and a typed view
  (port of `~/workspace/GodwinStream/crates/gs-config`, which has tests
  against the real file). Unknown directives are kept on save and listed
  in the import report.
* Import: each `stream` becomes a direct show with its input and one HLS
  output; a `transcoder` line becomes that output's rendition (`vb`, `size`
  with `fit`, `gop`, `deinterlace`, `ab`, `hw`, `deviceid`, `preset`,
  `bframes`). `godwinmix headend --flussonic /etc/flussonic/flussonic.conf`
  runs it, and watches the file.
* The facade, on the config's `http` port: `/<name>/index.m3u8` (master),
  `/<name>/video.m3u8`, `/<name>/mono.m3u8`, `/<name>/tracks-v1a1/mono.m3u8`
  (video with the first sound), `/<name>/tracks-a<n>/mono.m3u8` (each other
  sound), TS segments as `/<name>/tracks-<x>/YYYY/MM/DD/HH/MM/SS-<ms>.ts` in
  UTC so a name is never reused, `/<name>/preview.jpg`, `/<name>/embed.html`,
  `/<name>/mpegts`; the CMAF and LL-HLS of the same output stay at
  GodwinMix's own `/hls/...` paths.
* `/streamer/api/v3/streams` and `/streamer/api/v3/streams/<name>` with the
  fields in the table above, Basic auth with `edit_auth`; `PUT` and `DELETE`
  edit the config through the lossless writer, so Flussonic could read the
  file again.
* Log lines in Flussonic's format, to its log path when asked, for the
  channel wall's scrambled and GPU counts.

### 5. The headend wall (`wip/headend-wall`)

The monitoring wall, for a box with fifty channels and one GPU: a live
picture per channel, state, input and output rates, continuity errors,
scrambled, black and silence, sound levels per track; a GPU strip with
encoder, decoder and memory use, sessions, temperature and power (NVML);
the eight multicast groups with their rates and errors; a channel's detail
with its tracks and a player. Built on `ui/panels/wall`, in the existing
vanilla JS style, light and dark.

## How it is checked

* Every existing test passes unchanged on every branch; the copy path is
  byte for byte what it was.
* On the ISP box, on a port beside Flussonic's, before anything moves:
  all 51 programmes live from the real multicast, every sound track
  present, the channel wall reading the API without a change, a player on
  watch.btn36.com playing through the gateway and CloudSouth, ten minutes
  with no dropped GOPs.
* Measured against Flussonic's own numbers from the same box (11.5 of 12 GB
  video memory, NVENC 89 to 96%): CPU, resident memory, video memory,
  encoder and decoder use, threads.
* Then, with the user's go ahead: Flussonic off, GodwinMix on port 8090.
  Flussonic stays installed and its config untouched, so going back is
  `systemctl stop godwinmix-headend; systemctl start flussonic`.
