# Headend mode

Status: plan and work in progress on `feat/headend`.

## Why

A headend takes dozens of programmes off MPTS multicast, converts each one
on a GPU and serves it as HLS, all day, with nobody watching the machine.
Most of that is already GodwinMix. A show with compositing off is a direct
show: one input, no compositor, no programme encode, no process of its own,
a row in the ingest plugin's direct host. It already takes multicast
MPEG-TS with a program number, converts through the planner and the
governor, and serves HLS.

So the headend is a mode of GodwinMix, not a second product. A mixer user
never sees any of it. A headend user turns mixing off and gets a channel
server, and the same station can still mix a show beside its channels.

Many headends run Flussonic today. Headend mode reads Flussonic's config and
answers Flussonic's URLs and API, so a site can move to GodwinMix without
changing the gateways, caches, players and monitoring that sit around it.

## The job it has to do

The reference case is a box with eight MPTS multicast groups carrying 51
programmes off air: MPEG-2 video at 1920x1080i 29.97, 1280x720p 59.94 and
704x480i 29.97, and AC-3 sound with two to four tracks per programme, some
of them 5.1. Each programme comes out as H.264 1280x720 fitted with black
bars, 1500 kbit/s, a keyframe every 50 frames, encoded on an RTX 3060 class
card, with every sound track as AAC 128 kbit/s. One further feed is RTMP
H.264 and AAC, passed through as it comes. A Flussonic config for that
looks like this:

```
http 8090;
edit_auth <user> <password>;
stream CH117 {
  disabled false;
  input mpts-udp://eth1@239.10.0.1:2001?program=7;
  transcoder deviceid=0 gop=50 hw=nvenc deinterlace=false deinterlace_rate=frame vb=1500k vcodec=h264 size=1280x720:fit:#000000 ab=128k;
}
stream RELAY1 {
  input rtmp://127.0.0.1:1935/live/relay1;
}
```

What usually reads a Flussonic server, and so what must keep working
unchanged:

| Reader | What it uses |
|---|---|
| A gateway in front that signs paths | `GET /<name>/index.m3u8`, `/<name>/<x>.m3u8` and `/<name>/tracks-<x>/<anything>` on the `http` port, no auth, every URI in a playlist relative |
| A cache downstream | the same paths; it keeps segments by path for minutes, so a segment name is never reused, not even across a restart |
| A monitoring page | `GET /streamer/api/v3/streams` with Basic auth from `edit_auth`, reading `name`, `title`, `inputs[].url`, `transcoder`, `stats.status`, `stats.bitrate`, `stats.input_bitrate`, `stats.lifetime`, `stats.media_info.tracks[]` (`content`, `codec`, `width`, `height`, `fps`, `bitrate`, `channels`, `language`, `profile`, `level`, `last_gop`, `bframes`, `sample_rate`), `stats.inputs[].media_info.tracks[]` with `pid`, `stats.inputs[].stop_events[].code` (`scrambled`), `stats.input.error_rate`, `stats.input.num_sec_no_data`, `stats.transcoder.restarts`; and Flussonic's log for scrambled inputs and refused encoder sessions |
| Scripts | the `http` port in the config file |
| Web players | `hls.audioTracks`, so every sound track must be an `EXT-X-MEDIA` rendition |

## What changes in GodwinMix

Six pieces, each on its own branch off `feat/headend`, merged back here.

### 1. Carriage: MPEG-2 video and every sound track

The hub carries tags with one video and one audio. Two additions, both
invisible to every existing reader:

* `MediaTag.track: u8`. 0 is the one stream every reader has always had.
  Sound tracks after the first are 1, 2, 3 in PMT order. Every existing
  reader keeps reading only track 0; a reader that wants the others asks
  (`Hub::subscribe_all`, and `tracks=all` on the loopback relay). So a copy
  to RTMP, SRT or UDP is byte for byte what it was.
* MPEG-2 video as a tag: an enhanced body with the FourCC `mp2v` (ours,
  never sent on a wire; a copy output that meets it says it cannot carry
  MPEG-2 and needs a rendition), one picture per tag, keyframe on a picture
  that carries a sequence header, caps read off the frame.
* The input carries every sound track, with its language from the PMT, and
  MPEG-2 video, when the show has a rendition that converts them.
* `direct.input` gains `tracks`: `{track, kind, codec, channels,
  sample_rate, lang, pid}` per carried track.

### 2. A native MPTS input

One socket and one receiver thread per multicast group, shared by every
programme taken from it, so a group with nine programmes is joined and
parsed once rather than nine times. The demuxer, PES assembly and framing
into tags are Rust; nothing from GStreamer sits between the socket and the
hub. Other transports keep their current inputs.

### 3. Conversion on the GPU

* Planner and catalogue: MPEG-2, H.264 and HEVC decode on NVDEC, scale and
  pad in CUDA memory, encode on NVENC, with frames never leaving the GPU
  between decode and encode. CPU fallbacks for every step.
* `VideoWant` gains `fit` (pad with a colour, crop, stretch),
  `deinterlace` (off, on, auto), `keyframe_frames`, `preset` and
  `bframes`. `AudioWant` gains `tracks` (first or all).
* Governor: the NVENC session probe goes past 8 when the driver allows it,
  up to a configured limit, with a manual override.
* On the page these are the Custom format's Advanced group, and the two
  NVENC settings are under Advanced in Mixer settings.
* Every encoded frame keeps its source frame's time, so sound and picture
  stay together whatever the encoder's load.
* One CUDA context per process, shared by every decoder and encoder, and
  decoder surfaces sized for what the codec needs.

### 4. HLS for every player

* Every sound track as an `EXT-X-MEDIA` rendition with `NAME` and
  `LANGUAGE`, the first one `DEFAULT=YES`.
* MPEG-TS segments beside CMAF: `segments = "cmaf" | "ts" | "both"`, cut on
  the same keyframes with the same sequence numbers. TS from GodwinMix's
  own muxer.
* `access = "keyed" | "open"`. Open is for an output behind a gateway that
  checks its own signature, and answers only on loopback and LAN addresses
  unless told otherwise.
* On the page, an output's HLS settings: low latency as one click, the
  rest under Advanced. `show.list` reads each direct output's params back.

### 5. Flussonic compatibility (`godwinmix-flussonic`)

* The config language: a lossless parser and writer and a typed view.
  Unknown directives are kept on save and listed in the import report.
* Import: each `stream` becomes a direct show with one HLS output; a
  `transcoder` line becomes that output's rendition.
  `godwinmix headend --flussonic <file>` runs it and watches the file.
* The facade on the config's `http` port: `/<name>/index.m3u8`,
  `video.m3u8`, `mono.m3u8`, `tracks-v1a1/mono.m3u8`, `tracks-a<n>/mono.m3u8`,
  TS segments named by their UTC start so a name is never reused,
  `preview.jpg`, `embed.html`, `mpegts`.
* `/streamer/api/v3/streams` with the fields in the table above, Basic auth
  from `edit_auth`; `PUT` and `DELETE` edit the config through the lossless
  writer.
* Log lines in Flussonic's format when asked.
* `headend.import` makes a config's streams into shows once, from the page
  or an agent, with the report first; `headend.status` says which file a
  station follows.

### 6. The headend wall

The monitoring wall for a box with fifty channels and one GPU: a live
picture per channel, state, rates, continuity errors, scrambled, black and
silence, sound levels per track; a GPU strip (encoder, decoder, memory,
sessions, temperature, power); input groups with their rates and errors; a
channel's detail with its tracks and a player.

Built as View > Headend (`ui/panels/headend/`), on the wall's data, its
pictures and its alarms; [Watch a headend of many channels](../how-to/watch-a-headend.md)
says how to read it. The GPU and the box come from two new methods,
`gpu.status` and `machine.status` ([the reference](../reference/machine-status.md)),
read from NVML or `nvidia-smi` once a second while a client asks. A
channel's sound level is the silence check's peak, now in `show.stats` as
`audio_peak_db`. The view reads `input.tracks` and `input.scrambled` from
`show.stats` when the carriage work sends them, and until then shows the
one video and one sound track a station carries today.

## How it is checked

* Every existing test passes unchanged; the copy path is byte for byte what
  it was.
* On a real headend, beside the server it replaces: every programme live,
  every sound track present, a monitoring page written for Flussonic's API
  reading this one unchanged, ten minutes with no dropped GOPs, sound and
  picture within one frame of each other with every channel converting at
  once.
* Measured against the server it replaces on the same machine: CPU,
  resident memory, video memory, encoder and decoder use, threads.
