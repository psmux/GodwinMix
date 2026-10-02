# The chroma key: `chroma/filter`

A green or blue screen key. Put it on a scene item (`scene.item.filter.add`
with `type: "chroma/filter"`) and the item's picture is drawn over whatever is
under it, with the screen taken out. Every setting has a default, so `{}` is a
working key.

## Settings

| Setting | Range | Default | What it does |
|---|---|---|---|
| `color` | `"auto"` or `"#rrggbb"` | `"auto"` | The screen colour. `auto` looks at the first frames for the biggest green or blue area, and keeps looking every half second until it finds one. Also read as `key` or `colour`. |
| `method` | `green`, `blue`, `custom` | `green` | Which screen `auto` looks for. `custom` looks for either. |
| `similarity` | 0 to 1 | 0.4 | How far from the key colour, towards grey, a pixel can be and still go fully clear. Higher keys out darker and paler parts of the screen too. |
| `smoothness` | 0 to 1 | 0.1 | The width of the band between clear and solid: the edge softness. |
| `spill` | 0 to 1 | 0.6 | How much of the screen colour is taken out of what stays. 0 leaves it, 1 takes all of it. Brightness is not changed. |
| `feather` | 0 to 20 pixels | 1 | Softens the matte's edge inwards by about this many pixels. It also pulls the edge in by about as much. |
| `matte_left`, `matte_right`, `matte_top`, `matte_bottom` | 0 to 1 each | 0 | The garbage matte: the fraction of the picture cut away from each edge. Each opposite pair must add up to less than 0.98. |

A value outside its range is refused with the field named and its range.

Older spellings are read too. OBS's `similarity` and `smoothness` and `spill`
from 1 to 1000 are divided by 1000, `key_color_type` is read as `method`,
`key_color` (OBS's packed integer) as `color`, and `target_r`, `target_g`,
`target_b` together as `color`. `angle`, `noise`, `spread`, `opacity`,
`contrast`, `brightness` and `gamma` are accepted and ignored.

## How it decides

Each 2x2 block of the camera's I420 frame is looked up in a table built from
the settings, so a frame costs one lookup per block and nothing is converted.
A colour's lean towards the key is measured in the key's own direction:

```text
  m = (p - q) / |k|
```

where `k` is the key's chroma, `p` how far a pixel's chroma reaches along `k`
and `q` how far it strays sideways. `m` is 1 at the key colour, 0 for every
grey however bright or dark, and below 0 for colours on the far side, skin
among them. A pixel with `m` above `1 - similarity` is clear; the `smoothness`
band below that is the soft edge. Spill is the part of `p` larger than `q`, and
`spill` takes that share of it away.

The alpha is spread from blocks to pixels as the picture is drawn, with the
same 3 to 1 weights a bilinear upscale uses, so an edge is as fine as the
camera's luma.

## Where it draws

On a scene item, the key hands the programme's overlay board the camera frame
and its matte, and sends the compositor pad an empty gap buffer for each frame.
The board draws it after the compositor, at the place, size, z order and
opacity the scene gives the item, the same as a text or a transparent PNG. So
a picture behind the presenter stays behind, and a transparent picture above
the presenter in the scene, such as a desk, stays in front.

Where nothing can draw it, it flattens the key into the frame over black:

* On a source's input side (`filter.add` with `side = "input"`).
* On the programme itself (`attach = { programme = true }`).
* On a programme composited on a GPU graphics entry.

A key on a source's programme side (`side = "programme"`) draws on the board
wherever the scene puts that source.

The composer's preview does not run item filters, so it shows the camera
unkeyed. The programme, its snapshot and the multiview programme cell show the
key.

## Changes on air

`scene.item.filter.set` merges the params it is given into the key's and
applies them to the running key: no rebuild, no pad block, no gap. The UI's
sliders use this. Changing the filter's type, adding one or removing one
rebuilds that item's chain under a pad block on its own queue.

## Cost

Measured on an Apple M4 Pro, release build, a 30 fps programme, the whole
process's CPU over eight seconds with the key on minus with it off. The
presenter in the "figure" row is a head and shoulders on a green screen, about
a third of the frame; in the "bars" row it is `test://smpte` keyed on its green
bar, six sevenths solid.

| Canvas | Presenter | This key | The `alpha` key it replaced |
|---|---|---|---|
| 1280x720 | figure | 6.6 percent of a core | 11.2 |
| 1280x720 | bars | 7.2 | 7.0 |
| 1920x1080 | figure | 14.0 | 21.0 |
| 1920x1080 | bars | 14.7 | 13.7 |

The `alpha` key's figures are for a key that did not work: its output reached
the compositor as I420 with the screen turned black, and it was drawn through
the compositor as an opaque picture. The figures for this key include drawing
the presenter on the board, which the compositor no longer does. The key's own
work, without the drawing, is 0.7 ms a frame at 720p and 1.4 ms at 1080p.

The measurement is `crates/godwinmix-core/tests/key_cost.rs`:

```sh
GMX_KEY_SOURCE=studio GMX_KEY_CANVAS=1280x720 cargo test -p godwinmix-core --release --test key_cost -- --ignored --nocapture
```

## Reading a key colour

`source.key_color {id, x?, y?, screen?}` answers `{color, found, share?}`.
With `x` and `y` (0 to 1 across and down the source's picture) it is the mean
colour of a small square there, and `found` is `point`. With neither it is the
biggest green or blue area, `found` is `green` or `blue` and `share` the part
of the picture it covers. `screen: "green"` or `"blue"` looks for one only.
Both read the source's tile on the mosaic, so they need `[snapshot]` enabled;
when it is not, the error says so and names the setting. A source whose
picture has no screen covering at least 8 percent of it is refused with
`data.min_share`.
