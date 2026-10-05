# Reference: renditions and the planner

A rendition is one concrete encoded form of a picture and its sound, such as
H.264 1280x720 at 30 fps and 3000 kbit/s with AAC stereo at 128 kbit/s. An
output asks for one with a `RenditionRequest`; the planner in
`crates/godwinmix-render` turns every request in a show into the smallest
graph that serves them all.

The core plans every programme output that asks for a rendition and builds
the plan in GStreamer (below, "In the core"). An output that asks for none
reads the programme encoder exactly as before and costs nothing more.

The shared types live in `crates/godwinmix-protocol/src/rendition/`. The
planner, its errors and the plan it returns live in `godwinmix-render`.

## The call

```rust
use godwinmix_render::{plan, diff, StaticCostModel};

let model = StaticCostModel::software();
let plan = plan(&sources, &requests, &model)?;   // Result<Plan, PlanError>
let changes = diff(&running, &plan);             // PlanDiff
```

`sources` is `&[(SourceId, StreamInfo)]`: each source's slug and what it
carries. `requests` is `&[(SourceId, RenditionRequest)]`: the slug an output
reads and what it wants. Both are plain data; the planner reads no clock,
starts no thread and touches no GStreamer element.

## A request

| Field | Type | Left out means |
|---|---|---|
| `id` | slug | required, unique among the requests planned together |
| `container` | `flv`, `mpeg-ts`, `mp4-fragmented`, `mkv`, `hls`, `ll-hls`, `dash`, `rtp`, `webrtc` | `flv` |
| `video.codec` | `h264`, `h265`, `av1`, `vp8`, `vp9`, `mpeg2`, `prores` | the source's codec when the container carries it, else the first codec in the container's list this machine can encode |
| `video.width`, `video.height` | pixels | the source's; give one and the other follows the source's aspect ratio, rounded to even |
| `video.fps` | `{num, den}` | the source's |
| `video.bitrate_kbps` | kbit/s | on a copy, any; on an encode, a default from the size (below) |
| `video.bitrate_tolerance` | fraction | 0.25 |
| `video.keyframe_ms` | ms | the ladder's interval (below) |
| `audio.codec` | `aac`, `opus`, `mp3`, `ac3`, `pcm` | as for video |
| `audio.channels`, `audio.sample_rate` | | the source's |
| `audio.bitrate_kbps` | kbit/s | 64 per channel for AAC and MP3, 48 for Opus, 96 for AC-3 |
| `no_video`, `no_audio` | bool | false |

An empty request is a plain copy of the source.

Default video bitrates come from bits per pixel per frame, rounded to 100
kbit/s so two requests that leave it out share one encoder: 0.1 for H.264,
0.2 for MPEG-2, 0.11 for VP8, 0.06 for H.265, AV1 and VP9. H.264 1080p30 is
6200 kbit/s and 720p30 is 2800.

## What each container carries

The first codec in each list is the default.

| Container | Video | Audio |
|---|---|---|
| `flv` | h264, h265, av1 | aac, mp3 |
| `webrtc` | h264, vp8, vp9, av1 | opus |
| `mpeg-ts` | h264, h265, av1, mpeg2 | aac, mp3, ac3, opus |
| `mp4-fragmented` | h264, h265, av1, vp9 | aac, opus, mp3, ac3 |
| `mkv` | h264, h265, av1, vp8, vp9, mpeg2, prores | aac, opus, mp3, ac3, pcm |
| `hls`, `ll-hls` | h264, h265, av1 | aac, mp3, ac3 |
| `dash` | h264, h265, av1, vp9 | aac, opus, ac3 |
| `rtp` | h264, h265, av1, vp8, vp9, mpeg2 | opus, aac, mp3, ac3, pcm |

FLV is taken to be enhanced RTMP, which carries HEVC and AV1. An output
going to a server that only speaks classic RTMP asks for `h264` by name.

## The rules

1. **Copy when nothing changes.** A track is copied when the source is
   encoded, the codec, size and frame rate are equal, the bitrate is within
   the tolerance of the one asked for (or either is unknown), the keyframe
   interval is equal where both are known, and the container can carry the
   source's codec. A copy into a container that cannot carry the codec
   becomes an encode. A raw source (the programme, a camera) is never copied.
2. **Decode once per source and track**, however many renditions use it.
3. **Scale once per distinct size and frame rate** of a source, shared by
   every encode that wants that shape. No scale at all when both match.
4. **Encode once per distinct shape.** The key is (source, codec, width,
   height, frame rate, bitrate, keyframe interval). Every request with the
   same key shares one encoder and gets its own Mux reading it.
5. **One keyframe interval per source.** Every encode of a source uses the
   shortest interval any encoded rendition of it asked for, 2000 ms when
   none asked. When a rendition of that source is a copy and the source's own
   interval is shorter, that is used instead, so the encodes line up with
   the copy. Platforms state a maximum interval, so the shortest satisfies
   them all. `Plan.keyframe_ms` records the choice per source.
6. **Audio the same way**: copy, one decode, one convert per distinct
   channel count and rate, one encode per distinct (codec, channels, rate,
   bitrate).
7. **Encoder choice.** Among the encoders of the right codec, hardware comes
   first, then software, each in the order the cost model lists them. An
   encoder is skipped when the model says it cannot make the shape
   (`encode_cost` is `None`), or when it is on a device whose `room` would be
   exceeded by what this plan already put there plus this encode. Software is
   never refused for room: whether the CPU can take the whole plan is the
   governor's `admit`.
8. **Nothing possible is an error** that names what is missing and the
   nearest shape that works (below).

## The plan

`Plan.nodes` is in start order: every node comes after the nodes it reads.

| Kind | Id | Does |
|---|---|---|
| `source` | `source:cam` | where the source's bytes or frames come from |
| `copy` | `copy:cam:video` | the source's encoded track, parsed and passed on |
| `decode` | `decode:cam:video` | the source's one decoder for that track |
| `scale` | `scale:cam:1280x720p30` | scale, convert and change rate |
| `encode` | `encode:cam:h264:1280x720p30:3000k:g2000` | one video encoder |
| `audio-convert` | `aconvert:cam:1ch48000` | resample or remix |
| `audio-encode` | `aencode:cam:aac:2ch48000:128k` | one audio encoder |
| `mux` | `mux:youtube` | one request's container, reading its video and audio |

A fractional rate is written `30000/1001` in an id. Ids are built from what
the node does and never from a counter, which is how `diff` matches them.

Each node has `inputs` (node ids), `serves` (request ids), `device` (`cpu`,
or the GPU's name, or `gpu` when a hardware slot names none), `cost`, and on
Encode and Mux nodes a `reason`. `Plan.cost` sums the nodes per device and
`Plan.total` sums them all.

## Reasons

A `reason` is `{code, text}`. The text is a sentence the page can show as
it is.

| Code | On | Text |
|---|---|---|
| `hardware` | encode | `h264-nvidia is a hardware H.264 encoder on nvidia0` |
| `software-only` | encode | `using h264-software-x264 because this machine has no hardware H.264 encoder` |
| `device-full` | encode | `using h264-software-x264 because the GPU nvidia0 is full` |
| `shape-unsupported` | encode | `using h264-software-x264 because h264-nvidia cannot make 7680x4320 at 30 fps` |
| `copied` | mux | `copied: the source's video goes out as it is` |
| `transcoded` | mux | `encoded because the source is 1920x1080 and this output wants 1280x720` |

The `transcoded` text names the first thing that differs, in this order:
raw source, container cannot carry the codec, codec, size, frame rate,
bitrate, keyframe interval. For example `encoded because flv cannot carry
the source's VP9` or `encoded because the source runs at 6000 kbit/s and
this output wants 3000`.

## Errors

`PlanError` carries `code()`, a sentence from `to_string()`, and `data()`,
the same facts as an object with `code` and `message` in it.

| Code | When | Data |
|---|---|---|
| `unknown-source` | a request reads a source not in `sources` | `request`, `source`, `known` |
| `duplicate-request` | two requests share an id | `request` |
| `container-codec` | a codec asked for by name that the container cannot carry | `request`, `container`, `codec`, `allowed` |
| `missing-track` | video or audio asked for from a source without it | `request`, `source`, `track` |
| `nothing-asked` | a request drops both tracks, or the source has neither | `request` |
| `no-encoder` | no video encoder here can make the shape | `request`, `codec`, `shape`, `tried`, `nearest` |
| `no-audio-encoder` | no audio encoder here for the codec | `request`, `codec`, `nearest` |

`tried` lists each encoder passed over with `why`. `nearest` is the first of
these that some encoder can make: the same codec at half the frame rate
when it is over 30, then the same codec at each smaller standard height
(2160, 1440, 1080, 720, 540, 480, 360) at both rates, then each other codec
the container carries in the same order. It carries `codec`, `width`,
`height`, `fps`, `encoder` and a `text`. A message reads:

```
No encoder on this machine can make H.265 1280x720 at 30 fps for `a`. H.264 1280x720 at 30 fps with h264-software-x264 is possible; ask for that instead.
```

## The diff

`diff(old, new)` returns node ids in four lists.

| List | Holds | Order |
|---|---|---|
| `stop` | in the old plan only | consumers first |
| `restart` | in both under one id, but a different kind or different inputs | producers first |
| `start` | in the new plan only | producers first |
| `keep` | in both, unchanged | |

Apply `stop`, then `restart`, then `start`. A change to `serves` alone is
not a restart: a second output on a running encoder starts its own Mux and
nothing else. Adding a rung to a ladder starts its Scale, Encode and Mux.
Removing the last output on an encoder stops its Mux, Encode and Scale. An
encoder moved from a full GPU to software keeps its id and is restarted.
A rung that asks for a shorter keyframe interval than the ladder has
changes every encode of that source, because they must stay aligned.

## The cost model

```rust
pub trait CostModel {
    fn encoders(&self) -> Vec<EncoderSlot>;
    fn encode_cost(&self, shape: &VideoShape, enc: &EncoderSlot) -> Option<Cost>;
    fn scale_cost(&self, from: &VideoShape, to: &VideoShape) -> Cost;
    fn decode_cost(&self, shape: &VideoShape) -> Cost;
    fn audio_cost(&self, shape: &AudioShape, work: AudioWork) -> Option<Cost>;
    fn mux_cost(&self, container: Container, egress_kbps: u32) -> Cost;   // has a default
    fn room(&self, device: &str) -> Room;                                  // default: no limit
}
```

The planner asks and never measures. `room` is what is left on a hardware
device for this plan, not counting what the plan being replaced holds
there; otherwise a replan would find the GPU full of its own encoders.
`audio_cost(.., AudioWork::Encode)` returning `None` means no encoder for
that audio codec.

`StaticCostModel` answers from fixed figures for a machine with no
calibration, scaled by pixel rate from 1080p30: x264 1500 millicores, x265
6000, SVT-AV1 4500, VP8 2200, VP9 4500; a hardware encode 100 millicores and
160 thousandths of its device with one session, and no picture wider or
taller than 4096; decode 250 to 500 millicores by codec. `software()` gives
x264, x265, SVT-AV1, VP8 and VP9 with AAC and Opus; `with_hardware`,
`with_room` and `without` shape it for a test.

## On the wire

### `output.add` and `output.set`

Both take `rendition`, one of:

| Shape | Means |
|---|---|
| `{"preset": "youtube-720p30"}` | a preset from `rendition.presets` |
| `{"ladder": [RenditionRequest, ...]}` | a custom ABR ladder, top rung first, for an `hls/output` |
| a `RenditionRequest` | one rendition, as the table above |
| absent | the programme encoder, as every output always was |

`output.set` with `"rendition": null` puts an output back on the programme
encoder; leaving it out keeps what the output has. The `copy` preset means
the same as absent for a programme output.

A request's `id` is replaced by the output's id. A ladder's rungs are
`<output>-<rung id>`, the rung id being the request's own `id` when it is a
slug and its height (`480p`) otherwise, so `hls` with `abr-ladder-4` plans
`hls-1080p`, `hls-720p`, `hls-480p` and `hls-360p`. `output.get` and
`output.list` carry `rendition` back as it was sent, and `shed` while the
governor has the output's rendition stopped.

A plan the planner cannot make is refused with `-32602` and the planner's
`data` (the error table above). A plan the governor will not admit is
refused with `-32003` and:

```json
{
  "need": {"cpu_millicores": 988, "device_millis": 0, "device_sessions": 0, "egress_kbps": 0, "memory_mib": 31},
  "have": {"cpu_millicores": 0, "device_millis": 0, "device_sessions": 0, "egress_kbps": 4294967295, "memory_mib": 5731},
  "advice": [{"text": "720p30 H.264 on h264-software-x264 fits",
              "request": {"id": "o5", "container": "flv", "no_video": false, "no_audio": false,
                          "video": {"codec": "h264", "width": 1280, "height": 720, "fps": {"num": 30, "den": 1}, "keyframe_ms": 2000},
                          "audio": {"codec": "aac", "channels": 2, "sample_rate": 48000, "bitrate_kbps": 128}}}]
}
```

An `egress_kbps` of 4294967295 in `have` means no limit is known: nothing
has told the governor the uplink.

Each `advice.request` can be sent as the output's `rendition` as it is.
Nothing is started by a refused call: the whole change is planned and
admitted before any element is touched.

An output the core attaches at start, from the config or from what it kept
last time, has nobody to hear a refusal. A show a station starts again on a
busy machine is the usual case. Such an output is kept: `output.list` shows
it with `state: "failed"` and a `shed` saying why, it stays in the saved
list, and the core asks for it again by itself, half a second after the
refusal and then less often, never more than ten seconds apart while the
governor is what said no. Once there is room it attaches and reads as any
other output. `output.remove` takes it away; `output.set` tries the new
settings at once and keeps them waiting if they are refused too.

### `rendition.presets {}`

`{presets: [{id, title, group, request, ladder?, cost?, available, why?}]}`.
Every built in preset, priced on this machine: `cost` is the whole preset
(every rung, the scaling and the sound) as the governor would count it. One
this machine cannot make has `available: false` and `why`.

| Id | Group | What |
|---|---|---|
| `youtube-1080p30` | platform | FLV, H.264 1920x1080 30 fps 6000 kbit/s, AAC 128 kbit/s |
| `youtube-720p30` | platform | the same at 1280x720, 3000 kbit/s |
| `facebook-720p30` | platform | 1280x720 30 fps, 4000 kbit/s |
| `twitch-1080p60` | platform | 1920x1080 60 fps, 6000 kbit/s |
| `twitch-720p30` | platform | 1280x720 30 fps, 3000 kbit/s |
| `audio-only-aac` | audio | no picture, AAC 128 kbit/s |
| `abr-ladder-4` | ladder | HLS: 1080p 5000, 720p 2800, 480p 1400, 360p 800 kbit/s |
| `abr-ladder-3` | ladder | HLS: 720p, 480p, 360p |
| `copy` | copy | no conversion |

Every platform and ladder preset asks for a keyframe every 2000 ms.

### `rendition.plan {scope?}`

`scope` is `programme` (the default) or `channel:<id>`. The answer:

```json
{"nodes": [...,
           {"id": "encode:programme:h264:1280x720p30:3000k:g2000", "kind": "encode",
            "serves": ["o1", "o2", "o3", "o4"], "encoder": "h264-videotoolbox",
            "reason": {"code": "hardware", "text": "h264-videotoolbox is a hardware H.264 encoder on videotoolbox"},
            "cost": {"cpu_millicores": 74, "device_millis": 104, "device_sessions": 1, "egress_kbps": 0, "memory_mib": 18}},
           ...],
 "totals": {"cpu_millicores": 216, "devices": {"videotoolbox": {"millis": 104, "sessions": 1}}, "egress_kbps": 12512}}
```

That is four outputs on `youtube-720p30` on an M4 Pro: one scale, one
encoder and one AAC encoder, serving all four.

`serves` names outputs, each once, however many rungs of one ladder a node
works for. `encoder` is the catalogue id. `shed` is on a node the governor
has stopped, with why.

### `governor.status {}`

`{calibrated_at?, fingerprint?, calibrating, cpu: {cores, used_millicores, measured_millicores?,
room_millicores}, devices: [{id, kind, used_millis, room_millis,
sessions_used, sessions_max?}], egress_kbps, shed: [{what, why}]}`.
`calibrated_at` is Unix seconds and absent before the first calibration.
`room_millicores` is what could be admitted now, after the reserve.
`used_millicores` is the governor's own count: the larger of what it has
admitted and what its process measures, with what shows holding a ticket
report of themselves. A station also answers `measured_millicores`, what its
processes cost now: its own, every show process and every plugin process it
started, the ingest plugin that runs the direct shows among them. It is read
when `governor.status` is called and not otherwise. It is left out by a
single process core, and on Windows once any show or plugin runs, since
another process's CPU cannot be read there.

### `governor.calibrate {confirm?}`

Admin scope. Measures this machine again, in the background, and answers
`{started}` at once; `governor.status` says `calibrating` until it is done.
While anything is on air it is refused with `-32001`, `data.on_air`, and a
`retry` action whose `value` is `{"confirm": true}`: calibrating takes every
core for a few seconds and can cost what is on air frames.

### Events

| Event | Payload | When |
|---|---|---|
| `event/rendition.plan` | `{scope, plan}` | an output with a rendition is added, changed or removed, or the governor stops or brings back an encoder |
| `event/governor.shed` | `{what, why}` | something was stopped to keep what is on air whole |

Each shed also raises an `event/alert` at warning with the same sentence,
and coming back raises one at info.

## In the core

`crates/godwinmix-core/src/render/` builds the plan off the raw programme
tees. The programme is a raw source (`encoded: false`), so every rendition
is an encode.

| Node | Elements |
|---|---|
| `source` | none in system memory: the raw programme tee is the outlet. On a GPU canvas, one download and convert, shared by every rendition |
| `scale` | `videoscale`, `videorate` (`skip-to-first`), a caps filter at the size and rate |
| `encode` | `videoconvert`, the catalogue entry's encoder with its properties and keyframe rule, its parser with SPS and PPS before every keyframe |
| `audio-convert` | `audioconvert`, `audioresample`, a caps filter |
| `audio-encode` | `audioconvert`, `audiorate`, the first installed catalogue encoder for the codec, its parser |
| `mux` | none: the output's own feed and muxer are the mux |

Every node starts with a leaky one second queue, so a node that falls
behind drops its own frames and never holds the tee above it, and ends in a
tee (its outlet) that lives as long as the node's id is in the plan. The
body under the outlet is what a replan restarts and what the governor sheds;
consumers stay linked to the outlet through both. The encoder property
table and keyframe rule are the catalogue's, applied by the same code as the
programme encoder, with the video held back by the programme's A/V offset.

Keyframes: every encode of the programme is asked for a keyframe on the first
frame at or past each multiple of the plan's interval of running time, by a
probe on its sink pad that pushes one force key unit event. The encoder's
own interval is set to twice the plan's, a ceiling it never reaches. Two
rungs started at different moments put their keyframes on the same frames.

Before any new or changed node starts, the governor is asked: an encode by
its shape (`admit_encode`, which can pick a slower software preset that
fits), anything else by the cost the plan gave it. A single rendition is its
output's rung 0 and is never shed; a ladder's lower rungs are shed first.
Once a tick the mixer acts on the governor's `Drop` steps and, when nothing
has been over the line for ten seconds, admits and restarts what it shed.
A faster preset step is logged and not applied while running.

Everything runs on the mixer thread, as `OutputSlot` does; the only work on
a streaming thread is the keyframe probe.

### For a consumer of a rung (HLS)

`Tap` is one rung: `request`, `rung` (0 is the top), `video` and `audio`
shapes, `keyframe_ms`, and the `video_tee` and `audio_tee` it leaves by.
`OutputCtx::taps` hands an output's kind every rung when it builds, top
first; the generic feed already reads rung 0. `Mixer::rendition_taps(id)`
gives the same list. `Tap::feed(name, queue_secs)` puts a leaky queue and a
`proxysink` on a rung's tees in the programme pipeline and returns them as a
`Feed`, which `Feed::detach` takes out again.

## The governor in the station

The binary starts one `Station` with the core: it loads this machine's
calibration from `<runtime dir>/governor/` when there is one for its
fingerprint, starts the load sampler, and when there is none measures in
the background once nothing is on air. `[hardware] encode` narrows the
encoders the planner and the calibration see, exactly as it narrows the
programme encoder: `encode = "software"` is a machine with no GPU encoder.
`[governor] reserve_cores` is the Advanced override that keeps cores free.
The calibration candidates are `godwinmix_core::render::candidates`.

## Measured

On an M4 Pro (14 cores), a release build, a 1080p30 programme on a moving
test source, each destination an RTMP output to an ffmpeg listener on the
same machine, the mixer's own CPU averaged over 20 s. Other work was running
on the machine at the time (a load average near 12), so read the figures
against each other rather than as absolutes.

| Programme outputs | Mixer CPU |
|---|---|
| one output, no rendition, before this change | 0.221 and 0.229 cores |
| one output, no rendition, after | 0.225 and 0.216 cores |
| one `youtube-720p30`, VideoToolbox | 0.263 cores |
| four `youtube-720p30`: one scale, one encoder, one AAC encoder | 0.312 cores |
| 1080p, 720p, 480p, 360p, VideoToolbox | 0.379 cores |
| the same ladder, x264 (`[hardware] encode = "software"`) | 0.696 cores |

On the CPU only run with `[governor] reserve_cores = 7.2`, which left the
governor about 1.4 cores to give, the four rungs were admitted and a fifth
output asking for 1080p60 was refused: it needed 1.0 cores and 0.4 were
free, and the advice was 720p30 on x264. The first calibration on this
machine took 6.8 s, in the background, before anything was on air.

## How fast

A show of 16 sources and 64 requests, making about 200 nodes and 29
encoders, plans in about 130 microseconds in a release build on an M4 Pro,
and in about 400 in a debug build. A diff of two such plans takes about
30 microseconds. `cargo test -p godwinmix-render --release --test timing --
--nocapture` prints both, and the test fails over one millisecond.
