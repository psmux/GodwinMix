# Add text and a ticker

A name strap, a title, a crawl along the bottom, credits rolling up. The mixer
draws these itself, with no browser and no graphic template, so they cost
little enough to run on a Raspberry Pi beside the cameras.

The words are rendered once, when they change, and held. A text that does not
change costs the programme the blend of its box and nothing else. A ticker's
strip is rendered once too, and moved a little each frame.

## From the page

1. Press **Add sources** in the scene you want the words in.
2. Press **Add text** or **Add ticker** beside the search box.

That is the whole of it. The text arrives as a lower third in the bottom left;
the ticker as a bar across the bottom of the frame. Both are items in the
scene like any other, so drag them, resize them and stack them in the
composer as you would a camera.

**Text and tickers** in the picker's list has two more: a **Title** in the
middle with no box, and **Credits roll**, lines rolling up the whole frame.

## Change the words while it is on air

Press the gear on the text's tile. The settings that open are the words and
the look, and each change goes on air a fifth of a second after you stop
typing. There is no Apply button. The source is not rebuilt and the programme
does not miss a frame: the core renders the new words once and swaps them in.

For a ticker the words box takes one item a line. Speed, direction and
**Go round again** change without the crawl starting over; new words start
again from the edge.

## From the command line

```sh
gmx ctl source add strap "text:Ada Lovelace" --param size=44 --param shadow=true
gmx ctl source set strap --param text="Grace Hopper"
gmx ctl source add news "ticker:Polls close at ten" --param speed=150
gmx ctl source set news --param 'items=["Polls close at ten","Rain later"]'
```

A value that reads as JSON is taken as JSON, so numbers, `true` and lists
arrive as themselves; anything else is a string. Put the source in a scene
with `gmx ctl scene add`, the same as any source.

Over the API it is `source.add` with `type` `text/source` or `ticker/source`
(or an address starting `text:` or `ticker:`), and `source.set` with the
params that change. Every param is in the [text and ticker
reference](../reference/text-sources.md), and the schema for each kind is in
the `kinds` table of `protocol.json`.

## What it looks like

| Param | What it does |
|---|---|
| `text` | the words; a new line is a new line on screen |
| `font`, `size`, `weight`, `italic` | the letters. `size` is the letter height in canvas pixels |
| `color` | the letters' colour, `#rrggbb` |
| `outline`, `shadow` | an outline colour round each letter, a soft shadow under them |
| `background`, `padding`, `radius` | the box behind the words: a colour with opacity (`#000000b3`), the room round the words, the corners |
| `align` | left, center or right |

A box is as big as its words unless `width` and `height` say otherwise. Placed
bigger or smaller in a scene, it is rendered again at the size it is drawn, so
the letters stay sharp: the height of the box sets the letter size, and a wider
box is more room rather than wider letters.

## Fonts

`font` is the name of a font installed on the machine the mixer runs on,
found by Pango the way any desktop program finds it. A name the machine does
not have falls back to its default sans serif, so a show that moves between a
Mac and a Pi keeps working with different letters. Install the same font on
both to have the same letters.

## When it does not show

* **The words are drawn on top of a camera that is above them in the scene.**
  Text, tickers and every transparent picture are drawn over the opaque items
  of a scene, whatever their place in its stack. Among themselves the stack
  order holds.
* **A ticker longer than about 32000 pixels is cut.** That is a strip of a few
  hundred words at 40 pixel letters. Split it into two tickers.
* **On a GPU compositor** (`[hardware] graphics` set to a GPU entry), text is
  drawn as a solid box: the overlay drawing is for the software compositor,
  which is the default.
