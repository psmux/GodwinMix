# rtsp

Serve the programme over RTSP.

A lot of broadcast and AV gear does not take a push: a hardware decoder behind
a projector, a network video recorder, a video wall controller, a monitoring
screen in VLC, another mixer's "RTSP source". Each of those wants an address
to pull from. `rtsp/output` gives them `rtsp://<this machine>:8554/live`.

The port is chosen when the output is added and is open only while the output
exists. Every player shares one packetiser over the programme's own encode, so
ten players cost ten network streams and no encoder.

How to use it from the page is
[docs/how-to/serve-rtsp.md](../../docs/how-to/serve-rtsp.md). Every setting is
in [docs/reference/plugins-network.md](../../docs/reference/plugins-network.md#rtspoutput).

## Build and install

```sh
./build                           # stage bin/gmx-rtsp
gmx plugin add ./plugins/rtsp     # runs ./build itself when bin/ is empty
```

## What you need

* Linux or macOS. The core hands an output plugin the programme on a FIFO,
  which Windows does not have.
* GStreamer 1.24 or later with gst-rtsp-server (Debian and Ubuntu:
  `libgstrtspserver-1.0-0`; Homebrew's `gstreamer` carries it).

## Tested

`cargo test -p gmx-rtsp` feeds the output the way the core does (live H.264
and AAC in streamable Matroska, on a FIFO) and pulls it three ways: ffmpeg over
TCP, ffmpeg over UDP, and `uridecodebin`, which is what the core itself uses
for an `rtsp://` source. The ffmpeg runs compare every frame's hash with the
encoder's own output: the frames that arrive are the encoder's, in order,
with none missing.
