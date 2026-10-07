# Transitions and effects from packs

What an imported transition or effect is on disk, which files the mixer reads
and which it does not and why, every `fx.*` method, and how each kind is
drawn and what it costs.

The page for getting one on air is
[use transitions and effects from packs](../how-to/transitions-from-packs.md).
The built in transitions (fade, wipe, slide and the rest) are in
[transitions](transitions.md).

## The four kinds

| `look` | What it is | What a pack calls it | Drawn |
|---|---|---|---|
| `stinger` | a clip with an alpha channel, over both scenes, the cut underneath where it covers the picture | stinger, logo sting, transition with alpha | the clip by its alpha (`blend` `normal`) |
| `overlay` | a clip on black, meant for Screen or Add | light leak, bokeh, film burn, flames, sparks, dust, lens flare | `screen` or `add`, or `luma` to key its black |
| `matte` | a black to white picture: the new scene shows first where it is darkest | luma wipe, luma matte, gradient wipe | the old scene drawn back over the new by the matte |
| `shader` | GLSL in the gl-transitions form | shader transition, gl-transitions | on the GPU through GStreamer GL, or a software version |

A `stinger` and an `overlay` can be a transition (cut under them), an effect
(played over the programme on its own), or both. A `matte` and a `shader`
change one scene into another and are transitions only.

## What is read and what is not

| Format | Read | Why |
|---|---|---|
| WebM, VP8 or VP9 with alpha | yes | `vp8alphadecodebin` and `vp9alphadecodebin` give the alpha; the bundled runtime keeps both |
| MOV, ProRes 4444 with alpha | yes | `avdec_prores`, in the libav plugin the runtime keeps for H.264 |
| MOV, QuickTime Animation (RLE) with alpha | yes | `avdec_qtrle`, the same plugin |
| MOV with PNG frames | yes | `pngdec` |
| MP4, MOV, WebM, MKV with no alpha | yes, as an overlay or an opaque stinger | any decoder the runtime has; a light leak in H.264 is the common case |
| PNG, JPEG, TIFF, WebP still | yes, as a matte | one frame, read grey |
| a GIF | yes, as an overlay or a matte | GStreamer reads the frames |
| `.glsl`, `.frag` in the gl-transitions form | yes | see [shaders](#shaders) |
| a folder or a `.zip` of any of these | yes | deflate and stored zips, read by the mixer with no unzip tool |
| a PNG or TGA image sequence | no, and skipped by name | a numbered folder of stills is a clip without a frame rate; turn it into a MOV with PNG frames, or ProRes 4444, first |
| HEVC with alpha (Apple's) | not counted on | only a decoder that keeps the alpha layer gives one, and none was tested on this build; export ProRes 4444 or VP9 with alpha |
| a video luma matte (a clip that is the matte) | no | a matte is one picture; take the frame you want as a PNG |
| After Effects, Premiere or Resolve project files, MOGRTs | no | they are programs for those editors, not media. Render the transition out to one of the formats above |
| zips compressed with anything but deflate (LZMA, bzip2, zstd) | no | unpack it and import the folder |

## The folder

Every item is a gallery item: a folder in the gallery's folder, `graphics/`
inside the media library unless `[graphics] gallery` says otherwise, with a
`graphic.toml` whose `kind` is `transition` or `effect` (the gallery's format
is [the gallery format](gallery-format.md)). So the gallery lists, previews,
exports and deletes transitions and effects with every other design, and the
folder's name is the item's `name`, a slug.

```
<media dir>/graphics/
  .gmx-gallery               the gallery's marker; the media list skips the folder
  fx-assign.json             the default and per scene transitions (fx.assign)
  light-leak/
    graphic.toml
    light-leak.webm
    preview.jpg              the middle frame, the still the gallery shows
    preview-strip.jpg        twelve frames side by side, the picker's moving preview
    preview.webm             the same twelve frames as a loop, for the gallery's card (starters)
    LICENSE.txt              copied from the pack, when it had one
```

### graphic.toml

```toml
name = "Light leak"
kind = "transition"
file = "light-leak.webm"
description = "Warm light washing across the picture to near white."
tags = ["light leak", "warm", "overlay"]
origin = "shipped"

[transition]
look = "overlay"
blend = "screen"
duration_ms = 2000
cut_at_measured_ms = 1000
coverage = 1.0
transition = true
effect = true
licence = "Apache-2.0"
source = "made for GodwinMix by dev/make-starter-fx.py"
```

The top level keys are the gallery's (see its format page): `name` is what a
person calls it, `kind` is `transition`, or `effect` for an item that is only
an effect, `file` is the media file in the folder. The table named after the
kind holds the rest:

| Key | Required | What it is |
|---|---|---|
| `look` | yes | `stinger`, `overlay`, `matte` or `shader` |
| `blend` | no | `normal` (by alpha, the default), `screen`, `add`, `luma` |
| `duration_ms` | yes | a clip's own length; a matte or shader's default length |
| `cut_at_ms` | no | where the scenes swap under a clip, set by a person or `fx.set` |
| `cut_at_measured_ms` | no | where the import found the clip covers the picture most |
| `coverage` | no | how much of the picture the clip covers there, 0 to 1 |
| `softness` | no | a matte's edge, 0 (hard) to 1; 0.1 when absent |
| `invert` | no | read a matte white first |
| `transition` | no | a take may use it |
| `effect` | no | `fx.fire` may play it |
| `licence`, `source` | no | the pack's terms and where it came from, kept as the import found them |

A take cuts at `cut_at_ms`, then `cut_at_measured_ms`, then half way. Keys the
mixer does not know are kept, in the table and outside it, when it writes the
file back.

## How the import decides

`fx.import` decodes the whole file at 128x72 on a worker thread, never on the
mixer's, and measures every frame: how much of it is solid by its alpha, how
much is near white, how much near black, how far from grey.

* A clip whose decoder gave an alpha channel, and used it, is a `stinger`. The
  cut is the middle of the run of frames where it is most solid.
* A clip that is mostly black, or starts or ends on black, is an `overlay`
  for Screen. The cut is the middle of its whitest run of frames, and it is a
  transition only if that frame is at least half white. Otherwise it is an
  effect only, and its `note` says so.
* A grey picture, or a single still, is a `matte`.
* Anything else is an opaque `stinger` cut half way, which covers the picture
  for its whole length.
* A `.glsl`, `.frag` or `.fs` is a `shader`, checked for a `transition`
  function and a default for each uniform.

Pass `kind`, `blend` or `cut_at_ms` to `fx.import` to decide instead.

The gallery's own Import (`gallery.import`, its Import button, a drop on it)
takes the same way a `.glsl`, `.frag` or `.fs`, as a shader transition, and a
clip of light on black with no alpha, as an effect and a transition when it
covers enough. A clip with alpha stays a gallery clip there, because it is as
likely to be a moving lower third as a stinger; import it with `fx.import` or
the transition picker beside Take to make it a stinger. On the
starter set the measurement agrees with the shipped files to within 34 ms
(`crates/godwinmix-core/tests/fx_import.rs`).

## How each is drawn

Nothing here is a source and nothing changes how a scene is composited. A
look is a **pass** on the overlay board, drawn on each programme frame after
the compositor and after the transparent sources. With no pass and no
transparent source the board's probe is not on the pad at all.

### A clip: stinger and overlay

The clip is decoded on a pipeline of its own (`uridecodebin`, convert and
scale to the canvas in AYUV, an `appsink` that holds three frames), so a clip
that will not decode costs that clip and nothing else. Its frames are taken
by their own time against the programme's running time: a clip that started
late skips the frames it missed rather than running late.

A take with a clip waits for the clip's first frame before it starts the
window, so the cut lands under the clip. The take answers at once and the
scene on air stays live meanwhile; on this laptop that is about 40 ms warm. A
clip that has not opened in 800 ms starts anyway.

The blend is worked in Y'CbCr, because the canvas is I420 and stays I420.
`add` adds the clip's light to each component, which is what Add is in RGB
until it clips. `screen` is `a + b - ab`: exact in luma, and the first order
term of the same product in colour. `luma` uses the clip's brightness as its
alpha. Every mode skips a black pixel with one comparison, so a leak that is
mostly black costs mostly that.

### A matte and a shader

These need both scenes as pictures, and the compositor makes one. So the
compositor cuts to the new scene two frames after the take, and the pass
draws the old scene back over it. When the scene going away was one source
filling the canvas (the camera to camera case) the pass reads that source's
newest frame and the old picture keeps moving. Otherwise the pass keeps the
last programme frame before the window and the old picture holds still while
it is wiped away.

A matte is read once, on a thread, into one byte a pixel at the canvas size,
and kept for the next take of the same file. Each frame becomes a table of
256 weights, so a pixel is a lookup and a mix.

### Shaders

The gl-transitions form: a function `vec4 transition(vec2 uv)`, reading the
two scenes through `getFromColor(uv)` and `getToColor(uv)`, with `progress`
from 0 to 1 and `ratio` the canvas width over its height. A uniform of its own
is written with its default in a comment, which becomes a constant:

```glsl
uniform float amplitude; // = 0.04

vec4 transition(vec2 uv) {
  return mix(getFromColor(uv), getToColor(uv), progress);
}
```

A uniform with no default is refused at import, by name. The MIT licensed
gl-transitions collection is written this way and imports as it is; keep its
licence file beside it.

Where GStreamer GL runs, the shader runs on the GPU: both scenes go up as one
texture, old above new, through `glupload`, `glcolorconvert`, `glshader` and
back with `gldownload`, on a pipeline of its own for the length of the window.
The programme's thread never waits for the GPU: it draws the newest answer,
which is the previous frame's, so during a shader the new scene is one frame
behind. Where GL does not run, the two shipped shaders have software
versions (`ripple`, `glitch-slice`), and any other shader runs as a dissolve.
`fx.list` says which as `runs`: `gpu`, `cpu` or `fade`. Whether GL runs is
asked once, the first time something wants to know: two frames of a plain
mix must come back as asked. Each shader is then asked once more, at the
canvas size, before its first take on the GPU, and one that draws only the
old picture half way through runs the software way instead, with a warning
in the log. On macOS shaders always run the software way for now: on the
macOS build machines GL passed both checks and a take still showed only the
old scene.

## What it costs

Measured on this laptop (Windows 11, Intel Core Ultra 9 285H, Intel Arc
graphics) while four other builds ran on it, so read these as an upper bound:
the same loops gave numbers up to twice as far apart between runs. One core,
a 1920x1080 frame, `cargo test -p godwinmix-core --release --test fx_cost --
--ignored --nocapture`:

| What is drawn | Per frame | Share of a 30 fps frame |
|---|---|---|
| a stinger by its alpha, full size | 9.5 ms | 28 percent |
| a clip with Screen, decoded at half size | 13.3 ms | 40 percent |
| a clip with Add, at half size | 6.8 ms | 20 percent |
| a clip with a luma key, at half size | 10.3 ms | 31 percent |
| a luma matte | 5.7 ms | 17 percent |
| a dissolve (a shader with no GPU and no software version) | 2.5 ms | 7 percent |
| `glitch-slice`, software | 0.6 ms | 2 percent |
| `ripple`, software | 35 ms | 105 percent |

The software ripple is more than a frame at 1080p on this machine, so with no
GPU it is taken off after ten frames and the take stays a cut; at 720p it
fits. On the GPU it costs the programme's thread two frame copies a frame,
the pictures up and the answer down.

Decoding a starter clip into 1080p AYUV runs on the clip's own threads: 76
to 80 frames a second, so well ahead of a 30 fps programme. The first frame
of the first clip a process opens came after 556 ms, while the decoder's
plugin loaded; after that 48 ms.

End to end, an optimised core (`--profile ci`) at 1920x1080 and 30 fps, a
moving test pattern on air, the station and the show process together, each
item played back to back for eight seconds against the five seconds before
it, three rounds, the median (`fx.fire` for an effect, takes between two
sources for a transition):

| Item | Added CPU, percent of one core |
|---|---|
| `bokeh` fired | 15 |
| `film-burn` fired | 31 |
| `glitch` fired | 49 |
| `light-leak` fired | 96 |
| `glitch` take | 44 |
| `light-leak` take | 42 |
| `film-burn` take | 27 |
| `iris` take | 0 to 11 |
| `glitch-slice` take, on the GPU | 4 |
| `ripple` take, on the GPU | 34 |

The core at rest took 40 to 70 percent of a core in the same windows, which
is the noise these sit in. Nothing above showed as a slow programme frame,
and no effect was taken off.

## Methods

Every one is an MCP tool behind `search_tools`.

| Method | Tool | Scope | What it does |
|---|---|---|---|
| `fx.list` | `list_fx` | read | every item, with `runs`, `preview`, `note`, the assignments and whether GL runs here |
| `fx.import` | `import_fx` | operate | a file, a folder or a zip, by path on the mixer's machine or name in the media library |
| `fx.set` | `set_fx` | operate | change `blend`, `cut_at_ms` (0 puts the measured one back), `duration_ms`, `softness`, `invert`, `title`, `transition`, `effect` |
| `fx.remove` | `remove_fx` | operate, destructive | delete an item's folder; the starter set is refused |
| `fx.fire` | `fire_fx` | operate | play an effect over the programme once, with `opacity` and a `blend` for this firing |
| `fx.preview` | `preview_fx` | read | the strip's URL, frame count and size |
| `fx.assign` | `assign_transition` | operate | the transition a take uses when it names none, for a scene or for every take |

A take names an item as its transition, by name or as an object:

```json
{"method": "program.take",
 "params": {"scene": "wide", "transition": "light-leak"}}

{"method": "program.take",
 "params": {"scene": "wide",
            "transition": {"type": "iris", "duration_ms": 800,
                           "params": {"easing": "ease-out"}}}}
```

A clip runs for its own length whatever `duration_ms` says, and its cut may
be moved for one take with `params.cut_at_ms`. A matte or shader runs for the
take's `duration_ms`, or its own default. `program.transitions` lists every
item a take may use with `origin` `fx`. A built in name always means the built
in transition: an imported `fade` is named `fade-fx`.

When a take names no transition, its scene's assigned one is used, then the
default, then the cut a take has always been. `"cut"` is a cut whatever is
assigned, which is what the page's Cut button sends.

```json
{"method": "fx.assign", "params": {"scene": "Interview", "transition": "light-leak"}}
{"method": "fx.assign", "params": {"transition": "fade"}}
{"method": "fx.assign", "params": {"scene": "Interview"}}
```

Two routes carry bytes:

| Route | What it is |
|---|---|
| `GET /api/v1/fx/{name}/preview.jpg` | the preview strip, made the first time it is asked for; `?token=` works for an `<img>` |
| `POST /api/v1/fx/upload?name=pack.zip` | the body is the file; kept under `graphics/.uploads/` and imported, answering what `fx.import` answers |

A refusal names what to do next: an unknown name lists the names there are, an
effect taken as a transition says to fire it or turn it on with `fx.set`, a
file that will not decode lists the formats above.

## The starter set

Seven items ship in the binary from `graphics/starters/`, about 280 KB in all,
86 KB of it the media and the rest the previews the gallery and the picker show,
every one made for this project by `dev/make-starter-fx.py` with ffmpeg and
under the repository's licence:

| Name | Look | Blend | Use |
|---|---|---|---|
| `light-leak` | overlay | screen | transition and effect, 2 s |
| `bokeh` | overlay | screen | effect, 3 s |
| `glitch` | stinger (VP9 alpha) | normal | transition and effect, 1 s |
| `film-burn` | overlay | add | transition and effect, 1.5 s |
| `iris` | matte | | transition |
| `ripple` | shader | | transition, with a software version |
| `glitch-slice` | shader | | transition, with a software version |

They are written into the library the first time it is read, and again if a
folder is deleted. `fx.remove` refuses them; `fx.set` turns one off.

## Not done yet

* A clip's sound. A stinger's whoosh is decoded and dropped; the programme's
  audio carries on under it.
* A matte that is a clip.
* A shader with its own textures (`uniform sampler2D`): a gl-transitions
  shader that reads a picture of its own refuses at import.
* The station relays the two byte routes like any other; a show behind a
  station has not been tried with them.
