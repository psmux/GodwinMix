# udp

MPEG-TS over UDP, in and out, unicast and multicast.

This is how a broadcast plant moves pictures around: an IRD, a satellite
receiver or an encoder puts MPEG-TS on a multicast group, and whatever wants
it joins the group. `udp/source` joins (or listens on a port), takes one
program out of the transport stream, and hands it to the mixer. `udp/output`
puts the programme on a group, or sends it to one receiver, with a TTL, the
interface of your choice and an optional constant bitrate.

How to use it from the page is
[docs/how-to/udp-and-multicast.md](../../docs/how-to/udp-and-multicast.md).
Every setting is in [docs/reference/udp.md](../../docs/reference/udp.md).

## Build and install

```sh
./build                          # stage bin/gmx-udp
gmx plugin add ./plugins/udp     # runs ./build itself when bin/ is empty
```

## What the source does with a real feed

* Joins the group on the interface you name, source specific when you name a
  sender. Nothing is bound until the source starts.
* Tells bare TS and RTP apart by the first byte of each datagram, so
  `udp://` and `rtp://` both work whatever the sender does.
* Drops null packets before the core sees them.
* Reads the PAT, the PMTs and the SDT, and with several programs passes only
  the chosen one, with a PAT (and, when PIDs are named, a PMT) rewritten to
  match. With one program it passes the datagrams untouched.
* Counts lost packets from the continuity counters and the RTP sequence
  numbers, and never waits for them.
* Keeps listening when the sender stops, and resets its counters after a
  second of silence so a restarted sender is not counted as loss.

The MPEG-TS goes to the core on stdout, and the core decodes it once, the
same as `srt/source`. Nothing in this plugin decodes anything.

## Measured

On an Apple M4 Pro, macOS, GStreamer 1.28.7, release build:

| What | Result |
|---|---|
| 1080p30 H.264 at 8.8 Mbit/s from ffmpeg (`-f mpegts udp://239.1.1.1:19471?pkt_size=1316`), 60 s, received by the core's source and a second copy of the plugin on the same group | 48,417 datagrams, 0 packets lost; 1.6% of one core and 16 MB for each receiver |
| The same file sent raw by `tests/lossy_send.py` with 1% of datagrams dropped, 30 s | 252 datagrams dropped, 1,701 TS packets counted lost (the rest were null packets); the source stayed live, its picture never idle more than 150 ms; 2.3% of one core |
| The same inside RTP, 1% dropped | 297 datagrams dropped, 297 RTP datagrams counted lost; live throughout |
| The sender stopped for five seconds and started again | the core marked the source stalled, then live again by itself |
| `udp/output` from the core to ffmpeg, 10 s | 263 frames, no timestamp gaps, no decode errors after the first keyframe |
| `udp/output` fed a known encode, received by ffmpeg | every decoded frame identical to the encoder's own (framemd5), 150 of 150 |
| `udp/output` at a constant 8000 kbit/s | 8.03 Mbit/s over ten seconds, 1316 byte datagrams, 19.9% null packets |

`tests/drive.py` stands in for the core and prints the plugin's own counters,
CPU and memory while it runs; `tests/lossy_send.py` is the lossy sender. Both
are standard library Python.

## Tests

```sh
cargo test -p gmx-udp                    # unit tests and real sockets on this machine
gmx plugin test plugins/udp --offline    # tests/transcript.jsonl, no core
```

The socket tests send real H.264 from `gst-launch-1.0` and check frames with
`ffmpeg`, and skip with a line saying so when either is missing.

## Not here yet

* `udp/output` does not run on Windows: the core gives a sidecar output the
  programme on a FIFO, and Windows has none.
* FEC (SMPTE 2022-1) is not read. The FEC columns and rows arrive on other
  ports and are ignored.
* The constant bitrate mode caps the sending rate; it does not clock every
  packet out at an exact spacing.
* The page does not yet show the program list as a menu. It is in the source's
  health line and in the `programs` call.
