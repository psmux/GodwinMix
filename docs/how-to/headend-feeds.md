# Take headend feeds into direct shows

A headend has many feeds and wants each one passed on, not mixed: a
multicast program out to an RTMP platform, an SRT contribution feed into a
recorder, a camera's RTSP onto a CDN. In GodwinMix each of those is a direct
show, a show with compositing off. It takes one input and copies it to its
outputs without decoding it. Taking a 1080p feed at 8 Mbit/s in costs one to
three percent of one core, depending on the transport.

This page is about the input: which address to give for each kind of feed,
how to pick one program out of a multiplex, how to give a feed a backup,
and how to read the numbers it reports. Making the shows themselves, one at
a time or two hundred at once, is on the monitoring wall and in
`show.add_many`; neither is in this branch yet, so the examples below show
the `input` each one takes.

## Pick the address

Paste the address the way the encoder, the IRD or the camera's manual writes
it. The scheme says what kind of feed it is.

| The feed | Its address |
|---|---|
| A multicast group from an IRD or encoder | `udp://@239.1.1.1:5000` |
| The same, source specific | `udp://10.0.0.9@232.1.1.1:5000` |
| MPEG-TS sent to this machine's port | `udp://0.0.0.0:5000` |
| RTP wrapped TS (SMPTE 2022-2) | `rtp://@239.1.1.1:5000`. A `udp://` address works too; each packet says which it is |
| An SRT encoder that listens, which you dial | `srt://10.0.0.9:9000` |
| An SRT encoder that calls you | `srt://@:9000` |
| RIST | `rist://@0.0.0.0:5004` (an even port) |
| An IP camera or an encoder's RTSP | `rtsp://user:password@10.0.0.20/stream1` |
| HLS or DASH from a CDN | `https://cdn.example.com/live/master.m3u8`, or `.mpd` |
| Someone else's RTMP server | `rtmp://host/app/streamkey` |
| A clip to loop, for testing or playout | `file:///clips/loop.ts` (TS or MP4) |
| A stream already coming into one of your channels | `channel:church/main` |

The camera's password is never shown back in a status or an error; it
reads `user:***@10.0.0.20`.

### Multicast on the right network card

A headend machine usually has one card for the plant and one for the
office. Name the plant one in `params.interface`, so the group is joined
there and not on whichever card the default route uses:

```json
{"uri": "udp://@239.1.1.1:5000", "params": {"interface": "en1"}}
```

### SRT with a passphrase

```json
{"uri": "srt://10.0.0.9:9000", "params": {"passphrase": "the shared secret", "latency_ms": 500}}
```

Raise `latency_ms` on a long or lossy path: it is how long SRT waits to
recover a lost packet before giving up on it.

### RTSP over TCP

A camera across a firewall or a NAT usually needs TCP:

```json
{"uri": "rtsp://admin:pass@10.0.0.20/stream1", "params": {"transport": "tcp"}}
```

## Take one program out of a multiplex

A satellite or cable multiplex carries several programs. Give the input any
program number first, or none, and read `programs` in its numbers: every
program in the feed by number, with its name and provider from the SDT and
what each one carries.

```json
"programs": [{"name": "Sport", "number": 1, "provider": "FFmpeg", "streams": ["h264", "aac"]},
             {"name": "News", "number": 2, "provider": "FFmpeg", "streams": ["h264", "mpeg audio"]}]
```

Then set `program` to the one you want:

```json
{"uri": "udp://@239.1.1.1:5000", "program": 2}
```

Everything else in the multiplex is dropped as it arrives, before anything
reads it.

## Give a feed a backup

Name a second input as `backup`. Both run all the time, so the backup is
already up when it is needed:

```json
{"uri": "srt://10.0.0.9:9000",
 "params": {"stall_ms": 2000, "return_ms": 5000},
 "backup": {"uri": "udp://@239.1.1.1:5000"}}
```

When the main sends nothing for two seconds the show cuts to the backup at
its next keyframe, and when the main has been steady for five it cuts back
the same way. The outputs see one unbroken stream. While the backup is on,
the input's `error` starts with `on the backup input:` and says what is
wrong with the main.

## Read the numbers

Each input reports about once a second:

* `state` is `connecting`, `live` or `retrying`. A feed that goes quiet is
  `retrying` within three seconds, with `error` saying how long it has been.
* `kbps`, `fps`, `width`, `height`, the codecs and the audio channel count
  are what is arriving now. For a TS feed `kbps` is the chosen program,
  without the multiplex's stuffing.
* `cc_errors` counts continuity errors on the program, one per jump, and
  `packets_lost` what the transport knows went missing. Both keep counting
  across reconnects, so a number that climbs is a path that is losing
  packets. The picture carries on through loss: a lost packet costs the
  frames it was part of and nothing more.
* `keyframe_ms` is the keyframe interval. A contribution encoder set to two
  seconds should read about 2000.
* `last_frame_ms` is how long since the last frame.

`docs/reference/direct-inputs.md` has every field and param.

## What can go through, and what cannot

The input passes H.264 and HEVC video, and AAC, AC-3 (5.1 included),
E-AC-3, MPEG layer II and MP3 sound, on untouched. Whether an output can send
AC-3 or layer II as they are depends on where it sends: an RTMP platform
takes AAC and nothing else. One video and one audio stream are
taken from each feed: a second language, teletext and subtitles are left
out, and so is MPEG-2 video, which nothing downstream of a direct show
carries. `error` names what was left out once the feed is live.

The silence alarm listens to all of those sound codecs. Before this release a
direct show with layer II, MP3, AC-3 or E-AC-3 sound never had its sound
measured, so a feed whose sound had really gone quiet raised nothing.
