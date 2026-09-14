# Reference: the shipped codec catalogue

Every entry in `codecs.toml` as it ships. The file itself is the authority;
this table is generated from it with `gmx codec list --json` and is here so
you can see what exists without a checkout.

To see what is installed on the machine in front of you, with the rank
decision that follows from it, run:

```
godwinmix --probe
gmx codec list
```

## Entries

`platform` is where the elements normally exist, not a guarantee. `verified`
says whether anybody has run `gmx codec test` on that entry and sent the
report; see [choose-a-hardware-encoder.md](../how-to/choose-a-hardware-encoder.md)
for what an unverified entry means in practice.

| entry | kind | codec | platform | elements | rank | license | verified |
|---|---|---|---|---|---|---|---|
| `h264-nvidia` | video | h264 | Linux, Windows | `nvh264enc` / `nvh264dec` | 240 | LGPL-2.1-or-later | no |
| `h265-nvidia` | video | h265 | Linux, Windows | `nvh265enc` / `nvh265dec` | 230 | LGPL-2.1-or-later | no |
| `av1-nvidia` | video | av1 | Linux, Windows | `nvav1enc` / `nvav1dec` | 220 | LGPL-2.1-or-later | no |
| `h264-va` | video | h264 | Linux | `vah264enc` / `vah264dec` | 225 | LGPL-2.1-or-later | no |
| `h265-va` | video | h265 | Linux | `vah265enc` / `vah265dec` | 215 | LGPL-2.1-or-later | no |
| `av1-va` | video | av1 | Linux | `vaav1enc` / `vaav1dec` | 205 | LGPL-2.1-or-later | no |
| `h264-va-legacy` | video | h264 | Linux | `vaapih264enc` / `vaapih264dec` | 150 | LGPL-2.1-or-later | no |
| `h264-qsv` | video | h264 | Windows | `qsvh264enc` / `qsvh264dec` | 220 | LGPL-2.1-or-later | no |
| `av1-qsv` | video | av1 | Windows | `qsvav1enc` / `qsvav1dec` | 200 | LGPL-2.1-or-later | no |
| `h264-amf` | video | h264 | Windows | `amfh264enc` / `d3d12h264dec` | 210 | LGPL-2.1-or-later | no |
| `av1-amf` | video | av1 | Windows | `amfav1enc` / `d3d12av1dec` | 195 | LGPL-2.1-or-later | no |
| `h264-videotoolbox` | video | h264 | macOS | `vtenc_h264_hw` / `vtdec_hw` | 215 | LGPL-2.1-or-later | no |
| `h265-videotoolbox` | video | h265 | macOS | `vtenc_h265_hw` / `vtdec_hw` | 205 | LGPL-2.1-or-later | no |
| `h264-videotoolbox-any` | video | h264 | macOS | `vtenc_h264` / `vtdec` | 140 | LGPL-2.1-or-later | no |
| `h264-mediafoundation` | video | h264 | Windows | `mfh264enc` / `d3d11h264dec` | 180 | LGPL-2.1-or-later | no |
| `h264-d3d11` | video | h264 | Windows | `d3d11h264dec` | 185 | LGPL-2.1-or-later | no |
| `h264-d3d12` | video | h264 | Windows | `d3d12h264dec` | 175 | LGPL-2.1-or-later | no |
| `h264-v4l2` | video | h264 | Pi 4 and V4L2 SoCs | `v4l2h264enc` / `v4l2h264dec` | 160 | LGPL-2.1-or-later | no |
| `h264-vulkan` | video | h264 | Linux, GStreamer 1.26+ | `vulkanh264enc` / `vulkanh264dec` | 20 | LGPL-2.1-or-later | no |
| `h264-software-x264` | video | h264 | everywhere | `x264enc` / `avdec_h264` | 72 | GPL-2.0-or-later | no |
| `h264-software-openh264` | video | h264 | everywhere | `openh264enc` / `openh264dec` | 64 | BSD-2-Clause | no |
| `h264-software-decode` | video | h264 | everywhere | `avdec_h264` | 70 | LGPL-2.1-or-later | no |
| `av1-software` | video | av1 | everywhere | `svtav1enc` / `dav1ddec` | 60 | BSD-3-Clause | no |
| `h265-software` | video | h265 | everywhere | `x265enc` / `avdec_h265` | 50 | GPL-2.0-or-later | no |
| `aac-fdk` | audio | aac | everywhere | `fdkaacenc` / `avdec_aac` | 120 | FDK-AAC | no |
| `aac-avenc` | audio | aac | everywhere | `avenc_aac` / `avdec_aac` | 100 | LGPL-2.1-or-later | no |
| `aac-vo` | audio | aac | everywhere | `voaacenc` / `avdec_aac` | 80 | Apache-2.0 | no |
| `aac-software-decode` | audio | aac | everywhere | `faad` | 90 | GPL-2.0-or-later | no |
| `opus-software` | audio | opus | everywhere | `opusenc` / `opusdec` | 40 | BSD-3-Clause | no |
| `cuda` | graphics |  | Linux, Windows | `cudacompositor` | 240 | LGPL-2.1-or-later | no |
| `gl` | graphics |  | Linux, macOS, Windows | `glvideomixer` | 200 | LGPL-2.1-or-later | no |
| `va` | graphics |  | Linux | `vacompositor` | 200 | LGPL-2.1-or-later | no |
| `d3d12` | graphics |  | Windows | `d3d12compositor` | 190 | LGPL-2.1-or-later | no |
| `d3d11` | graphics |  | Windows | `d3d11compositor` | 185 | LGPL-2.1-or-later | no |
| `software` | graphics |  | everywhere | `compositor` | 64 | LGPL-2.1-or-later | yes |

Notes on the table:

* Ranks decide. The highest ranked entry whose elements are installed wins,
  and decode and encode are decided separately, so a machine with NVDEC and no
  usable NVENC gets `nvh264dec` and `x264enc`.
* `x264enc` and `x265enc` are GPL and are never bundled into a build. They are
  there for an operator who installs `gst-plugins-ugly` themselves. `fdkaacenc`
  carries the Fraunhofer licence and is not redistributable either. The always
  present fallbacks are openh264 and SVT-AV1, both permissive, plus `avenc_aac`
  from gst-libav.
* The Raspberry Pi 4's `v4l2h264enc` is in the table. The Raspberry Pi 5 has no
  hardware video encoder at all and falls back to software, which is why the
  newer board is the harder target for a mixer.
* Every GPU graphics entry is rank none in GStreamer 1.28, which means
  GStreamer itself does not auto select it. The software compositor stays the
  default until an entry carries a `verified` record for your platform.

## Containers

The output kind picks a container; the catalogue says which muxer that is and
what it can carry. An RTMP output is FLV, which carries H.264 and AAC and
nothing else, and that is why an AV1 entry at a high rank still does not give
you an AV1 programme over RTMP.

| container | muxer | video | audio | streamable |
|---|---|---|---|---|
| `flv` | `flvmux` | `h264` | `aac` | yes |
| `mpegts` | `mpegtsmux` | `h264`, `h265`, `av1` | `aac`, `opus` | yes |
| `mp4` | `mp4mux` | `h264`, `h265`, `av1` | `aac`, `opus` | no |
| `matroska` | `matroskamux` | `h264`, `h265`, `av1` | `aac`, `opus` | yes |
| `webm` | `webmmux` | `av1` | `opus` | yes |
| `ogg` | `oggmux` | none | `opus` | yes |


## Where entries come from

In this order, each layering over the one before:

1. The `codecs.toml` compiled into the binary, which is this table.
2. `[codecs]` in your configuration file.
3. `--codecs <file>` on the command line.
4. `GMX_CODEC_RANK=<id>=<rank>,...` and `GMX_CODEC_DISABLE=<id>,...` from the
   environment.

An entry whose `id` matches an existing one replaces it outright. Anything else
is appended. See [add-a-codec-entry.md](../how-to/add-a-codec-entry.md).

## Regenerating this page

```
gmx codec list --json > /tmp/catalogue.json
```

and rebuild the tables from `entries` and `containers`. The `present` field in
that JSON is about the machine you ran it on, so it is deliberately not in the
table above.
