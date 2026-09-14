# How to add or override a codec entry

The codec catalogue is a file, not a table in the source. A new GPU
generation, an element somebody renamed, or a codec the core has never heard of
is an entry you write, and none of it needs a rebuild.

There are three places to put one, in the order they are applied:

1. `[codecs]` in your configuration file. Yours, per machine.
2. `--codecs <file>`, the same shape in a file of its own. For trying a
   catalogue update before you install it.
3. `codecs.toml` in the repository, through a pull request. For everybody.

An entry whose `id` matches an existing one replaces it outright. Any other id
is appended. Replacing rather than merging is deliberate: half an entry from
two places is harder to reason about than one entry restated.

## A worked example: an AV1 programme

Suppose the box has SVT-AV1 and dav1d installed and you are sending to
something that speaks MPEG-TS rather than RTMP. Put this in your config:

```toml
[codecs]
# RTMP is FLV and FLV has never carried AV1, so the programme has to be muxed
# into something that can. Selection keeps to codecs this container can carry.
programme_container = "mpegts"

[[codecs.video]]
id = "av1-house"
codec = "av1"
accel = "software"
rank = 900                    # above every H.264 entry, so this one wins
encoder = "svtav1enc"
decoder = "dav1ddec"
parser = "av1parse"
requires = []                 # extra elements beyond the ones named above
license = "BSD-3-Clause"
container = ["mpegts", "matroska", "webm"]
keyframe = { property = "intra-period-length", unit = "frames" }
properties = { preset = 10, "target-bitrate" = { unit = "kbit", from = "video.bitrate_kbps" } }
```

Restart and check:

```
godwinmix --probe
```

```
video.encode
  -> av1-house                  accel software  rank 900  svtav1enc  installed
     h264-software-x264         accel software  rank 72   x264enc    installed

chosen
  video encoder : svtav1enc (software, entry av1-house, rank 900)
    codec       : av1
    parser      : av1parse
```

Then prove it encodes rather than merely builds:

```
gmx codec test av1-house
```

That is the whole change. No rebuild, no core release.

## The fields

| Field | What it does |
|---|---|
| `id` | How the entry is named and how an override finds it. Derived from codec, accel and element if you leave it out, but write one. |
| `codec` | `h264`, `h265`, `av1`, `aac`, `opus`. Containers are matched against this. |
| `accel` | The backend family. `software`, or a vendor name an operator can pin with `[hardware] encode`. |
| `rank` | Higher wins among the entries that are installed. |
| `encoder`, `decoder` | Element names. An entry may have only one of them; encode and decode are chosen separately. |
| `parser` | What goes between the encoder and the muxer. `h264parse`, `aacparse`, `av1parse`. The mixer takes the name from here and never writes one itself. |
| `download` | Element that pulls frames out of this backend's memory into system memory, for a hardware decoder handing out GPU surfaces. |
| `memory` | What memory type this encoder accepts on its sink pad. Defaults to `system`. |
| `requires` | Extra registry elements that must exist. See the rule below. |
| `container` | Containers this codec can be muxed into. |
| `license` | Required on every entry. `gmx build` refuses to bundle a copyleft entry into a custom build. |
| `keyframe` | The property that carries the keyframe interval, and its unit. |
| `properties` | Everything else to set on the encoder. |
| `verified` | Reports from people who ran `gmx codec test` on real hardware. |
| `comment` | Anything the next person needs to know. Write one. |
| `disabled` | Takes a shipped entry out of selection without restating it. |

### The `requires` rule

The role's own element is always required: an entry only encodes if its
`encoder` is installed. `requires` is for anything on top of that.

Naming the *other* role's element in `requires` does not block this role. The
shipped NVIDIA entry says `requires = ["nvh264enc"]` and also names
`nvh264dec`, and a machine with a working NVDEC and an unusable NVENC still
gets the fast decoder. That is a real configuration and the catalogue exists
partly to keep it working.

## Property units

A property is either written out or derived from the running configuration:

```toml
properties = { preset = "p4", bitrate = { unit = "kbit", from = "video.bitrate_kbps" } }
```

`{ unit, from }` means "take this variable and give it to the element in this
unit". It exists because encoders disagree. x264 wants kilobits, openh264 and
AMF want bits, and stating the programme bitrate once and converting at the
call site is the difference between a 2,500 kbit/s stream and a 2.5 Mbit/s one
that nobody notices until the upload dies.

Variables you can name, and the unit each is held in:

| `from` | Held in | Is |
|---|---|---|
| `video.bitrate_kbps` | `kbit` | the programme video bitrate |
| `audio.bitrate_kbps` | `kbit` | the programme audio bitrate |
| `keyframe.frames` | `frames` | the keyframe interval in frames |
| `keyframe.secs` | `seconds` | the same, in seconds |
| `canvas.fps` | `count` | the canvas framerate |
| `cpu.count` | `count` | usable cores on this machine |

Units that convert into each other: `bit`, `kbit`, `mbit` among themselves;
`frames` and `seconds` through the canvas framerate; `count` and `raw` for
plain numbers. Asking for `{ unit = "bit", from = "cpu.count" }` is refused,
with the reason, by the validation that runs in CI.

`keyframe` is separate from `properties` because every encoder has one and they
all spell it differently:

```toml
keyframe = { property = "key-int-max", unit = "frames" }   # x264enc
keyframe = { property = "gop-size", unit = "frames" }      # nvh264enc
keyframe = { property = "max-keyframe-interval", unit = "frames" }  # vtenc
```

### Nothing here can crash the mixer

Every property is set defensively. A property this element version does not
have, an enum nickname it has never heard of, a type that is a boolean on one
backend and an enum on the next: all of them are a logged warning and the
element runs with its default. So you can write `preset = "p4"` for the current
nvcodec and `zerolatency = true` for the older one in the same entry, and each
build takes the one it understands.

That means a typo is silent. `gmx codec test` is how you find out whether the
entry did what you meant.

## Nudging a rank without writing an entry

```
GMX_CODEC_RANK=h264-nvidia=0,av1-software=300 godwinmix
GMX_CODEC_DISABLE=h264-software-x264 godwinmix
```

Useful for bisecting a bad backend on a box you would rather not edit.

## Graphics entries

Compositor and conversion backends are entries too, in the same file:

```toml
[[graphics]]
id = "cuda"
accel = "cuda"
rank = 240
compositor = "cudacompositor"
convert = "cudaconvert"
upload = "cudaupload"
download = "cudadownload"
memory = "cuda"                # frames stay here from upload to encoder
requires = ["cudacompositor", "cudaconvert"]
license = "LGPL-2.1-or-later"
verified = []
```

A GPU graphics entry is not taken automatically until its `verified` list names
your platform, however high its rank. Pin it with `[hardware] graphics = "cuda"`
to try it, and see
[choose-a-hardware-encoder.md](choose-a-hardware-encoder.md) for why.

## Sending an entry upstream

Add it to `codecs.toml`, run `gmx codec test <id>` on the hardware, and paste
the `verified` line the test prints into the entry with your driver version and
name filled in. The pull request wants the hardware, the driver version, the
GStreamer version and that report. The project's CI covers NVIDIA and Intel;
everything else is verified by the person who owns the board.

The CI validation runs against a registry made of strings and needs no hardware
at all. It checks that the file parses, that ids are unique, that every entry
carries a licence, that every element name is a plausible factory name, that
every `{ unit, from }` names a variable that exists and a unit it can convert
to, and that selection still resolves for each of the reference machines. An
entry that fails any of those fails the build.
