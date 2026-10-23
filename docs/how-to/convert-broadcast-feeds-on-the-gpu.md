# Convert broadcast feeds on the GPU

A broadcast multiplex carries MPEG-2 video and AC-3 sound, often two or three
sound tracks to a programme, some in 5.1. Most players want H.264 and AAC.
This page is about a direct show that converts such a feed on an NVIDIA card:
the request that asks for it, what the machine needs, and how to check that
every channel landed on the GPU.

## Ask for the rendition

Give the show's output a rendition like this one, which is what a headend
of fifty channels at 720p asks for:

```json
{"video": {"codec": "h264", "width": 1280, "height": 720, "bitrate_kbps": 1500,
           "keyframe_frames": 50, "fit": "pad", "pad_color": "#000000",
           "deinterlace": "off", "preset": "p1"},
 "audio": {"codec": "aac", "bitrate_kbps": 128, "tracks": "all"}}
```

* `keyframe_frames` counts frames at the source's own rate, so 50 is a
  keyframe every 1.67 s on a 29.97 feed and every 0.83 s on a 59.94 one.
* `fit: pad` keeps the whole picture and adds black bars. `crop` fills the
  frame and cuts what overhangs; `stretch` changes the picture's shape.
* `deinterlace: off` scales an interlaced picture as the frame it is, both
  fields woven together. `on` and `auto` deinterlace on the CPU, since no
  CUDA deinterlacer is installed, and that takes the channel off the GPU for
  its scale.
* `preset` is the encoder's own word. On NVENC `p1` is the fastest, and the
  one a card carrying fifty channels needs: at the catalogue's `p4` an RTX
  3060 class card runs out of encoder well before that.
* `tracks: all` converts every sound track, each at its own channel count,
  so a 5.1 English track and a stereo Spanish one come out as 5.1 and stereo
  AAC.

The source's frame rate is kept: leave `fps` out.

## What the machine needs

* GStreamer's `nvcodec` plugin (`gstreamer1.0-plugins-bad` on Debian and
  Ubuntu) with an NVIDIA driver. `gst-inspect-1.0 nvcodec` lists
  `nvmpeg2videodec`, `nvh264dec`, `nvh264enc` and `cudaconvertscale`.
* The CUDA runtime compiler, which `cudaconvertscale` needs to build its
  kernels. GStreamer opens it as `libnvrtc.so`; Debian's `libnvrtc12`
  (non-free) installs `libnvrtc.so.12`, so link one to the other. Without
  it `gst-inspect-1.0 nvcodec` says `libnvrtc.so was not found`, there is
  no `cudaconvertscale`, and every picture goes through system memory.
* `a52dec` or `avdec_ac3` for the sound, and `avenc_aac`.
* On a consumer card that has its session limit lifted, tell the governor
  so it finds the real one:

```toml
[governor]
nvenc_session_probe = 128
```

## In a container, ask for the GPU through CDI

Started with `--gpus all` (or `runtime: nvidia`), a container loses the GPU
the first time systemd reloads its units, which a package refresh does on
its own: inside it `nvidia-smi` says `Failed to initialize NVML: Unknown
Error`, and every decoder and encoder after that fails. The NVIDIA hook adds
the GPU's devices behind Docker's back, and the reload puts back only what
Docker knows of.

Ask for it as a CDI device instead, which Docker 25 and later understand
and which the NVIDIA Container Toolkit describes in `/var/run/cdi/nvidia.yaml`
(its `nvidia-cdi-refresh` unit keeps that file current):

```sh
docker run --device nvidia.com/gpu=all ...
```

or in a compose file:

```yaml
services:
  headend:
    devices:
      - nvidia.com/gpu=all
```

A container started this way kept the GPU, and kept encoding without a lost
frame, through `systemctl daemon-reload`.

## What it builds

For each show, one decode on NVDEC, one scale and pad in CUDA memory, one
encode on NVENC, and one AC-3 decode and AAC encode per sound track. The
picture never leaves the card between the decode and the encode, and every
pipeline in the process shares one CUDA context, made so that a thread
waiting for the card sleeps rather than spins.

## Check it

`rendition.plan` for the show lists its nodes. Every `encode` names
`h264-nvidia`; a node that says `using h264-software-x264 because the GPU
nvidia is full` is a channel that did not fit.

`nvidia-smi dmon -s u` shows the encoder (`enc`) and decoder (`dec`) busy
share. Both near 100% means frames are being lost: the show's stats count
them in `dropped_gops`. With MPEG-2 the decoder fills first, before the
encoder, because each 1080i and 720p60 programme costs it several times what
it costs NVENC at 720p.

## When a sound track fails

An AAC encoder that refuses a frame stops its node. The node is built again
with what reads it, at most once every five seconds, and the station's log
says `transcode: building <node> again`.
