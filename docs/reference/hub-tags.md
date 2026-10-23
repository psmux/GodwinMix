# Reference: hub tags

Everything the ingest plugin carries between an input and its readers is a
`MediaTag` (`plugins/ingest/src/media_tag.rs`): an FLV tag body with its
kind, time and flags. This page is the part a new input or a new reader must
agree on.

## The fields

| Field | Meaning |
|---|---|
| `track` | 0 for the video and the first sound. Further sound tracks are 1, 2, 3, in PMT order |
| `kind` | `Audio`, `Video` or `Script` |
| `timestamp_ms` | decode time in ms on the session's timeline; every track of one session shares its zero |
| `keyframe` | a video tag a decoder can start at |
| `sequence_header` | a codec configuration a late reader is given first (AVC, HEVC, AV1, AAC) |
| `payload` | the body, shared by every reader, never copied |

## Who gets which track

| Reader | Tracks |
|---|---|
| `Hub::subscribe` (every output, every channel destination) | 0 alone, byte for byte as before tracks existed |
| `Hub::subscribe_all` (the transcoder) | every track |
| the relay, `GMXHUB <app>/<stream>` | 0 alone |
| the relay, `GMXHUB <app>/<stream> tracks=all` | every track, each tag's `track` in the last byte of its FLV stream id |

A further track that is AAC sends a sequence header tag of its own, with its
track number. The hub keeps one per track for readers that join late.

## Video bodies

H.264 is classic FLV. HEVC and AV1 are enhanced RTMP (`hvc1`, `av01`).
MPEG-2 is ours, FourCC `mp2v`, and never leaves this process: a copy output
that meets it sends the sound alone and says it needs a rendition.

| Bytes | `mp2v` |
|---|---|
| 0 | `0x80 \| frame type << 4 \| 1`: frame type 1 for a keyframe, 2 for any other picture |
| 1 to 4 | `mp2v` |
| 5 to 7 | composition offset, PTS less DTS, in ms, signed 24 bit big endian, always present |
| 8 on | one coded picture as it was in the elementary stream: the sequence header, sequence extension, GOP header and user data in front of it, the picture header, its extensions and every slice |

A picture is a keyframe when a sequence header is in front of it. There is
no sequence start tag: each keyframe says the size, frame rate and scan,
which `crate::mp2v::sequence` reads. `crate::mp2v::tag` makes one.

## Sound bodies

AAC is classic FLV. AC-3, E-AC-3 and MPEG audio layer II are enhanced RTMP
audio (`ac-3`, `ec-3`, `.mp3`), one frame a tag, with no header tag. MP3 is
classic FLV.

## What an input says of its tracks

An input fills `InputStats.tracks`, one `TrackInfo {track, kind, pid, lang}`
per stream it carries. The host puts those beside what the hub measured of
each track and sends them as `direct.input`'s `tracks`
([direct shows](direct-shows.md)). An input carries MPEG-2 and the further
sound tracks only when its `InputSpec.carry` says so, which the host sets
from the show's outputs.
