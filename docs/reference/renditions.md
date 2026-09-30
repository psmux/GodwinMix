# Reference: renditions and the planner

A rendition is one concrete encoded form of a picture and its sound, such as
H.264 1280x720 at 30 fps and 3000 kbit/s with AAC stereo at 128 kbit/s. An
output asks for one with a `RenditionRequest`; the planner in
`crates/godwinmix-render` turns every request in a show into the smallest
graph that serves them all.

Nothing in the core calls the planner yet. Outputs still build their own
encoders; wiring the plan into the core, and a page that shows it, is the
next piece of work.

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

## How fast

A show of 16 sources and 64 requests, making about 200 nodes and 29
encoders, plans in about 130 microseconds in a release build on an M4 Pro,
and in about 400 in a debug build. A diff of two such plans takes about
30 microseconds. `cargo test -p godwinmix-render --release --test timing --
--nocapture` prints both, and the test fails over one millisecond.
