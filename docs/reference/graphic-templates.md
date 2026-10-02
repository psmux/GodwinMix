# Graphic templates

The `template/source` kind, the template file format, the built in pack, the
methods and the MCP tools. How to write one is in
[write an SVG template](../how-to/write-an-svg-template.md); how to have an
agent do it is in [design graphics with an AI agent](../how-to/design-graphics-with-ai.md).

## `template/source`

Claims addresses starting `template:` (rank 230, above `image/source`, so
`template:/srv/gfx/bar.svg` is a template and not a still). The rest of the
address names the template:

| Address | Template |
|---|---|
| `template:news-lower-third` | the pack template of that name |
| `template:my-bar.svg` or `template:my-bar` | `my-bar.svg` in the media library (`[media] dir`) |
| `template:/srv/gfx/bar.svg` | that file, by absolute path |

| Param | Type | Default | Meaning |
|---|---|---|---|
| `fields` | table of string, number or boolean | empty | each field's value by name. A number or a boolean is taken as the words it is written as, so a score can be sent as `2` |

Any other param is refused with the one it takes. A name in `fields` that the
template does not have is refused, and the error's `data.fields` lists the
fields it does have. A value is at most 2,000 characters; a colour field takes
`#rgb`, `#rrggbb` or `#rrggbbaa`.

What a field shows, first match wins:

1. the source's own `params.fields.<name>`;
2. for `accent`, `text` and `panel`, the station's `[graphics]` colour, when set;
3. the template's declared default;
4. nothing.

`source.set` merges `params` one level down, so
`{"params": {"fields": {"headline": "..."}}}` changes that field and leaves the
others. `null` puts one field back to step 2. The change is applied in place:
the SVG is drawn again on the overlay worker and the new picture replaces the
old one on the next frame. The source is not rebuilt and the programme does not
skip a frame.

The source is drawn at the size its scene item is drawn at, once per change of
a field or of that size, and held between. It has no sound. It declares
`alpha`, so it goes over opaque items in the scene; see
[transparent items](../explanation/how-a-scene-reaches-the-compositor.md#transparent-items).

## The template file

An SVG document, at most 4 MiB, with:

* a `viewBox`, or a `width` and `height`. That is the size it was designed at,
  reported as `width` and `height` by `template.list`.
* any number of `{{name}}` markers, in the words of an element or in an
  attribute value. A name is a lower case letter, then lower case letters,
  digits and `_`, at most 40 characters. At most 64 fields.
* optionally, a `<metadata>` block in the `https://godwinmix.dev/ns/template`
  namespace saying what each field is.

```xml
<svg xmlns="http://www.w3.org/2000/svg" xmlns:gmx="https://godwinmix.dev/ns/template"
     viewBox="0 0 1920 1080" width="1920" height="1080">
  <metadata>
    <gmx:template title="Breaking news bar" description="A label and one headline"/>
    <gmx:field name="headline" label="Headline" default="Polls close at ten"/>
    <gmx:field name="accent" label="Accent colour" type="color" default="#d4202c"/>
  </metadata>
  <rect x="96" y="900" width="1728" height="96" fill="{{accent}}"/>
  <text x="128" y="966" font-size="52" fill="#fff" data-fit-width="1664">{{headline}}</text>
</svg>
```

| Element or attribute | Meaning |
|---|---|
| `<gmx:template title description>` | what `template.list` and the UI call it |
| `<gmx:field name label type default>` | one field. `type` is `text` (the default) or `color` (`colour` is read too) |
| `data-fit-width="W"` on a `<text>` | shrink to fit: see below. Every field inside that text reports `fit: W` |

A marker with no `<gmx:field>` is a text field labelled by its name with an
empty default, except `accent`, `text` and `panel`, which are colours.

### Values are escaped

A value is put into the SVG with `&`, `<`, `>`, `"` and `'` escaped, and any
control character but a tab or a new line made a space. A headline reading
`Q&A <live>` shows exactly that, and no value can add an element or close an
attribute.

### Shrink to fit

A `<text>` with `data-fit-width="W"` is measured, with the font, weight and
letter spacing it is drawn in, every time it is drawn. Narrower than `W`
template units, it is left exactly as written. Wider, it is scaled down as a
whole, height too, about its own `x` and `y`: a left aligned text keeps its
left edge, a centred one (`text-anchor="middle"`) its middle, and the baseline
does not move. Words are never wrapped; a template that needs two lines has
two fields.

### Pictures

`href` and `xlink:href` may be a `data:` URI or the name of a file in the media
library (`href="logo.png"`), which is read and inlined each time the template
is drawn. A template naming an `http:`, `https:` or any other address is
refused when it is read: nothing is fetched while a graphic is on air.

### What is not drawn

The template is drawn as a still. `<animate>`, SMIL and CSS animation are
ignored, as is CSS `text-transform`. Fonts are the ones installed on the
mixer; give a list that ends in a generic family. Movement on and off screen is
the scene item's `enter` and `exit`; see [transitions](transitions.md).

## The pack

Built into the binary from `graphics/` in the repository, read only, and
updated with the mixer. Each is designed on 1920 by 1080, transparent outside
its graphic, with every word inside title safe (96 to 1824 across, 54 to 1026
down). Place it over the whole canvas.

| Name | Title | Fields beyond the three colours |
|---|---|---|
| `news-lower-third` | News lower third | `name`, `title` |
| `breaking-news` | Breaking news bar | `label`, `headline` |
| `headline-strap` | Two line headline strap | `topic`, `headline`, `subhead` |
| `score-bug` | Score bug | `home`, `score_home`, `away`, `score_away`, `clock`, `period` |
| `logo-bug` | Logo bug | `station`, `tag` |
| `title-card` | Full screen title card | `kicker`, `title`, `subtitle` |
| `quote-card` | Quote card | `line1`, `line2`, `line3`, `attribution` |
| `location-tag` | Location and weather tag | `place`, `detail` |

Every one also has `accent`, `text` and `panel`. `template.list` gives each
field's label and default.

## `[graphics]`

The station's brand, used by every template whose source does not set the
field itself. Read at start.

| Key | Default | Meaning |
|---|---|---|
| `accent` | empty | the strong colour: a bar, a stripe, a label block |
| `text` | empty | the colour of words on a panel |
| `panel` | empty | the colour of the panels behind the words |

Empty means each template's own default. A value that is not a colour is
logged and the three are ignored until it is fixed.

## Methods and tools

| Method | MCP tool | Scope | What it does |
|---|---|---|---|
| `template.list` | `list_templates` | read | the pack and every SVG in the media library with a `{{` or a `<gmx:template>` in it, each with `uri`, size and fields. A library file that would not read is in `errors` with why |
| `template.get {name}` | `get_template` | read | one template and its `svg` as written |
| `template.save {name, svg, replace?}` | `save_template` | operate | checks the SVG reads as a template, then writes it into the media library as `name` (`.svg` added). Refuses to write over a file unless `replace`; with `replace`, every source drawing it is drawn again on air, and `redrawn` lists them |
| `template.fields {id}` | `template_fields` | read | a running graphic's fields, each with `value` (what is on screen) and `set` (whether the source set it), and `path`, `params.fields.<name>` |
| `scene.preview.frame {width?}` | `preview_frame` | read | the armed preview scene as a JPEG, which over MCP is an image the model sees |

All four template tools and `preview_frame` are behind `search_tools`, so the
hot list is the size it was. Adding the graphic is `source.add`
(`add_source`), placing it is `scene.item.add` (`add_scene_item`), which takes
`visible`, `enter` and `exit`, and changing a field is `source.set`
(`set_source`).

From a terminal:

```sh
gmx ctl template list
gmx ctl template get <name> [--out file.svg]
gmx ctl template save <name> <file.svg> [--replace]
gmx ctl template fields <source-id>
gmx ctl source set <id> --param fields.headline="Polls close"
gmx ctl rpc <method> '<json>' | @file.json
```

## What it costs

Measured with `crates/godwinmix-core/tests/template_cost.rs` on a 1280 by 720,
30 fps programme of colour bars with the encoder off, as process CPU over ten
seconds in percent of one core:

COST_TABLE

A field change draws the SVG once, a few milliseconds of one overlay worker
thread, and nothing else changes. Between changes the board blends only the
part of the picture with something in it.
