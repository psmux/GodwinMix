# Write an SVG template

A template is an SVG file with `{{fields}}` in it. The mixer fills in the
fields, draws the SVG itself with no browser, and holds the picture until a
field changes. A lower third built this way costs a Raspberry Pi almost
nothing while it sits on screen, and its words change on air with no rebuild
and no dropped frame.

Use a template for anything designed: panels, an accent colour, two sizes of
type, a logo. For plain words in a box, a [text source](add-text-and-a-ticker.md)
is cheaper still. For a graphic that moves inside itself every frame, use
[OGraf](make-a-graphic.md), which runs a browser page per graphic.

Every key and limit is in the [reference](../reference/graphic-templates.md).

## 1. Put one from the pack on air first

Eight templates ship inside the mixer. In the web UI, **Add sources**, then
**Graphics**, then **Choose** beside one. The form asks for its words; **Add**
puts it over the whole scene, where its own layout lands it inside title safe.

From a terminal it is the same two steps:

```sh
gmx ctl template list
gmx ctl source add strap template:news-lower-third --param fields.name="Ada Lovelace" --param fields.title="Engine Research"
```

Then put `strap` on a scene over the whole canvas, as in
[compose a scene](compose-a-scene.md).

Change a field while it is on air, from the gear on its tile (the change goes
out as you type) or from the terminal:

```sh
gmx ctl source set strap --param fields.title="Head of Engine Research"
```

## 2. Copy a pack template and change it

The pack is read only, so a station's own version is a copy in the media
library. Start from the one closest to what you want:

```sh
gmx ctl template get news-lower-third --out ours.svg
```

Open `ours.svg` in a text editor or in Inkscape. Keep three things:

* the `viewBox="0 0 1920 1080"` and the same `width` and `height`;
* the `{{name}}` markers where the words go;
* the `<metadata>` block, which gives each field its label and default.

Save it into the library:

```sh
gmx ctl template save ours-lower-third ours.svg
```

The mixer checks the SVG draws before it writes anything, and answers with the
fields it found and the address, `template:ours-lower-third.svg`. Saving again
with `--replace` writes over it, and every source drawing it is drawn again on
air with the new design.

## 3. The rules a template follows

### Fields

A field is `{{name}}`: lower case letters, digits and `_`, starting with a
letter. It can stand for words inside a `<text>` or for an attribute value:

```xml
<rect x="96" y="836" width="1000" height="84" fill="{{panel}}"/>
<text x="140" y="894" font-size="52" fill="{{text}}">{{name}}</text>
```

Declare each one so a form and an agent know what it is:

```xml
<svg xmlns="http://www.w3.org/2000/svg" xmlns:gmx="https://godwinmix.dev/ns/template" ...>
  <metadata>
    <gmx:template title="Our lower third" description="Name and title, bottom left"/>
    <gmx:field name="name" label="Name" default="Ada Lovelace"/>
    <gmx:field name="accent" label="Accent colour" type="color" default="#c8102e"/>
  </metadata>
```

Values are escaped before they go into the SVG, so a name like `Smith & Sons`
shows the ampersand and nobody's words can break the picture.

### Long words: shrink to fit

A template never wraps words, and a headline is always longer than the one you
designed with. Put `data-fit-width` on every `<text>` that holds a field:

```xml
<text x="140" y="894" font-size="52" data-fit-width="936">{{name}}</text>
```

When the words come out wider than 936 template units, the whole text is made
smaller until they fit, about its own `x` and `y`. A left aligned line keeps
its left edge; one with `text-anchor="middle"` keeps its centre. Set the width
to the inside of the panel, less 28 to 32 units of padding each side.

For a long quote, use one field a line, as `quote-card` does.

### Colours and the station's brand

Name the three main colours `accent`, `text` and `panel`. A station sets them
once in its config and every template that uses those names follows:

```toml
[graphics]
accent = "#0b6e3d"
text = "#ffffff"
panel = "#0d1f16"
```

A source can still set its own `fields.accent`, which wins.

### Fonts

The fonts are the ones installed on the mixer machine. Write a list that ends
in a generic family, and put DejaVu in it for a Raspberry Pi:

```xml
<style>
  .type { font-family: Inter, 'Helvetica Neue', Helvetica, Arial, 'DejaVu Sans', 'Liberation Sans', sans-serif; }
</style>
```

Write capitals yourself; CSS `text-transform` is not drawn.

### A logo

Put the logo file in the media library and name it:

```xml
<image href="logo.png" x="96" y="836" width="134" height="134"/>
```

A `data:` URI works too. An `https://` address is refused when the template is
saved, because a graphic that waits on a server is late on air.

### Safe areas

On 1920 by 1080, keep every word inside title safe and everything that matters
inside action safe:

| Area | x | y |
|---|---|---|
| Title safe, 90 percent | 96 to 1824 | 54 to 1026 |
| Action safe, 93 percent | 67 to 1853 | 38 to 1042 |

A full screen background may run to the edges.

### Movement

The template is drawn as a still: `<animate>` and CSS animation do nothing.
The way on and off screen is the scene item's `enter` and `exit` (a slide, a
wipe, a fade, a zoom), set in the composer's inspector or with
`scene.item.add`. See [transitions](transitions.md).

## 4. Check it

Look at it before anyone else does. Arm the scene in the preview and look at
the preview monitor, or ask for the picture:

```sh
gmx ctl rpc scene.preview.frame '{"width": 1280}'
```

Then try the worst words you can think of: the longest name in the building,
an ampersand, a score in three digits. Each should stay inside its panel.

## When something is wrong

* `there is no template "..."`: the message lists the pack. A library
  template is named with the `.svg` it was saved as.
* `is not an SVG with a size`: the `<svg>` element needs a `viewBox`.
* `has no field "..."`: the `data.fields` of the error lists the ones it has.
  Fields are lower case.
* The graphic is grey where it should be clear: the GPU compositor draws
  templates flat. The software compositor, the default, draws them with their
  transparency.
