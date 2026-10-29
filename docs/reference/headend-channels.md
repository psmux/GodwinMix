# Reference: headend channels, profiles and the scan

The headend channels a station keeps itself, with no Flussonic file behind
them: `headend.channel.*`, `headend.profile.*`, `headend.scan` and
`headend.export`. Every station answers them, in headend mode or not. The
headend screen uses these and nothing else. [Set up a headend in
GodwinMix](../how-to/set-up-a-headend.md) is the walk through.

## How a channel runs

A channel compiles to the plan a Flussonic import makes for a stream, and
the station runs that plan as a direct show:

* the show's id is made from the channel's name (`CH117` runs as `ch117`);
* its input is `input`, with `interface` moved into the input's `params`
  as a direct show takes it, and `backup` as its backup;
* its first output is the HLS output `flussonic` (`hls://flussonic`), with
  `hls` as its params and the profile as its rendition, with the channel's
  `video_overrides` over the profile's picture; a profile with a `ladder`
  makes it a ladder, each picture a variant of the master playlist;
* each further output is a push, `push-1`, `push-2` unless it has an id,
  with the same rendition (a ladder's first picture) unless `copy` is set.

So a channel set up here and the same stream imported from a Flussonic
config are the same show: the same input, the same outputs, the same
rendition, the same playback paths. Only a change of input, conversion or
outputs makes the show again; a title is a rename and `enabled` is a stop
or a start.

The station keeps the channels, the saved profiles and which streams of a
followed file it holds in `headend.json`, and which show each channel runs
as in `headend-shows.json`, both beside the list of shows. A channel that
could not be made (the governor had no room, the plugin was not up) is
tried again every 30 seconds.

## A channel

```json
{"name": "CH101", "title": "News", "enabled": true,
 "input": {"uri": "udp://@239.0.0.1:5000", "program": 1, "interface": "eth1",
           "backup": {"uri": "srt://192.0.2.10:9000", "params": {"passphrase": "the shared secret"}}},
 "profile": "h264-720p-1500",
 "hls": {"segments": "both", "access": "open", "segment_ms": 3400},
 "outputs": [{"uri": "udp://239.0.1.1:5000"}, {"id": "cdn", "uri": "rtmp://host/app/key", "copy": true}]}
```

| Field | Meaning |
|---|---|
| `name` | the public path, `/<name>/index.m3u8`: 1 to 64 letters, digits, `-`, `_` or `.`, starting with a letter or a digit |
| `title` | what the screen and the show list call it; left out, the name |
| `enabled` | false keeps the channel and stops its show; true when left out |
| `input.uri` | any address a direct show takes ([the inputs](direct-inputs.md)) |
| `input.program` | the MPEG-TS program to take; left out, the first |
| `input.interface` | the card a multicast group is joined on, by name or by one of its IPv4 addresses |
| `input.params` | further input params: `passphrase` and `latency_ms` for SRT, `transport` for RTSP, `stall_ms` and `return_ms` for the backup switch |
| `input.backup` | `{uri, program, interface, params}`, taken when the main input stops |
| `profile` | `copy`, a built in or saved profile's id, or `custom` |
| `custom` | with `profile` `custom`: the channel's own conversion, `{video, audio, ladder}` as a profile has them |
| `video_overrides` | picture settings of this channel alone over its profile's: each field set replaces the profile's on every picture of it, the rest come from the profile. For `aspect` when a broadcaster flags its picture wrong, `deinterlace`, `deinterlace_method`, `fit`. Left out, the profile as it is. A pass through profile has no picture for them to act on |
| `hls.segments` | `both` (MPEG-TS and CMAF), `ts` or `cmaf`; `both` when left out |
| `hls.access` | `open` (no viewer key, loopback and private addresses only) or `keyed`; `open` when left out |
| `hls.segment_ms` | the longest a segment is, 500 to 10000; 3400 when left out |
| `hls.window` | seconds of segments in the playlist; the packager's when left out |
| `outputs[].uri` | `udp://`, `srt://` or `rtmp://` |
| `outputs[].copy` | true sends the input as it comes, whatever the profile |
| `gpu` | the NVIDIA card its conversion runs on, a CUDA device index; left out, the card with the least work when it starts (`video.gpu = "auto"`), where it stays while it runs. `headend.channel.list` answers `gpus`, the cards there are, and each entry's `on_gpu`, the card it is on now |
| `origin` | set by the station: `added here`, `import of <source>`, or `<file>, edited here` |

## Profiles

A profile is `{id, name, video, audio, ladder}`. `video` and `audio` are a
rendition's wants ([renditions](renditions.md)), so every field there
works: `codec`, `width`, `height`, `fit` (`pad`, `crop`, `stretch`),
`pad_color`, `aspect`, `bitrate_kbps`, `keyframe_ms` or `keyframe_frames`,
`fps`, `deinterlace` (`off`, `on`, `auto`), `deinterlace_method`,
`match_source`, `preset`, `bframes`, and `encoder`; for sound `codec`,
`bitrate_kbps`, `channels`, `sample_rate` and `tracks` (`first` or `all`).
A half left out is passed through, so a profile with neither is a pass
through.

### The picture's settings

| Field | Values | Meaning |
|---|---|---|
| `width`, `height` | even, up to 7680x4320 | the frame made; a side left out follows the source's shape |
| `fps` | `{num, den}` | the frame rate; the source's when left out |
| `bitrate_kbps` | 100 to 100000 | the picture's bitrate |
| `keyframe_frames` or `keyframe_ms` | frames, or ms | the GOP; frames is Flussonic's `gop` |
| `preset` | `p1` to `p7` on NVENC, `ultrafast` to `placebo` on x264 | the encoder's speed against quality; a preset another family uses is left out |
| `fit` | `pad` (the default), `crop`, `stretch` | `pad` lays the whole picture in with bars, `crop` fills the frame and cuts what overhangs, `stretch` fills it and changes the picture's shape. `pad` and `crop` go by the picture's shape as shown, not its stored size: 704x480 shown at 16:9 fills a 16:9 frame, and a 4:3 picture is laid in 960 wide in a 1280x720 frame |
| `aspect` | `auto` (the default), `16:9`, `4:3` | the shape the source is shown at. `auto` is what the stream says (MPEG-2's aspect code, H.264's VUI); the others override it for a broadcaster that flags its picture wrong. A source whose shape changes mid stream, adverts between 4:3 and 16:9, is fitted again at the change |
| `deinterlace` | `off`, `on`, `auto` | `auto` makes progressive every picture the decoder marks as interlaced and passes the rest; `on` does every picture; `off` takes both fields as one frame, which combs on movement. Off when left out of a rendition; `auto` in every built in profile |
| `deinterlace_method` | `yadif` (the default), `greedyh`, `linear` | how the missing lines are made. On an NVIDIA card it runs on the card, with a kernel of GodwinMix's own: `yadif` and `greedyh` keep still detail woven and rebuild the lines only where the two fields comb, `linear` rebuilds every line. Without a card, or with `GMX_CUDA_DEINTERLACE=off`, it is GStreamer's `deinterlace` on the CPU with the method named. The renditions of one source that deinterlace the same way share one deinterlace |
| `match_source` | `{max_height, bitrate_kbps?}` | a source taller than `height` is made at its own height up to `max_height`, the frame's shape kept, at `bitrate_kbps` at `max_height` (else the rendition's bitrate grown by the area); a source no taller at `width` x `height`. `max_height` is even, 144 to 4320, and taller than `height` |
| `encoder` | `gpu`, `cpu` | see below |

Measured on an RTX 4060 Ti with 40 s of a 1080i and of a 480i broadcast,
played at their own speed through a channel's scale to 1280x720: on the
card a 1080i channel's deinterlace and scale take 0.08 of one core, where
taking the picture down for YADIF on the CPU took 0.37, and a 480i one
0.05 against 0.11. Scored with VMAF against ffmpeg's bwdif, the card's
pictures came within 0.15 of YADIF's (93.7 against 93.8 for 1080i, 83.0
for both at 480i). A progressive picture passes for nothing to speak of.
`dev/build-deinterlace-ptx.sh` rebuilds the kernel; the measuring run is
`measure_deinterlace` in `plugins/ingest/src/transcode/`.

### A ladder

`ladder` is further pictures of the same programme for the HLS output, each
a variant of the master playlist under `video`, so a player on a weak
connection can step down. Every one shares the sound. A field left out of a
rung is taken from `video`, except `match_source`, which is `video`'s alone:

```json
{"id": "news-abr", "name": "News, 1080p and 720p",
 "video": {"codec": "h264", "width": 1920, "height": 1080, "bitrate_kbps": 4500, "keyframe_frames": 50, "deinterlace": "auto"},
 "ladder": [{"width": 1280, "height": 720, "bitrate_kbps": 1500}],
 "audio": {"codec": "aac", "bitrate_kbps": 128, "tracks": "all"}}
```

The rungs are decoded once and deinterlaced once, then scaled and encoded
each, so a rung costs a scale and an encode (one more NVENC session). A rung
that comes out the same as `video` or an earlier rung, a 720p rung under a
`match_source` picture of an SD source, say, is that variant already and is
left out of the master. The channel's other outputs carry `video` alone.
Through the Flussonic URLs the variants are `tracks-v1a1`, `tracks-v2a1`
and on. The rungs' encoders start together, so their keyframes fall
together; one built again on its own later keeps its own.

`video.encoder` is `gpu` or `cpu`. Left out, the planner takes a hardware
encoder first and software when the GPU is full or has no encoder for the
codec. `gpu` refuses rather than fall back to the CPU, and `cpu` never
takes the GPU. It is a field of any rendition, not only a profile's.

The built in profiles, which cannot be changed or removed:

| id | video | audio |
|---|---|---|
| `copy` | as it comes | as it comes |
| `aac-sound` | as it comes | AAC 128 kbit/s, every track |
| `h264-480p-800` | H.264 854x480, 800 kbit/s | AAC 128 kbit/s, every track |
| `h264-720p-1500` | H.264 1280x720, 1500 kbit/s | AAC 128 kbit/s, every track |
| `h264-1080p-4000` | H.264 1920x1080, 4000 kbit/s | AAC 128 kbit/s, every track |
| `h264-hd-1080p-else-720p` | H.264 1280x720 at 1500 kbit/s, an HD source at 1920x1080 and 4500 kbit/s (`match_source`) | AAC 128 kbit/s, every track |
| `h264-hd-1080p-720p-ladder` | the same, with a 1280x720 1500 kbit/s rung under an HD source's 1080p | AAC 128 kbit/s, every track |

Each picture profile pads with black bars by the picture's shape,
deinterlaces what the stream marks as interlaced (`auto`) with YADIF and puts
a keyframe every 2000 ms.

## Methods

| Method | Scope | Takes | Answers |
|---|---|---|---|
| `headend.channel.list` | read | nothing | `{channels: [{channel, show, follows, detached_from}], profiles, encoders: [{id, codec, on}], following}` |
| `headend.channel.add` | admin | `{channels, dry_run}` | `{done, refused: [{index, name, why}], dry_run}` |
| `headend.channel.update` | admin | `{name, channel}` | `{done, refused, restarted}` |
| `headend.channel.bulk` | admin | `{names, profile?, custom?, enabled?, interface?, gpu?, video_overrides?}` | `{done, refused, restarted}`. `gpu` is a card for every one, or `"auto"`, which places them by load and evens the cards out at once (`gpu.rebalance` for those channels). `video_overrides` replaces each one's; `{}` clears them |
| `headend.channel.remove` | admin, destructive | `{names}` | `{done, refused}` |
| `headend.channel.follow` | admin | `{names}` | `{done, refused}` |
| `headend.profile.save` | admin | `{profile}` | `{id, channels}`: the channels that take it |
| `headend.profile.remove` | admin, destructive | `{id, move_to?}` | `{id, channels}` |
| `headend.scan` | admin | `{uri, interface?, wait_ms?}` | see below |
| `headend.export` | read | nothing | `{text, notes}` |

`headend.channel.add` checks each channel, then prices the ones that pass
with `show.add_many`'s dry run, which also refuses an address no input
takes. A channel refused at either step is not kept, and `why` says what to
send instead. `dry_run: true` stops there. A name taken by a channel here or
by a stream of the followed file is refused. A refusal after the channel
was kept (a show the station could not make just then) says `kept, and
tried again every 30 s`.

`headend.channel.update` replaces the channel whole. A different `name`
renames it, which changes its public path and makes its show again.

`headend.profile.remove` refuses a profile some channel takes, with
`data.channels`, unless `move_to` names the profile they take instead.

`headend.export` writes the channels kept here as Flussonic `stream`
blocks, a ladder as one more `vb` and `size` on the line for each rung. What
Flussonic has no word for is in `notes`: a keyframe interval in
milliseconds, a push that is a copy while the channel converts, an aspect
override, a method other than YADIF, `match_source`, and an explicit
`deinterlace=false`, which an import reads back as auto unless
`[headend] flussonic_deinterlace = "file"`.

For an agent, each method is a tool behind `search_tools`: `headend_channels`,
`add_headend_channels`, `update_headend_channel`, `bulk_edit_headend_channels`,
`remove_headend_channels`, `follow_flussonic_again`, `save_transcode_profile`,
`remove_transcode_profile`, `scan_feed` and `export_flussonic`.

## A followed Flussonic file

On a station started with `godwinmix headend --flussonic FILE`,
`headend.channel.list` lists the file's streams after the channels kept
here, each with `follows` set to the file and the show it runs as.

`headend.channel.update`, `headend.channel.bulk` and
`headend.channel.remove` take a followed stream as well. The first two take
it over: the station keeps the edited channel from then on, its running
show is handed from the file to the station without a restart, the entry
has `detached_from` set to the file, and later edits to that stream in the
file are not applied. A removal hides the stream from the file the same
way, and stops its show; the file itself is not edited.
`headend.channel.follow` gives a stream back: the station drops its own
copy, and the file's version runs, made again only if it differs.

## The scan

`headend.scan` opens the feed through the same input a channel would use,
as a probe that is no show: it is in no list and raises no alarm. It reads
the program table until it has stayed the same for 2.5 seconds, then opens
one probe per program (48 at most), all on the one socket, until each has
sent its picture's size, or its sound for a program with no picture, or
`wait_ms` (8000 when left out, 2000 to 20000) has passed. Every probe is
closed before the answer.

```json
{"uri": "udp://@239.0.0.1:5000", "listened_ms": 6120,
 "programs": [
   {"number": 1, "name": "News", "provider": "Example", "streams": ["h264", "aac"],
    "tracks": [{"kind": "video", "codec": "h264", "width": 1920, "height": 1080, "fps": 25.0, "pid": 256},
               {"kind": "audio", "codec": "aac", "channels": 2, "lang": "eng", "pid": 257}],
    "kbps": 6100},
   {"number": 2, "name": "Sport", "streams": ["mpeg2", "ac3", "ac3"], "scrambled": true, "channel": "CH102"}]}
```

| Field | Meaning |
|---|---|
| `number`, `name`, `provider` | from the PAT and the SDT; a feed with no SDT has no names |
| `streams` | what the PMT lists, in its order, teletext and subtitles too |
| `tracks` | the video and sound the program sent while it was listened to, with the sizes and rates its frames said |
| `scrambled` | scrambled packets arrived: the key is missing here, and there will be no picture |
| `channel` | the channel that already takes this program from this address |
| `error` | set when no program was read: what the input said, and what to check |

The scan needs the ingest plugin running, as every channel does.
