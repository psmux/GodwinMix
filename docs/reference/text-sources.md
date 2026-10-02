# Text, ticker and transparent sources

The kinds the mixer draws over the programme rather than through the
compositor, and the params each takes. The same schemas are in the `kinds`
table of `protocol.json`, under each kind's `params`, which is what the CLI,
the MCP server and the web UI build their forms from.

How they are drawn, and why, is in
[how a scene reaches the compositor](../explanation/how-a-scene-reaches-the-compositor.md#transparent-items).

## `text/source`

Claims addresses starting `text:`. The rest of the address is the words when
`params.text` is not given, with `\n` for a new line.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `text` | string | the address after `text:` | the words; a new line is a new line on screen |
| `width` | integer or absent | as wide as the longest line | width of the box at the item's own size; given, lines wrap inside it |
| `height` | integer or absent | as tall as the lines | height of the box at the item's own size |
| `font` | string | `Sans` | a font family installed on the mixer; an unknown one falls back to the default sans serif |
| `size` | number | 48 | letter height in canvas pixels, more than 0 and at most 1000 |
| `weight` | integer | 600 | 100 to 900: 400 regular, 700 bold |
| `italic` | boolean | false | |
| `color` | string | `#ffffff` | `#rgb`, `#rrggbb` or `#rrggbbaa` |
| `outline` | string | empty | a colour for an outline round each letter; empty for none |
| `shadow` | boolean | false | a dark shadow under the letters |
| `background` | string | `#000000b3` | the box behind the words, a colour with opacity; empty for no box |
| `padding` | number | 24 | pixels between the words and the edge of the box |
| `radius` | number | 12 | corner radius of the box, in pixels |
| `align` | string | `left` | `left`, `center` or `right` |
| `valign` | string | `middle` | `top`, `middle` or `bottom`, in a box taller than the words |

Every param changes in place through `source.set`: the source is not rebuilt.

## `ticker/source`

Claims addresses starting `ticker:`. The rest of the address is the one item
when neither `items` nor `text` is given. Takes every param `text/source` does
except `text`'s wrapping (`width` here is the bar's width), and these:

| Param | Type | Default | Meaning |
|---|---|---|---|
| `items` | list of strings | empty | shown one after another with `separator` between; rolling up, one a line |
| `text` | string | the address after `ticker:` | one item, used when `items` is empty |
| `separator` | string | three spaces, a bullet, three spaces | between items, and between the end of the list and its start again |
| `speed` | number | 120 | pixels a second, 0 to 5000 |
| `direction` | string | `left` | `left` (right to left), `right` (left to right) or `up` (credits) |
| `loop` | boolean | true | go round again for ever; off, the words cross once and the bar empties |
| `width` | integer | 1920 | the bar's width before it is placed; the scene's box is what it is drawn in |

The defaults differ from a text's where a bar wants it: `size` 40, `padding`
12, `radius` 0. Speed, direction and loop change without the crawl starting
over; a change to the words or the look starts it again from the edge.

A strip longer than 32000 pixels at the size it is drawn is cut there.

## `image/source` and `file/source`: `alpha`

| Param | Type | Default | Meaning |
|---|---|---|---|
| `alpha` | boolean or `"auto"` | `"auto"` | whether the picture's or clip's transparency is kept |

With `auto` a PNG or WebP is looked at when the source is added, from the
first bytes of the file, and goes to the overlay board when it has an alpha
channel. An SVG always does. A clip in a WebM, Matroska, QuickTime or GIF
container is looked at when its decoder starts: with alpha it is drawn over
the programme, without it goes through the compositor as every clip does.
`false` draws either flat. `true` stands the alpha branch by for a clip in any
other container. A picture behind an `https://` address is not looked at
before it is decoded, so it is drawn flat unless `alpha` is `true` or it is an
SVG.

## What the status says

A source drawn by the overlay board carries `"alpha": true` in its status. A
clip that could have carried alpha and turned out not to says `false`. A text
or ticker also carries its current `params`, which is what the editor in the
web UI opens on.

## Formats that keep their alpha

Checked on macOS with GStreamer 1.28.7, with the `which_alpha_formats_this_machine_keeps`
test in `crates/godwinmix-core/tests/overlay_clip.rs`:

| Format | Decoder | Alpha |
|---|---|---|
| PNG, WebP, SVG stills | `pngdec`, `webpdec`, `rsvgdec` | kept |
| WebM, VP8 with alpha | `vp8alphadecodebin` | kept |
| WebM, VP9 with alpha | `vp9alphadecodebin` | kept |
| QuickTime, ProRes 4444 | `avdec_prores` (VideoToolbox is passed over for it, because it drops the alpha) | kept |
| QuickTime Animation (`qtrle`) | `avdec_qtrle` | kept |
| QuickTime, PNG frames | `pngdec` | kept |
| HEVC with alpha | `vtdec_hw` | not kept: drawn flat, the transparent part showing as whatever colour it holds |

Export HEVC with alpha as ProRes 4444 or WebM VP9 for this build.
