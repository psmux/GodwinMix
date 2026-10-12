# Why GodwinMix uses FFmpeg as little as it can

GodwinMix never runs the `ffmpeg` program. FFmpeg's libraries used to come in
anyway, through GStreamer's gst-libav plugin: `avenc_aac` encoded the
programme's sound on every machine without fdk-aac, and `avdec_h264` and
`avdec_aac` decoded most sources. Since 0.3.5 gst-libav is a fallback, and a
machine without it mixes, streams and records.

## Why bother

Three reasons, in the order they matter to the people running the mixer.

FFmpeg is a large library with a large attack surface, and a mixer opens
streams from the network. Every codec it does not need is code that can still
be reached by a crafted stream.

It is also the heaviest thing a distribution's GStreamer pulls in. On Linux
the shared FFmpeg links speech synthesis, SVG, libplacebo and x265; the
AppImage builds a private static FFmpeg (`dev/build-linux-libav.sh`) to avoid
carrying all of that. A Docker image or a Raspberry Pi with no gst-libav at
all is smaller again.

And its licensing depends on how it was built. A libav built against x264 is
GPL, which is what Homebrew and most distributions ship, so a build that
wants to stay permissive has to know which of its elements are FFmpeg's.

## How it works

The codec catalogue (`codecs.toml`) decides, as it decides everything else
about codecs. Every libav element sits in an entry of its own at rank 30 or
100, under every alternative, so selection only reaches it for a codec
nothing else on the machine handles.

Selection covers the programme's encoder and the channel transcoder. Sources
are opened by `decodebin`, which picks a decoder by GStreamer's own ranks, and
gst-libav registers `avdec_h264` and `avdec_aac` at primary, over openh264,
faad and the operating system's AAC decoders. So when the mixer starts it
swaps ranks: for each codec where the catalogue prefers a software decoder to
a libav one, the preferred decoder takes libav's GStreamer rank and libav's
drops one under it (`crates/godwinmix-core/src/catalogue/libav.rs`). Hardware
decoders are not touched, so a GPU decoder that beat libav still wins. libav
stays pluggable, so a stream the preferred decoder's caps refuse still opens.

## What each codec uses now

| codec | encode | decode, after hardware |
|---|---|---|
| H.264 | hardware, then `x264enc`, then `openh264enc` | `openh264dec`, then `avdec_h264` |
| HEVC | hardware, then `x265enc` | `d3d11h265dec` on Windows, `libde265dec`, then `avdec_h265` |
| AV1 | hardware, then `svtav1enc` | `dav1ddec` |
| AAC | `fdkaacenc`, `atenc` (macOS), `mfaacenc` (Windows), `voaacenc`, then `avenc_aac` | `fdkaacdec`, `atdec`, `mfaacdec`, `faad`, then `avdec_aac` |
| MP2 and MP3 | none needed | `mpg123audiodec` |
| AC-3 | none needed | `a52dec`, then `avdec_ac3` |

## What each release carries

| build | AAC encoder | AAC decoder | software H.264 decoder | HEVC decoder | needs libav for |
|---|---|---|---|---|---|
| Windows installer | `mfaacenc` | `mfaacdec` | `openh264dec` | `d3d11h265dec` | E-AC-3, HEVC on a GPU with no HEVC decode, ProRes and QuickTime Animation clips |
| macOS app | `atenc` | `atdec` | none (VideoToolbox decodes) | VideoToolbox | AC-3, E-AC-3, ProRes clips; H.264 on a Mac whose VideoToolbox refuses a stream |
| Linux AppImage | `voaacenc` | `faad` | `openh264dec` | `libde265dec` | E-AC-3, ProRes clips |
| Linux `.deb` | `voaacenc` | `faad` | `openh264dec` | `libde265dec` | the same; gst-libav is a recommended package, not a dependency |
| Docker image | `voaacenc` | `faad` | `openh264dec` | `libde265dec` | the same; `--build-arg WITH_LIBAV=0` leaves it out |

The macOS app is trimmed from Homebrew's GStreamer, which has no openh264, no
voaacenc and no a52dec. Every release still carries gst-libav as the
fallback; the table says what it is still there for.

`fdkaacenc` is the best AAC encoder of the lot, and none of the releases
carries it. The Fraunhofer licence allows redistribution, but it grants no
patent licence and is incompatible with the GPL, and the same runtimes carry
x264 and faad. An operator who installs it (`gstreamer1.0-fdkaac` on Ubuntu
from multiverse, on Debian from contrib with non-free enabled) gets it with
no configuration.

## What it costs

These are the trade-offs, measured on a Windows laptop with GStreamer 1.28.6.

`voaacenc` is the weakest AAC encoder of the four. It is clean at 128 kbit/s
and up for a stream, and it is only chosen where neither fdk-aac nor the
operating system's encoder exists, which in practice means Linux.

`openh264dec` decodes on one thread. A 1080p30 high profile stream of 600
frames cost 6.9 s of CPU against 4.6 s for `avdec_h264`, and took 7.1 s of
wall time against 0.9 s. It also does not decode interlaced H.264, which a
broadcast feed can be. A machine decoding several cameras on the CPU, or
taking interlaced feeds, should put libav back in front:
`GMX_CODEC_RANK=h264-software-decode=70`.

Each AAC encoder leaves a different amount of delay in front of the sound,
and the mixer holds the video back by the catalogue's `priming_delay_ms` so
lips stay on the sound. Those figures are measured by a test on every CI
platform (`catalogue::priming_tests`), which fails when a figure is off by
more than 4 ms.

Decoders differ on the same point. faad drops the first 1024 samples of a
stream and `avdec_aac` does not, so an AAC source decoded by faad, which is
the Linux default now, has its sound 21 ms earlier against its picture than
it had through libav. For a source whose encoder left the usual 1024 samples
of delay in, that is the sound back where it was recorded.

## What still needs libav

E-AC-3 has no decoder outside it. Neither do ProRes and QuickTime Animation
clips with alpha (`avdec_prores`, `avdec_qtrle`). HEVC in software needs
libde265, which only Debian and Ubuntu package. These are listed by
`gmx doctor` on the machine in front of you.
