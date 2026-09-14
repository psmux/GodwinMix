# How to choose a hardware encoder

GodwinMix picks an encoder for you at startup and it usually picks right. This
page is for the times it does not, or for the hour before a show when you want
to know what it picked and whether that choice has ever been tested on the
machine you are standing in front of.

## Find out what this machine will do

```
godwinmix --probe
```

It prints every entry in the catalogue, whether the elements are installed, and
which one won. The arrow is the entry that was chosen.

```
video.encode
     h264-nvidia                accel nvidia          rank 240   nvh264enc     missing nvh264enc
     h264-va                    accel va              rank 225   vah264enc     missing vah264enc
  -> h264-videotoolbox          accel videotoolbox    rank 215   vtenc_h264_hw installed
     h264-software-x264         accel software        rank 72    x264enc       installed

chosen
  video encoder : vtenc_h264_hw (videotoolbox, entry h264-videotoolbox, rank 215)
  graphics      : software (software, compositor + videoconvert, memory system)
    why         : highest ranked entry present; GPU entries are taken only once verified here
```

Rank decides. The highest ranked entry whose elements are actually in the
GStreamer registry wins. Decode and encode are decided separately, so a machine
with a working NVDEC and an NVENC that refuses to license gets the fast decoder
and the software encoder, which is the right answer and is a real configuration.

`gmx codec list` prints the same catalogue as a table, with the licence and
whether anybody has verified each entry.

## Find out whether it actually works

A driver that loads is not a driver that encodes. The element builds, the
pipeline reaches PLAYING, and then either nothing comes out or what comes out is
green. Ten seconds through the real encoder and back through the real decoder
tells you:

```
gmx codec test h264-videotoolbox
```

```
h264-videotoolbox  vtenc_h264_hw -> vtdec_hw
  1280x720@30 for 10 s
  frames    300 in, 300 out
  psnr      33.0 dB average, 32.7 dB worst frame
  cpu       2.02 s for 10 s of media, 0.20 of one core at real time
  wall      1.7 s (the test runs as fast as it can, not in real time)
  rss       162 MB peak, whole process
  result    pass
```

Read it in this order. Frames out should equal frames in; an encoder that drops
a tenth of them still produces a valid stream and a stuttering programme. PSNR
in the thirties is a normal compressed picture at live bitrates; under thirty
means the picture that came back is not the one that went in. The CPU figure is
against the media duration, so 0.20 means a fifth of one core to encode in real
time, which is the number to compare with your budget.

The last line it prints is a `verified` record ready to paste into the entry in
`codecs.toml`. Fill in your driver version and your name and send it as a pull
request. That is how entries get verified for hardware the project does not own.

`gmx doctor` runs a one second version of the same thing for every entry whose
elements exist, which is the check to run on a new box before pointing cameras
at it.

## Pin one, when you need to

In your configuration:

```toml
[hardware]
encode = "nvidia"
decode = "nvidia"
```

A pinned backend that is not installed is a startup failure with the list of
what was available, not a quiet fallback:

```
no video.encode entry for accel "nvidia" is installed. Set [hardware] back to
"auto", or install the elements. The entries the catalogue knows for this role:
  h264-nvidia                accel nvidia           rank 240  missing nvh264enc
  h264-videotoolbox          accel videotoolbox     rank 215  installed
  h264-software-x264         accel software         rank 72   installed
```

That is deliberate. An operator who pinned NVENC wants to hear that the driver
did not come back after the last reboot, not to find out from the CPU graph
halfway through a show.

The accel names are the ones in the table: `nvidia`, `va`, `qsv`, `amf`,
`videotoolbox`, `mediafoundation`, `d3d11`, `d3d12`, `v4l2`, `vulkan`,
`software`, and `auto`, which is the default.

## When the GPU misbehaves during a show

```toml
[hardware]
encode = "software"
```

and restart. The software entries are always present, are tested on a GPU free
runner on every commit, and are what a machine with no GPU gets anyway. On a
1080p30 programme expect roughly one core; the mixer says so in the log when it
selects one.

## A machine with no GPU

Nothing to do. The software entry is the floor and it always resolves.

Which software encoder you get depends on what is installed:

* `x264enc` (rank 72) where `gst-plugins-ugly` is installed. Faster and better
  looking, and GPL, which is why GodwinMix never bundles it: you install it.
* `openh264enc` (rank 64) otherwise. Cisco's encoder, BSD licensed, ships in
  every build including a closed custom one.

Either way the decoder is `avdec_h264` from gst-libav, or `openh264dec`.

## Graphics: the compositor is a separate choice

Compositing on the CPU costs about as much as the encoder. GodwinMix can
composite on the GPU instead, and the catalogue chooses that backend the same
way it chooses a codec:

```toml
[hardware]
graphics = "gl"
```

The software compositor stays the default even on a machine that has the GPU
elements, and `--probe` says so in as many words: "present but not verified on
macos-aarch64". That is not caution for its own sake. Every GPU compositor in
GStreamer 1.28 is rank none, meaning GStreamer itself will not auto select it,
and `glvideomixer` has open bugs on dynamic pad add and remove, which is what a
mixer does every time a source arrives or leaves. The D3D12 elements leak on
pipeline rebuild.

So a GPU graphics entry is taken automatically only once somebody has run
`gmx codec test` plus a sixty minute add and remove soak on that platform and
written the result into the entry's `verified` list. Pinning it is how you run
that soak. If it works for you, send the record.

With a GPU entry the frame is uploaded once, stays in device memory through
conversion and compositing and into the encoder, and comes back to system
memory only for the multiview thumbnail and for a software encoder. On a Mac
the compositor is GL and VideoToolbox wants system memory, so there is one
download in front of the encoder; on an NVIDIA box with NVENC the frame never
comes down.

## The Raspberry Pi, honestly

| Board | Encode path |
|---|---|
| Raspberry Pi 4 | `v4l2h264enc`, hardware, entry `h264-v4l2` |
| Raspberry Pi 5 | software only |

The Pi 5 has no hardware video encoder. Broadcom removed it. A Pi 5 running
GodwinMix encodes the programme on the CPU, which works at 720p30 and is the
reason the older and slower board is the easier target for a mixer. If you are
buying a board to run this on and you want hardware encode, buy the Pi 4.

The `v4l2h264enc` entry sets no properties, because the Pi's driver carries
bitrate and GOP inside a V4L2 `extra-controls` structure rather than as plain
element properties. You get the driver's defaults.

## Related

* [add-a-codec-entry.md](add-a-codec-entry.md) for adding or overriding an entry.
* [../reference/codecs.md](../reference/codecs.md) for the whole shipped table.
