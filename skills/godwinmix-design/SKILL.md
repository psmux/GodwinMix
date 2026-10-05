---
name: godwinmix-design
description: Design on screen graphics for a GodwinMix mixer and put them on air, drawn by the mixer itself with no browser. Use when asked to make, brand, restyle or reword a lower third, a breaking news bar, a news ticker or crawl, a score bug, a logo bug, a title card, a background, a virtual set or news studio set for a presenter (with or without a green screen), or to choose between a text source, an SVG template and an OGraf page. Covers the built in pack, writing an SVG template with {{fields}} and shrink to fit, safe areas, save_template, placing it on the scene that is on air with an enter and an exit, the virtual-set layout, looking at the result with snapshot and preview_frame, and changing its words on air with set_source.
---

# Designing graphics for a GodwinMix mixer

A graphic here is an SVG with named fields in it, `{{headline}}`, drawn by the
mixer at the size it is placed at, once per change, and held. A field changed
on air is drawn again in a few milliseconds and swapped in on the next frame,
with no rebuild and no gap. It costs a Raspberry Pi almost nothing while it
holds still, so it is the default for anything designed.

## Calling the tools

Over MCP, call the tools by name. A tool that is not in your list (`get_scene`,
`list_layouts`, `template_fields`) runs through `call_tool`:
`call_tool {"name": "get_scene", "arguments": {"scene": "Live"}}`.
`search_tools {"query": "..."}` finds one by what it does.

No MCP? Every tool is also a command, with its arguments as one JSON object
(pi, or any agent with a shell):

```
godwinmix tool agent_state
godwinmix tool add_source '{"id": "lower", "uri": "template:news-lower-third"}'
```

It finds the GodwinMix app's mixer on the same machine by itself. A file goes
into the media library with `godwinmix ctl upload picture.png`.

## Pick the cheapest thing that does the job

| Ask | Use | Why |
|---|---|---|
| Plain words in a box, a crawl, credits | `text:` or `ticker:` source | the cheapest there is; see `godwinmix-operate` |
| A designed graphic: panels, an accent colour, a hierarchy of type, a logo | an SVG template, `template:<name>` | native, cheap, branded, live fields |
| Something that moves inside itself every frame: a ticking clock face, a particle wipe, a chart that animates | OGraf (`scene.apply_graphic`) | a Chromium page per graphic; too heavy for a Pi |

Movement in and out is not a reason to reach for OGraf. Every scene item has
an `enter` and an `exit` (fade, slide, wipe, zoom), and they move templates
like anything else. Do not put `<animate>` or CSS animation in a template:
it is drawn once, as a still.

## The pack

`list_templates {}` answers with every template and its fields. Eight are
built in:

| Name | Fields |
|---|---|
| `news-lower-third` | `name`, `title` |
| `breaking-news` | `label`, `headline` |
| `headline-strap` | `topic`, `headline`, `subhead` |
| `score-bug` | `home`, `score_home`, `away`, `score_away`, `clock`, `period` |
| `logo-bug` | `station`, `tag` |
| `title-card` | `kicker`, `title`, `subtitle` (full screen, opaque) |
| `quote-card` | `line1`, `line2`, `line3`, `attribution` |
| `location-tag` | `place`, `detail` |

Every one also has `accent`, `text` and `panel`, three colours. Set them per
graphic in `fields`, or once for the whole station in the config's
`[graphics]` section, which every template falls back to.

Each is laid out on a 1920 by 1080 canvas with a transparent background and
sits inside title safe, so placed over the whole canvas it lands where a
broadcast graphic belongs. Only the part with something in it costs anything
to draw.

## Put one on air

Names in `code` are MCP tools; the CLI is at the end.

0. `agent_state {}`. `program` is what is on air. If it names a scene, use
   that scene below. If it names a source (`cam1`), make a scene of it once,
   then take it; the picture does not change:

   ```
   create_scene_from {"sources": ["cam1"], "name": "Live"}
   take {"scene": "Live"}
   ```

1. Add the graphic as a source, with its words:

   ```
   add_source {"id": "breaking", "uri": "template:breaking-news",
               "params": {"fields": {"label": "BREAKING", "headline": "Storm warning for the coast tonight"}}}
   ```

2. Put it on the scene that is on air, hidden, with its way in and out:

   ```
   add_scene_item {"scene": "Live", "content": {"source": "breaking"}, "name": "breaking bar",
     "visible": false,
     "enter": {"type": "slide", "edge": "left", "duration_ms": 400, "easing": "ease-out"},
     "exit": {"type": "fade", "duration_ms": 300}}
   ```

   With no `transform` a template covers the whole canvas, which is where it
   was designed to sit. Leave out `visible` and it is on air at once.

3. Look before it airs (next section).
4. Show it: `set_scene_item {"scene": "Live", "item": "breaking bar", "props": {"visible": true}}`.
   It slides in. `false` takes it out the way `exit` says.
5. Change the words on air:

   ```
   set_source {"id": "breaking", "params": {"fields": {"headline": "Roads closed on the coast road"}}}
   ```

   Only the fields you name change; the rest stay. `null` puts one back to its
   default. A value lives at `params.fields.<name>`, the path any client or
   data feed sets. `template_fields {"id": "breaking"}` says what each field shows now.
   A field the template does not have is refused with the ones it does have in
   `data.fields`.

A colour named but not given ("my brand blue") is not stored anywhere unless
`[graphics] accent` is set. Pick a fitting one, put it on air as asked, and
say which hex you used and how to change it, rather than stopping to ask.

## Look at your own work

Always look once before you tell anyone it is done. Just after a take, or
while the pictures start ("no mosaic frame yet"), wait a second and look
again before you call anything black.

```
arm_preview {"scene": "studio"}
preview_frame {"width": 1280}
```

`preview_frame` answers with the armed scene as a picture. A hidden item is
not in it, so show the item on a scene that is not on air, or look at the
programme after step 4 with `snapshot {"id": "program", "width": 1280}`.
1280 wide is about 1,200 tokens and is what reading a lower third needs; 320
is enough to see where things are. Check: are the words inside their panel,
is anything cut at an edge, does it read against the picture under it. Fix,
look again, and stop when it is right.

## Write your own template

Start from a pack template rather than a blank page:

```
get_template {"name": "news-lower-third"}
```

Change it, then save it into the media library:

```
save_template {"name": "news24-strap", "svg": "<svg ...>...</svg>"}
  -> {"template": {"uri": "template:news24-strap.svg", "fields": [...]}, "path": "...", "redrawn": []}
```

`save_template` checks the SVG renders before it writes anything. With
`"replace": true` it writes over a template of the same name and every source
drawing it is drawn again on air, which is how you fix a design you are
looking at.

The rules a template follows:

* `viewBox="0 0 1920 1080"` and the same `width` and `height`, transparent
  where there is nothing. Design on the whole canvas and place the item over
  the whole canvas.
* A field is `{{name}}`: lower case letters, digits and `_`. It may be in the
  words of a `<text>` or in an attribute (`fill="{{accent}}"`). Values are
  escaped for XML, so a headline with `&` or `<` shows those characters.
* Declare each field in `<metadata>` with a label, a default and, for a
  colour, `type="color"`. Put `xmlns:gmx="https://godwinmix.dev/ns/template"`
  on the `<svg>`:

  ```xml
  <metadata>
    <gmx:template title="NEWS 24 strap" description="Name and title, bottom left"/>
    <gmx:field name="headline" label="Headline" default="Polls close at ten"/>
    <gmx:field name="accent" label="Accent colour" type="color" default="#c8102e"/>
  </metadata>
  ```

* Shrink to fit: `data-fit-width="W"` on a `<text>` makes the words smaller,
  about the text's own `x` and `y`, whenever they come out wider than `W` in
  canvas units. Give every field that holds a headline or a name one. A
  template does not wrap words: give a long text two or three fields, one a
  line, as `quote-card` does.
* Fonts are the ones installed on the mixer. Write a list that ends in a
  generic family: `font-family="Inter, 'Helvetica Neue', Helvetica, Arial,
  'DejaVu Sans', 'Liberation Sans', sans-serif"`. A Raspberry Pi has DejaVu.
  Write words in capitals yourself; `text-transform` is not drawn.
* A logo is a data URI (`href="data:image/png;base64,..."`) or the file name
  of a picture in the media library (`href="logo.png"`). Nothing is fetched
  from the network, and a template that names an `https://` address is
  refused.
* Weight by size and boldness: one large bold line, one smaller regular line,
  a small capitals label. Leave 28 to 32 units between words and the edge of
  their panel. Use `accent` for one strong shape, not everything.

### Safe areas, on 1920 by 1080

| Area | Share | x from, to | y from, to |
|---|---|---|---|
| Title safe: all words | 90 percent | 96 to 1824 | 54 to 1026 |
| Action safe: anything that matters | 93 percent | 67 to 1853 | 38 to 1042 |

A full screen background may run to the edges; its words stay in title safe.

## Three requests and the calls that answer them

**"A red breaking news bar for our channel, called NEWS 24."**

```
add_source {"id": "breaking", "uri": "template:breaking-news",
  "params": {"fields": {"label": "NEWS 24", "headline": "<the story>", "accent": "#d4202c"}}}
add_scene_item {... "content": {"source": "breaking"}, "visible": false, "enter": {"type": "wipe", "edge": "left", "duration_ms": 350}, ...}
set_scene_item {... "props": {"visible": true}}
snapshot {"id": "program", "width": 1280}
```

**"Put up the score, Arsenal 2 Chelsea 1, 67 minutes, and keep the clock going."**

```
add_source {"id": "score", "uri": "template:score-bug",
  "params": {"fields": {"home": "ARS", "score_home": 2, "away": "CHE", "score_away": 1, "clock": "67:00", "period": "2ND HALF"}}}
set_source {"id": "score", "params": {"fields": {"clock": "67:01"}}}
```

A clock changed once a second is one small render a second. For a running
clock, change it once a second, not more.

**"Our brand is green and gold. Make the lower third ours, with the logo from the media library."**

```
get_template {"name": "news-lower-third"}
save_template {"name": "ours-lower-third", "svg": "<the pack SVG with an <image href=\"logo.png\" .../> added beside the name>"}
add_source {"id": "strap", "uri": "template:ours-lower-third.svg",
  "params": {"fields": {"name": "Ada Lovelace", "title": "Engine Research", "accent": "#0b6e3d", "panel": "#0d1f16", "text": "#ffffff"}}}
```

Then place, look and show as above. To make green the whole station's colour,
set `graphics.accent`, `graphics.text` and `graphics.panel` in Settings; every
template uses them from the next start.

**"An animated news ticker with these three headlines."**

A ticker is a source the mixer draws and moves itself, not a template:

```
add_source {"id": "ticker", "uri": "ticker:", "params": {"items": ["First headline", "Second", "Third"], "speed": 120, "background": "#0b1f3a"}}
add_scene_item {"scene": "Live", "content": {"source": "ticker"}, "name": "ticker",
  "transform": {"position": {"x": 0, "y": 1000}, "frame": {"w": 1920, "h": 80}}}
set_source {"id": "ticker", "params": {"items": ["New first headline", "Second", "Third"]}}
```

**"Design a background for my presenter."** A full screen template: an SVG
with no transparent part (a gradient, panels, a skyline, the show's name),
saved with `save_template` and added as a source like any other:

```
save_template {"name": "studio-bg", "svg": "<svg viewBox=\"0 0 1920 1080\" width=\"1920\" height=\"1080\" ...>...</svg>"}
add_source {"id": "studio-bg", "uri": "template:studio-bg.svg"}
```

**"Make me a modern news studio set and put me in it, no green screen."**
There is no separate virtual set feature: a set is a background, the camera
cut out in front of it, and a scene made in one call with the `virtual-set`
layout. Design the background as above, then:

```
create_scene_from {"sources": ["studio-bg", "cam1"], "layout": "virtual-set", "name": "Studio",
  "settings": {"screen": "none", "presenter_scale": 0.8, "presenter_x": 0.35}}
take {"scene": "Studio"}
snapshot {"id": "program", "width": 640}
```

`screen: "none"` cuts the person out with a model, so no green screen is
needed; leave it out for a green screen and say `"blue"` for a blue one. A
desk in front of the presenter is a third source, a template with a
transparent top half. A lower third goes on the `Studio` scene as above.

## From a terminal

```sh
gmx ctl template list
gmx ctl template get news-lower-third --out strap.svg
gmx ctl template save ours-lower-third strap.svg            # --replace to write over it
gmx ctl source add strap template:ours-lower-third.svg --param fields.name="Ada Lovelace"
gmx ctl source set strap --param fields.title="Engine Research"
gmx ctl template fields strap
gmx ctl rpc scene.item.add @item.json                        # any method, params as JSON or @file
```

## What goes wrong

* **The graphic draws over a camera that is above it in the scene.** Every
  transparent item is drawn over every opaque one. Among graphics, the stack
  order holds.
* **A long headline is tiny.** It was shrunk to fit. Shorten it, or use the
  two line strap.
* **On a GPU compositor** (`[hardware] graphics` set to a GPU entry) a
  template is drawn flat, its clear parts grey. The software compositor is the
  default.
* **`there is no template`**: the error lists the pack; `list_templates` lists
  the library too.
