# LUTs and looks

`lut/filter` puts a look on a scene item, a source or the programme. The
`lut.*` methods manage the library of LUTs and presets it reads from. How to
use them from the page is in [Use LUTs and looks](../how-to/use-luts-and-looks.md).

## `lut/filter`

| Param | Type | Default | What |
|---|---|---|---|
| `lut` | string | none | a LUT in the library by name, built in or imported |
| `file` | string | none | a LUT file on the mixer's machine, read directly. OBS's `image_path` is read the same way |
| `preset` | string | none | a saved preset. Its values fill in whatever the params leave out |
| `amount` | number, 0 to 1 | 1 | how much of the LUT. OBS's `clut_amount` is read the same way |
| `brightness` | -1 to 1 | 0 | as in `color/filter`, run before the LUT |
| `contrast` | -4 to 4 | 0 | as above |
| `gamma` | -3 to 3 | 0 | as above |
| `saturation` | -1 to 5 | 0 | as above |
| `hue_shift` | -180 to 180 | 0 | as above |

`file` wins over `lut` when both are given. A LUT that is missing when the
filter is built leaves the picture as it is, with the reason in the log; a
change that names a missing LUT or preset is refused with the names the
library has.

The filter works on the canvas's own I420. Every change bakes the colour
correction, the LUT and the amount into one 33 point table from Y'CbCr to
Y'CbCr, on a thread of its own, and the frame interpolates in that table
(tetrahedral, whole numbers). Luma above white or below black passes around
the LUT unchanged. A frame of 1280x720 or more is cut into bands across up
to four cores.

## Built in LUTs

| Name | Group | What |
|---|---|---|
| `slog3-to-709` | camera | Sony S-Log3, S-Gamut3.Cine |
| `vlog-to-709` | camera | Panasonic V-Log, V-Gamut |
| `clog3-to-709` | camera | Canon C-Log3, Cinema Gamut |
| `logc3-to-709` | camera | ARRI LogC3 at EI 800, ARRI Wide Gamut 3 |
| `flog-to-709` | camera | Fujifilm F-Log, F-Gamut |
| `nlog-to-709` | camera | Nikon N-Log, Rec.2020 |
| `hlg-to-709` | camera | HLG, Rec.2020 |
| `warm` | look | more red, less blue |
| `cool` | look | more blue |
| `teal-orange` | look | teal shadows, orange highlights |
| `faded-film` | look | lifted blacks, soft whites, gentle colour |
| `bleach-bypass` | look | low colour, hard contrast |
| `high-contrast` | look | an S curve, colour kept |
| `vivid` | look | stronger colour |
| `black-and-white` | look | Rec.709 luma |
| `sepia` | look | black and white toned brown |
| `day-for-night` | look | darker, bluer, less colour |

Each is made in code at 33 points a side. A camera conversion decodes the
maker's log curve to scene linear, converts the gamut to Rec.709 with a
matrix worked out from its primaries, and maps to the display with a filmic
curve and a 2.4 gamma.

## Files

| Format | Read |
|---|---|
| `.cube` | `TITLE`, `LUT_1D_SIZE`, `LUT_3D_SIZE` (2 to 65), `DOMAIN_MIN`, `DOMAIN_MAX`, `LUT_1D_INPUT_RANGE`, `LUT_3D_INPUT_RANGE`. A file with both sizes runs the 1D table first |
| `.3dl` | an input mesh line, then the cube with blue fastest, 10, 12 or 16 bit |
| `.png` | 512 by 512 is OBS's layout (64 points, eight tiles across). Any other square whose side is a cube (64, 729, 1728, 4096) is a Hald CLUT |

The library is `luts/` in the graphics library's folder. Each imported LUT is
`<name>.cube` there, and presets are `presets.toml`:

```toml
[[preset]]
name = "church-warm"
title = "Church warm"
lut = "warm"
amount = 0.6
saturation = 0.1
```

## Methods

| Method | Scope | Params | Answers |
|---|---|---|---|
| `lut.list` | read | `group`: `camera`, `look` or `imported`, optional | `luts`, `presets`, `errors`, `formats` |
| `lut.import` | operate | `path` (absolute, or a name in the media library), `name`, `title` | `lut` |
| `lut.remove` | operate | `name` | `removed`. A built in is refused |
| `lut.preset.save` | operate | a preset: `name` and any of `title`, `lut`, `amount` and the five colour settings | the preset as kept |
| `lut.preset.remove` | operate | `name` | `removed` |
| `lut.preview` | read | `source`, `luts`, `presets`, `amount`, `width` (64 to 640, 240 by default) | `original`, `previews` (each `name`, `kind`, `image` as a JPEG data URL), `source` |

`lut.preview` grades a still of the source's tile on the mosaic through the
same table a frame goes through. Without a source, or before the source has a
picture, it uses a test card, and `source` is left out of the answer.

`POST /api/v1/luts/upload?name=look.cube` takes a LUT file as the body,
imports it, and answers what `lut.import` answers. It needs the operate
scope, and `title` in the query names it.

The MCP tools are `list_luts`, `import_lut`, `remove_lut`,
`save_lut_preset`, `remove_lut_preset` and `preview_luts`, found through
`search_tools`.
