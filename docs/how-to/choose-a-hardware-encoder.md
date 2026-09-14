# Choose a hardware encoder

The mixer picks its codecs at startup and works with or without a GPU. This
page is about when to override that choice, and why you probably should on a
machine you control.

## See what it picked

```sh
godwinmix --probe
docker exec godwinmix godwinmix --probe     # in a container
```

That prints the decoder and the encoder it would use on this machine, for video
and audio, and whether the path is hardware accelerated.

## What it can use

| Backend | Decode | Encode | Where |
|---|---|---|---|
| `nvidia` | `nvh264dec` | `nvh264enc` | an NVIDIA card with the driver installed |
| `va` | `vah264dec` | `vah264enc` | Intel Quick Sync, AMD, on Linux |
| `d3d11` | `d3d11h264dec` | | Windows |
| `mediafoundation` | | `mfh264enc` | Windows |
| `videotoolbox` | `vtdec_hw` | `vtenc_h264_hw` | macOS |
| `software` | `avdec_h264` | `x264enc` | everywhere |

Decode and encode are chosen independently, because a machine with NVDEC and no
usable NVENC is a real configuration and not a rare one. Consumer NVIDIA cards
also cap how many NVENC sessions run at once, which is a reason to decode on
the GPU and encode on the CPU rather than the other way round.

There is no V4L2 entry yet, so a Raspberry Pi 4's hardware encoder is not used.
That is on the roadmap. See [Run it on a Raspberry Pi](raspberry-pi.md).

## Pin it on a server you control

```toml
[hardware]
decode = "auto"
encode = "nvidia"
```

`auto` picks the best backend present, which is right on a laptop and wrong on
a server. Naming a backend makes startup fail loudly when it is missing.
Without that, a driver upgrade that breaks NVENC turns into a mixer that
silently falls back to x264, takes two cores it does not have, and starts
dropping frames on air, with nothing in the log that looks like a problem.

Fail at start, in the open, rather than degrade in the middle of a service.

## Which one to pick

* **An NVIDIA card in the box**: `nvidia`. NVENC costs almost no CPU, which
  leaves the cores for the compositor, and the compositor is what falls over
  first on a small machine.
* **An Intel N100 or any recent Intel or AMD on Linux**: `va`. Quick Sync at
  6 W is the sweet spot for a rack in a church.
* **A Mac**: `videotoolbox`.
* **Windows**: `mediafoundation` to encode, `d3d11` to decode. Note that
  `wasapi2sink` and `mfvideosrc` have open upstream bugs; if audio stutters or
  a capture device will not start, that is why.
* **No GPU, or a GPU whose driver you do not trust**: `software`. x264 at
  `veryfast` costs about 0.4 of a core at 720p30. Two live sources plus the
  compositor plus the encoder is about 1.5 cores. That is a real deployment,
  not a fallback, and it is the one the CI tests run on so it never rots.

## Check it took effect

```sh
gmx ctl status | jq .backend
```

```json
{
  "video_decoder": "nvh264dec",
  "video_encoder": "nvh264enc",
  "audio_decoder": "avdec_aac",
  "audio_encoder": "avenc_aac",
  "hardware_accelerated": true
}
```

`hardware_accelerated: false` when you asked for a hardware backend means it
was not found and startup should have failed. If it did not, that is a bug
worth an issue.

## What it costs to get this wrong

The encoder is the one thing in the programme pipeline with a deadline. If it
misses, the compositor's queue fills, and the mixer's design is to leak frames
downstream rather than let anything apply backpressure to the programme. So a
badly chosen encoder does not produce an error. It produces a stream that looks
slightly wrong to viewers and normal to you.

`gmx bench`, which will print the cost of each preset on the reference
machines, is planned. Until then the answer is to pin the backend, watch the
frame interval, and not stream at 1080p60 on a box you have not tested.

## A note on properties

Every encoder property the mixer sets is set defensively. Backends disagree
about names, units and integer widths (`bitrate` is bits per second on one and
kilobits on another, guint on one and gint on the next), and a property that
does not exist on the chosen backend is a logged warning rather than a crash.
If a bitrate you set is being ignored, look in the log for that warning before
looking anywhere else.
