# Configuration

Every key the mixer reads, with its default and what it is for. The file is
TOML. Print a complete annotated one with:

```sh
godwinmix --example-config > godwinmix.toml
```

The copy in the repository is [`godwinmix.example.toml`](../../godwinmix.example.toml)
and it is the same text. This page is the flat list; that file is the same
material in the order you would fill it in.

Sections may be left out entirely. Every key has a default and the mixer starts
with an empty file.

## `[canvas]`

The contract every source is normalised to. It cannot change while a broadcast
is running, because the output encoder is started once and never restarted, so
pick it deliberately before you go live.

| Key | Default | Meaning |
|---|---|---|
| `width` | 1920 | canvas width in pixels |
| `height` | 1080 | canvas height |
| `fps` | 30 | canvas frame rate; every source is retimed to it |
| `sample_rate` | 48000 | audio sample rate in Hz |
| `channels` | 2 | audio channels |

## `[program]`

The programme encoder, which runs from the moment the first output is added
until the mixer stops.

| Key | Default | Meaning |
|---|---|---|
| `video_bitrate_kbps` | 6000 | video bitrate. 2,500 at 720p30 is the recommended starting point on a small uplink |
| `audio_bitrate_kbps` | 160 | audio bitrate |
| `keyframe_interval_secs` | 2 | keyframe interval. Two seconds is what every CDN asks for |
| `audio_ramp_ms` | 180 | audio crossfade on a take. Zero gives an audible click |
| `av_offset_ms` | unset | milliseconds to hold video back relative to audio at the encoders. Unset means the known priming delay of the AAC encoder in use (`fdkaacenc` 43 ms, `avenc_aac` 21 ms) |

## `[multiview]`

The operator's mosaic: every source and the programme in one JPEG stream over
the WebSocket. It is a second encoder and it costs, so it stops when nobody is
watching.

| Key | Default | Meaning |
|---|---|---|
| `enabled` | true | build the mosaic at all. `false` removes it entirely |
| `width` | 960 | width of the whole mosaic, not of one cell |
| `height` | 540 | height of the whole mosaic |
| `fps` | 8 | mosaic frame rate |
| `jpeg_quality` | 60 | JPEG quality, 1 to 100 |
| `include_program` | true | show what is going out as one of the cells |

Bandwidth is roughly `width * height * fps * quality` to each connected
operator. The measured figure at the defaults is 8.0 fps, 22.7 KB a frame,
about 1.5 Mbit/s.

## `[control]`

| Key | Default | Meaning |
|---|---|---|
| `bind` | `"0.0.0.0:8080"` | address and port for the HTTP API, the WebSocket and the UI |
| `ui_dir` | unset | serve static UI files from this directory instead of the page built into the binary |
| `token` | unset | bearer token every `/api/*` request and the WebSocket must carry. Unset means the port is open to whoever can reach it |

`GODWINMIX_TOKEN` in the environment overrides `token`, so a deployment can keep
the secret out of the config file. Put it there rather than here.

## `[hardware]`

| Key | Default | Meaning |
|---|---|---|
| `decode` | `"auto"` | `auto`, `nvidia`, `va`, `videotoolbox`, `mediafoundation`, `d3d11`, `software` |
| `encode` | `"auto"` | the same set |

Decode and encode are chosen independently. `auto` picks the best backend
present; naming one makes startup fail when it is missing, which is what you
want on a server you control. See
[choose a hardware encoder](../how-to/choose-a-hardware-encoder.md).

## `[media]`

The ad clip library, on the machine running the mixer. The UI lists it, which
is why it is a server side directory rather than a browser file picker: an
operator in another city cannot hand the mixer a file from their own laptop.

| Key | Default | Meaning |
|---|---|---|
| `dir` | `"media"` | where the clips are |
| `max_depth` | 3 | how deep to recurse into subdirectories |
| `max_files` | 500 | upper bound on the listing, so a path pointing at something enormous cannot hang the control server |
| `probe_timeout_secs` | 3 | seconds to spend working out one file's duration |
| `allow_upload` | true | whether the control port may write files into the library |
| `max_upload_bytes` | 2147483648 | largest upload accepted. The body streams to disk, so this is about disk, not memory |
| `convert_threads` | 2 | x264 threads for a conversion, capped so a transcode cannot take every core from the live programme encoder |

## `[safety]`

The rules that stand in front of every take, for every caller through every
door. Enforced in `program.take`, not in a plugin and not in an agent's
prompt. [`docs/reference/safety.md`](safety.md) has the whole story.

| Key | Default | Meaning |
|---|---|---|
| `min_hold_ms` | 8000 | a take within this window of the last one is refused with -32003 and the milliseconds left |
| `max_takes_per_minute` | 12 | takes allowed in any rolling minute, counted per core |
| `flash_guard` | true | the ITU-R BT.1702-3 hold: a cut changing luminance by 20 cd/m2 over more than a quarter of the frame is held 360 ms from the next one (334 above 50 Hz), at most three in a second |
| `on_operator_silence` | `{ after_secs = 120, action = "alert" }` | what happens when the token that made the last take stops calling |

`action` is `"alert"`, `"hold"`, `"slate"` or `"fallback:<source id>"`. The
default is `alert` because a programme that keeps running is the safe state.
A misspelt action is a startup error rather than a silent `alert`.

`min_hold_ms` is a default, not a standard: no standards body publishes a
minimum shot length. The flash guard is the one hold that is regulated, and it
is on by default.

## `[[tokens]]`

Several credentials, each with its own scopes, beside the single
`[control] token` which still works and still carries everything.

| Key | Default | Meaning |
|---|---|---|
| `id` | required | legible, and recorded against every take in `program.history` |
| `secret` | required | the bearer token |
| `scopes` | `["read"]` | `read`, `operate`, `admin`. Anything more than read has to be asked for |
| `confirm` | `"none"` | `"required"` makes a destructive call answer -32020 with a confirm token first |
| `rehearsal` | false | accepted only by a core started with `--rehearsal` |
| `profile` | `"standard"` | which MCP tool surface this credential is meant for: `standard` or `minimal` |
| `agent` | false | this credential belongs to an unattended agent, so its `safety` override may only tighten |
| `safety` | none | `{ min_hold_ms, max_takes_per_minute, flash_guard }`, overriding `[safety]` for this token |

## `[security]`

| Key | Default | Meaning |
|---|---|---|
| `allow_exec_sources` | false | allow `exec:` sources, which run a command line on this machine |

Off by default and deliberately so. An `exec:` source is arbitrary code
execution for anyone who can reach the control port, which is a much larger
grant than "can switch cameras".

## `[browser]`

How `web+` sources are rendered. The preferred renderer is the CEF sidecar
`godwinmix-browser`. When it is not found, `web+` falls back to GStreamer's
`wpesrc`.

| Key | Default | Meaning |
|---|---|---|
| `sidecar` | unset | path to `godwinmix-browser`. Unset means look next to this executable, then on `PATH`. A path that does not exist makes `web+` fail with a clear message rather than silently falling back |
| `args` | `[]` | extra arguments appended to the sidecar's command line |
| `env` | `{}` | environment for the sidecar on top of the mixer's own. On a headless Linux box this is where `DISPLAY` and `PULSE_SINK` go |
| `overlay_fps` | 10 | frames per second the browser draws at while it is only drawing the page over video the mixer decodes itself |

`overlay_fps` is worth understanding before raising it. A superimposing page's
frames carry an alpha channel and cross raw, at 4 bytes a pixel against I420's
1.5, so a 720p page at the full canvas rate is 110 MB/s where an ordinary
source is 41. The compositor holds the last page frame between updates, so the
picture still leaves at the canvas rate with the video moving underneath.

## `[stall]`

What the supervisor does with a source that has stopped delivering.

| Key | Default | Meaning |
|---|---|---|
| `restart_after_secs` | 10 | seconds a source may deliver nothing before its pipeline is rebuilt |
| `rebuild_attempts` | 3 | attempts at full speed before the backoff starts |
| `rebuild_backoff_secs` | 30 | first delay after those attempts |
| `rebuild_backoff_max_secs` | 300 | ceiling the delay doubles up to |
| `hold_last_frame` | true | keep the last frame of a source being rebuilt on programme instead of cutting to the slate |

The backoff exists because of two specific nights: 485 rebuilds one night and
1,174 the next, on a superimposed source that could not recover. The counter is
cleared the moment the source delivers a frame.

## `[[sources]]`

One table per source. Sources added over the API are written to
`<config>.runtime.toml` beside the config file; once that file exists it is the
authoritative list and these are not merged in. Delete it to go back to the
config.

| Key | Default | Meaning |
|---|---|---|
| `id` | required | the slug the API and the UI use. Legible, stable across restarts |
| `name` | unset | what the operator sees. Defaults to the id |
| `uri` | required | see [sources](sources.md) for every scheme |
| `stall_timeout_secs` | 2.0 | seconds without a frame before the source is treated as dead and faded to the slate rather than frozen |
| `rtmp_client` | `"auto"` | `auto`, `rtmp2` or `librtmp`. The two GStreamer RTMP clients do not work with the same servers |
| `superimpose` | `"off"` | `off`, `auto` or `on`. Web pages only. `auto` hands the page's video to the mixer's own decoder where it can |
| `gain` | 1.0 | the operator's fader, 0.0 silent through 1.0 unity to a ceiling of 10.0, saved so a restart brings the desk back where it was left |
| `muted` | false | muted by the operator, saved separately from the fader so unmuting returns to the level it had |

## `[[outputs]]`

One table per destination. Each is supervised separately and one failing cannot
touch the programme.

| Key | Default | Meaning |
|---|---|---|
| `id` | required | the slug |
| `uri` | required | `rtmp://` or `rtmps://` |
| `policy` | `"own"` | `own` reconnects fast, for a server you control. `cdn` backs off much harder, because public ingests throttle aggressive reconnects |
| `queue_secs` | 5.0 | seconds of encoded data held before the muxer. This is the buffer that makes a short hiccup invisible to the viewer |

### `[outputs.reconnect]`

Overrides the policy outright. All three keys together.

| Key | Meaning |
|---|---|
| `initial_delay_ms` | first delay before a reconnect |
| `max_delay_ms` | ceiling |
| `multiplier` | how fast the delay grows |

## Environment variables

| Variable | What it does |
|---|---|
| `GODWINMIX_TOKEN` | the control token; overrides `[control] token` |
| `GODWINMIX_URL` | where `gmx ctl` and `godwinmix mcp` look for a mixer |
| `GST_DEBUG` | GStreamer's own logging, 1 to 9. Start at 3 |
| `GMX_BROWSER_SWITCHES` | extra Chromium switches for the sidecar |
| `GMX_SIDECAR_LOG` | where the sidecar writes its log |

The `LIVEBOXMIX_` spellings of the first two are read for one release and warn.
See [upgrading from LiveboxMix](../how-to/upgrade-from-liveboxmix.md).
