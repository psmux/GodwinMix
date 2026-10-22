# Reference: the Flussonic facade

`godwinmix headend` answers on the port of the `http` line in Flussonic's
config (or `--port`), with the URLs and the API Flussonic had, so whatever
read Flussonic reads GodwinMix the same way. [Replace Flussonic with
GodwinMix](../how-to/replace-flussonic.md) is the walk through; this page is
every path and field.

## The command

```sh
godwinmix --config godwinmix.toml headend --flussonic /etc/flussonic/flussonic.conf [--port 8091] [--address 127.0.0.1] [--flussonic-log /var/log/flussonic/flussonic.log] [--check]
```

| Flag | Meaning |
|---|---|
| `--flussonic FILE` | Flussonic's config. Left out: `flussonic` in `[headend]` of the GodwinMix config |
| `--port N` | Answer on this port instead of the `http` line's |
| `--address A` | Answer on this address instead of the `http` line's (all addresses when the line names none) |
| `--flussonic-log FILE` | Write Flussonic style log lines here. Left out: `flussonic_log` in `[headend]`, or none |
| `--check` | Print what the import makes and every line it does not carry, then exit |

The same settings as a table in `godwinmix.toml`:

```toml
[headend]
flussonic = "/etc/flussonic/flussonic.conf"
flussonic_log = "/var/log/flussonic/flussonic.log"
port = 8091
```

The station runs with mixing off: the show `main` is not started, and every
stream is a show without compositing. The station's own control port and
page are where they always are.

## How the config is read

| Flussonic | GodwinMix |
|---|---|
| `stream NAME { ... }` | a show without compositing, its id made from `NAME` (`CH117` is `ch117`). `NAME` stays the public id in every URL and in the API |
| `title` | the show's name |
| `disabled true` | the show is stopped |
| `input mpts-udp://eth1@239.10.0.1:2001?program=7` | `{"uri": "udp://@239.10.0.1:2001", "program": 7, "params": {"interface": "eth1"}}` |
| `input udp://...` | the same, without `mpts` |
| `input rtmp://...`, `srt://`, `rtsp://`, `http(s)://`, `file://` | passed through as written |
| a second `input` | the show's backup input |
| `transcoder ...` | the rendition of the show's one HLS output, below |
| no `transcoder` | the HLS output copies the input |
| `segment_duration`, `segment_count` | the HLS output's `segment_ms` and `window` (in seconds) |
| `push URL` | a further output of the show, with the same rendition |
| `dvr` | not carried: GodwinMix keeps the live window only |

Every directive and option the import does not use is listed with its line
by `--check` and in the log at start. Nothing is dropped without a line.

### The transcoder line

`transcoder deviceid=0 gop=50 hw=nvenc deinterlace=false vb=1500k vcodec=h264 size=1280x720:fit:#000000 ab=128k`
becomes:

```json
{"video": {"codec": "h264", "bitrate_kbps": 1500, "width": 1280, "height": 720,
           "fit": "pad", "pad_color": "#000000", "deinterlace": "off",
           "keyframe_frames": 50, "hardware": "nvenc", "device": 0},
 "audio": {"codec": "aac", "bitrate_kbps": 128, "tracks": "all"}}
```

`codec`, `width`, `height`, `fps` and `bitrate_kbps` are acted on now.
`fit`, `pad_color`, `deinterlace`, `keyframe_frames`, `preset`, `bframes`,
`hardware`, `device` and `tracks` are passed on for the GPU conversion, which
this build does not have yet; `--check` lists them as `waiting`. `vb=copy`
copies the video and `ab=copy` the sound. A second `vb=` (a ladder) is not
carried: the HLS output of a show without compositing has one rendition.

## Changes to the file

The file is looked at once a second. When it changes it is read again and
compared stream by stream: a stream whose input or outputs changed is made
again, one whose `disabled` moved is stopped or started, one whose title
moved is renamed, and every other stream is left running. A file with a
mistake changes nothing and the log says what is wrong. Every 30 seconds the
file is applied again, which makes any stream that could not be made before.

Which show each stream runs as is kept in `flussonic-shows.json` beside the
list of shows, so a restarted headend finds its shows and restarts none of
them unless the file changed meanwhile.

## Playback paths

No key or token: these paths are for a gateway or a cache on the same
network that checks its own signatures, as Flussonic's were. Every URI in a
playlist is relative, so a signature in the path covers every file a player
fetches after the first.

| Path | Answer |
|---|---|
| `/<name>/index.m3u8`, `/<name>/playlist.m3u8` | the master playlist; every sound track an `EXT-X-MEDIA` in group `audio`, the first `DEFAULT=YES` |
| `/<name>/video.m3u8`, `/<name>/mono.m3u8` | the default media playlist, its URIs under `tracks-<sel>/` |
| `/<name>/tracks-v1a1/mono.m3u8` | the video with the first sound, where segments mux them (MPEG-TS) |
| `/<name>/tracks-v1/mono.m3u8` | the video alone |
| `/<name>/tracks-a<n>/mono.m3u8` | sound track `n`, from 1 |
| `/<name>/tracks-<sel>/YYYY/MM/DD/HH/MM/SS-<ms>.<ext>` | one segment, named by the UTC second it began and its length in milliseconds |
| `/<name>/tracks-<sel>/init-<id>.mp4` | the init segment of fragmented MP4 segments; `id` changes whenever its bytes do |
| `/<name>/preview.jpg` | the show's latest picture, 640 wide |
| `/<name>/embed.html` | a player page that loads only `../flu/player/hls.min.js` |
| `/flu/player/hls.min.js` | hls.js 1.5.20 |
| `/<name>/mpegts` | live MPEG-TS over HTTP |

A segment's name comes from the wall clock time its first frame was taken,
so a name is never handed out twice, not after a restart either. A cache in
front may keep segments by path for as long as it likes.

What this build serves: the segments are fragmented MP4 (`.m4s`, with an
`init-<id>.mp4`), so the master names `tracks-v1/mono.m3u8` for the picture
and `tracks-a1/mono.m3u8` for the sound, and `tracks-v1a1` and `/mpegts`
answer 501. MPEG-TS segments (`.ts`, with `tracks-v1a1` muxed and the first
sound inside the variant) come with the TS packaging, which is not in this
build yet. Until a stream runs a path answers 404 with why; while its
output starts it answers 503 with `Retry-After: 2`.

Caching headers: playlists `no-cache`, segments and init segments
`public, max-age=600, immutable`.

## The API

Basic auth with the config's `edit_auth` user and password; `view_auth`,
when the config has it, may read. A config with no `edit_auth` answers this
machine only. A wrong or missing password is a 401 with a `Basic` challenge.

| Call | Answer |
|---|---|
| `GET /streamer/api/v3/streams` | `{"estimated_count": N, "streams": [...]}`, every stream of the config |
| `GET /streamer/api/v3/streams/<name>` | one stream |
| `PUT /streamer/api/v3/streams/<name>` | merge a JSON body into the stream's block, add the stream when there is none, write the file and apply it; answers the stream |
| `DELETE /streamer/api/v3/streams/<name>` | take the stream out of the file and stop it; 204 |
| `GET /streamer/api/v3/config` | the config's typed view, with no password in it (input and push URLs read `user:***@host`) |
| `GET /streamer/api/v3/health`, `GET /health` | no auth: `{"status": "ok", "server": "godwinmix", "streams", "shows", "running"}` |

### One stream

```json
{"name": "CH117", "title": "News", "disabled": false, "static": false,
 "inputs": [{"url": "mpts-udp://eth1@239.10.0.1:2001?program=7"}],
 "transcoder": {"global": {"hw": "nvenc", "deviceid": 0, "gop": 50, "deinterlace": false},
                "tracks": [{"content": "video", "codec": "h264", "bitrate": 1500000,
                            "size": {"width": 1280, "height": 720, "strategy": "fit", "background": "#000000"}},
                           {"content": "audio", "codec": "aac", "bitrate": 128000}]},
 "stats": {"status": "running", "alive": true, "bitrate": 1650, "input_bitrate": 1819, "lifetime": 61000,
           "media_info": {"tracks": [{"content": "video", "track_id": "v1", "codec": "h264", "width": 1280, "height": 720,
                                      "fps": 29.97, "bitrate": 1650, "profile": "High", "level": "3.1"},
                                     {"content": "audio", "track_id": "a1", "codec": "aac", "channels": 2}]},
           "inputs": [{"url": "...", "active": true,
                       "media_info": {"tracks": [{"content": "video", "track_id": "v1", "codec": "h264", "width": 1280, "height": 720, "fps": 29.99, "last_gop": 60}]},
                       "stop_events": [{"code": "scrambled", "at": 1792756804000}]}],
           "input": {"error_rate": 0.0, "num_sec_no_data": 0},
           "transcoder": {"restarts": 0}}}
```

| Field | Meaning |
|---|---|
| `stats.status` | `running` when the input has frames and the HLS output is live; `stopped` when disabled; `starting` otherwise |
| `stats.alive` | running, with video going out |
| `stats.bitrate` | what the HLS output sends, kbit/s; 0 unless running |
| `stats.input_bitrate` | what arrives for the chosen program, kbit/s |
| `stats.lifetime` | milliseconds since the output last went live; 0 unless running |
| `stats.media_info.tracks` | what the output serves, read from its master playlist; for a copy before that is read, the input's |
| `stats.inputs[].media_info.tracks` | what the input carries, with `pid` once the input reports each track |
| `stats.inputs[].stop_events` | `scrambled` for each time in the last ten minutes the input reported encrypted packets |
| `stats.input.error_rate` | continuity errors in the last second |
| `stats.input.num_sec_no_data` | seconds since the last frame, while the input is not live |
| `stats.transcoder.restarts` | times the HLS output had to connect again |
| `stats.last_error` | what is wrong, in GodwinMix's words, when something is |

Rates in `stats` are kbit/s and the transcoder's `bitrate` is bit/s, as in
Flussonic.

### PUT

The body takes `title`, `disabled`, `static`, `inputs` (`[{"url": ...}]`,
replacing every input) and `transcoder` (the object the API answers with,
a line of options as a string, or `null` to remove it). A field left out
keeps what the file has, and every line of the block the body does not
name is written back as it was, so Flussonic could read the file again.
Any other field is refused with the list of these.

```sh
curl -u admin:password -X PUT localhost:8090/streamer/api/v3/streams/CH117 \
     -d '{"title": "News", "transcoder": "vb=2m size=1280x720 ab=128k"}'
```

## The log

With a log path, these are written in Flussonic's format, for a reader that
counts them:

```text
2026-10-23 12:00:04.123 <0.0.0> [CH117] Input closed with reason => scrambled
2026-10-23 12:00:04.123 <0.0.0> [CH117] transcoder nvenc_pin open_session configure fail: error 21, no encoder session free
```

The first when an input starts to report scrambled packets, the second when
an output's error says the GPU gave it no encoder session; each again once a
minute while it holds. The file is rotated at 10 MB, keeping three old files
(`.1` to `.3`).
