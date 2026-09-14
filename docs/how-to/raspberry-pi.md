# Run it on a Raspberry Pi

A Pi is a target machine for this project, not a curiosity. A church or a
school that can spend fifty pounds on a mixer can put one in a rack and leave
it there.

Read the next paragraph before you buy a board, because the newer one is the
harder target.

## Which board

| Board | Hardware H.264 encoder | What that means |
|---|---|---|
| Pi 4 Model B, 2 GB or more | yes, `v4l2h264enc`, up to 1080p30 | the cheapest board that can work, once the mixer can use it |
| Pi 5, 4 GB | **none at all** | faster CPU, and x264 in software competes with the compositor for the same four cores |
| Pi Zero, Pi 3 | no | not enough |

The Pi 5 dropped the hardware video encoder the Pi 4 has. It decodes HEVC in
hardware and encodes nothing. For a mixer, which encodes continuously, that
makes the older and cheaper board the better one.

**Today the mixer encodes in software on both boards.** The backends it knows
are NVIDIA, VA, VideoToolbox, Media Foundation, D3D11 and software
(`godwinmix --probe` prints what it picked). There is no V4L2 entry yet, so a
Pi 4's hardware encoder sits idle. Adding it is on the roadmap and it is the
single change that makes a 2 GB Pi 4 a comfortable 1080p30 mixer instead of a
strained 720p30 one. Until it lands, plan for 720p30 in software on either
board.

## Install

64 bit Raspberry Pi OS (Debian based). Docker works and is the least trouble:

```sh
curl -fsSL https://get.docker.com | sh
sudo usermod -aG docker "$USER"   # log out and back in

docker run -d --name godwinmix \
  --restart unless-stopped \
  -p 127.0.0.1:8080:8080 \
  -e GODWINMIX_TOKEN=change-me \
  ghcr.io/psmux/godwinmix:latest
```

No `--device` line, because nothing in the mixer opens a V4L2 encoder yet. When
the V4L2 backend lands, a Pi 4 will want `--device /dev/video11:/dev/video11`
and the container user in the `video` group.

The published image is `linux/amd64` today; a multi architecture image is
planned. Until it exists, build it on the Pi (about 40 minutes) or with
`docker buildx build --platform linux/arm64`. CI does cross build the
`linux-aarch64` binaries on every push to main, and they are attached to every
release, so the systemd install is the quicker path on a Pi right now.

## Configure it for the board

```toml
[canvas]
width = 1280
height = 720
fps = 30

[program]
# 2,500 kbit/s at 720p30 is the default for a reason: most of the uplinks this
# runs on cannot hold the 6,000 YouTube asks for at that size.
video_bitrate_kbps = 2500
audio_bitrate_kbps = 128

[multiview]
# The mosaic is a second encoder. On a Pi it is the difference between working
# and not. Turn it off if you drive the mixer from a script rather than from
# the UI; it costs nothing while no client is watching, but it costs when one
# is.
enabled = true
width = 640
height = 360
fps = 5

[hardware]
decode = "auto"
# On a server you control, name the backend instead of leaving it "auto", so a
# missing driver fails at startup rather than quietly costing you two cores.
# On a Pi today that means "software", because there is no V4L2 backend yet.
encode = "software"
```

Set the canvas before you go live. It cannot change while a broadcast is
running, because the output encoder is started once and never restarted.

## What to expect

No measured numbers are published yet. The budgets the project is holding
itself to are in
[footprint budgets](../explanation/footprint-budgets.md), they are targets
rather than measurements, and they are marked as such. What is known from the
shape of the thing:

* Compositing and colour conversion cost about as much as the encoder. On a
  small board the mixer's own hot path falls over before the encoder does.
* Every source pays for its decode all the time it exists, whether or not it is
  on programme. Two sources on a Pi 4 at 720p30 is a sensible ceiling; the
  `idle` capability that parks a source in no scene is planned and will change
  that.
* A bare GStreamer process costs about 41 MB of memory before it does anything.

Measure your own with `top` and the mixer's log, and if your numbers differ
from what you expected, that is worth an issue.

## Things that bite on a Pi

* **Power.** An undervolting Pi throttles and your frame rate falls with it.
  Use the official supply. `vcgencmd get_throttled` should print `0x0`.
* **The SD card.** Recording to it will stall. Record to USB storage or not at
  all.
* **Heat.** A mixer runs the CPU at 100 percent for hours. Fit a heatsink and a
  fan; a throttled Pi drops frames and nothing in the log says "I am hot".
* **Wi-Fi.** An RTMP output over Wi-Fi is a reconnect waiting to happen. The
  outage buffer covers a hiccup, not a dead link. Use Ethernet.
* **A web page source.** WebKit on a Pi is slow enough that a page with video in
  it will not keep up. Add the video's own address as a source instead, which
  is cheaper and looks better.
