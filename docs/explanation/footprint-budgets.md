# Footprint budgets and the reference machines

The tables on this page define targets. Measured results, named machines and
reproduction commands live in [Footprint](footprint.md), including the native
runtime packaging checks. Empty reference machine cells remain unmeasured.

## Why publish numbers at all

OBS Studio publishes no CPU or memory requirement. The public record is forum
anecdote ranging from three percent idle CPU to a 24 GB leak. Somebody sizing a
box cannot plan against that.

The people this project is for are sizing small boxes: a Raspberry Pi in a
church rack, a five year old laptop in a volunteer's cupboard, a six watt mini
PC. For them the question "will it run on this" is the whole question, and
"try it and see" is a bad answer when the trying happens on a Sunday morning.

## The reference machines

Five machines, named in every performance claim this project makes.

| Id | Machine | RAM | Encode path |
|---|---|---|---|
| `pi4` | Raspberry Pi 4 Model B | 2 GB | hardware H.264 (once the V4L2 backend exists) |
| `pi5` | Raspberry Pi 5 | 4 GB | x264 software; this board has no hardware encoder |
| `n100` | Intel N100 mini PC | 8 GB | VA (Quick Sync), about 6 W |
| `laptop` | a 2020 class x86 laptop, integrated graphics, no usable hardware encode | 8 GB | x264 software |
| `gpu` | desktop with an NVIDIA card | 16 GB | NVENC |

`laptop` is the most common machine this will ever run on. `pi4` is the
cheapest one that works. `gpu` is the only one OBS is really designed for.

## The budgets

| Measure | Target | Measured |
|---|---|---|
| Core idle memory, no sources, multiview off, `linux-aarch64` | at most 60 MB | |
| Core idle CPU, no sources, multiview off | at most 1 percent of one core on `pi4` | |
| Added memory per file source, container mode | at most 40 MB | |
| Added CPU per 720p30 file source into the compositor, no encode | at most 15 percent of one core on `pi4` | |
| Multiview encoder, 8 fps mosaic, 1280 wide | at most 10 percent of one core on `pi5`, and zero when no client is subscribed | |
| Snapshot and motion tracker | at most 5 percent of one core on `pi5`, and zero when disabled | |
| 720p30, two live sources, programme encode | `pi4` with hardware encode at most 1.0 core; `n100` at most 0.6; `pi5` software at most 2.0; zero dropped frames over 60 minutes | |
| 1080p60, six live sources, one chroma key, NVENC on `gpu` | core CPU at most 0.5 core, zero dropped frames over 60 minutes | |
| Core plus two sidecar plugins plus encoder, 720p30 | under 512 MB total, so a 2 GB device keeps room for the OS | |
| Cold start, process exec to first encoded programme frame | at most 2.0 s on `pi4` | |
| Core binary, `linux-aarch64` | at most 30 MB, plus the platform's GStreamer packages (about 19 MB installed) | |
| Windows desktop installer, GStreamer bundled and trimmed | at most 150 MB | |
| Command acknowledgement, p99, a take | under 50 ms, with the take landing on the next frame | |

## What is already known about the shape

These come from measurements someone else made, of GStreamer rather than of
this mixer, on an Apple M4 Pro at 720p30. They are not this project's numbers
and they are here because they say where the cost lives.

| Pipeline | Peak memory | Share of one core |
|---|---|---|
| `videotestsrc ! fakesink` | 41 MB | negligible |
| 720p30 raw, convert, fakesink | 55 MB | 0.08 |
| 720p30, convert, x264 veryfast | 121 MB | 0.40 |
| compositor, 2 layers, no encode | 109 MB | 0.29 |
| compositor, 2 layers, x264 veryfast | 146 MB | 0.48 |

Two things follow. A bare GStreamer process costs about 41 MB before it does
anything, which is the floor every out of process plugin inherits. And
compositing plus colour conversion costs about as much as the encoder, so on a
small board the mixer's own hot path, not the encoder, is what falls over
first. That is the opposite of the usual intuition and it is why the budgets
above separate the compositor from the encoder.

## The comparison the size budget is against

| Thing | Artefact | Size |
|---|---|---|
| ripgrep 15.2 | amd64 .deb | 1.6 MB |
| go2rtc 1.9 | linux arm64 binary | 4.4 MB |
| Caddy 2.11 | linux arm64 | 15 MB |
| MediaMTX 1.21 | linux arm64 | 28 MB |
| GStreamer core, base, good on Debian arm64 | installed | about 19 MB |
| OBS Studio 32.2 | Ubuntu .deb | 128 MB |
| Electron 44 runtime, before any app code | linux x64 | 117 MB |
| The stock GStreamer Windows runtime installer | | 527 MB |

A 30 MB binary plus 19 MB of platform packages is the target because that is
the company this project wants to keep. The 527 MB line is why bundling a
trimmed GStreamer into the Windows installer is a task rather than a nicety:
a 527 MB download is not something you ask a volunteer to do before a service.

## The bitrate default, which is a footprint too

The default output preset is 720p30 at 2,500 kbit/s. YouTube recommends 6,000
for 720p30. The default is lower on purpose: median fixed download is 22 Mbit/s
in Nigeria, 15 in Kenya, 32 in Indonesia, and uplink is a fraction of
download. A default above about 3,000 kbit/s fails for a large share of the
people this is for. The mixer degrades bitrate before it drops frames.

## How the numbers will be produced

`gmx bench` prints this table on the machine it runs on, naming the machine and
the commit. Every performance figure in the README will come from it. A run on
each reference machine goes into every release. Until that command exists, this
page has an empty column and says so.
