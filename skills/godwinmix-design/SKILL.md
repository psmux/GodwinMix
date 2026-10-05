---
name: godwinmix-design
description: Design on screen graphics for a GodwinMix mixer and put them on air. Use when asked to make, brand, restyle or reword a lower third, a breaking news bar, a ticker, a score bug, a logo bug, a countdown, a title card, a starting soon slate, a looping background, a 3D or animated graphic, or a virtual set behind a presenter. Covers choosing between a text source, an SVG template (a still, almost free) and an HTML template (motion, 3D, live data, transparent), the starter packs, copying and restyling one with get_template and save_template, check_template, placing it with an enter and a hold exit, building a set with create_scene_from, looking at the result, and changing its words on air with set_source.
---

# Designing graphics for a GodwinMix mixer

A graphic here is a template with named fields. Two formats:

* **SVG** (`template:<name>`): drawn by the mixer once per change and held.
  Costs a Raspberry Pi almost nothing. The default for anything that holds
  still.
* **HTML** (`html:<name>`): a web page drawn by the browser renderer with its
  transparency kept, for anything that moves: CSS animation, a crawl, a clock,
  WebGL 3D, its own way in and out. A held page costs next to nothing; a small
  moving one 10 to 30 percent of a core; full screen motion up to most of one.
  On a Raspberry Pi keep to SVG, stills and video loops.

A field changed on air reaches the screen on the next frame either way, with
no rebuild and no reload.

## No MCP? Run each tool as a command

Every tool this skill names is also a command, with its arguments as one JSON
object, for an agent that has a shell and no MCP (pi, or any other):

```
godwinmix tool agent_state
godwinmix tool add_source '{"name": "lower", "uri": "template:news-lower-third"}'
```

It finds the GodwinMix app's mixer on the same machine by itself. A file goes
into the media library with `godwinmix ctl upload picture.png`.

## Pick the cheapest thing that does the job

| Ask | Use | Why |
|---|---|---|
| Plain words in a box, a crawl, credits | `text:` or `ticker:` source | the cheapest there is; see `godwinmix-operate` |
| A designed graphic that holds still: panels, an accent colour, a logo | an SVG template, `template:<name>` | native, cheap, branded, live fields |
| A graphic that moves: animated strap, flip ticker, running clock, countdown, 3D logo, looping background, set backdrop | an HTML template, `html:<name>` | real animation and alpha; costs while it moves |
| A pre rendered animation | WebM with VP9 alpha in the media library | one video decode |

Movement in and out alone is not a reason for HTML. Every scene item has an
`enter` and an `exit` (fade, slide, wipe, zoom), and they move SVG templates
like anything else. Do not put `<animate>` or CSS animation in an SVG
template: it is drawn once, as a still.

Never start from a blank page. `get_template` a starter design, change its
words and colours, save it under a new name, check it.

## The pack

`list_templates {}` answers with every template, its `format` and its
fields. Built in, SVG (`template:<name>`):

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
| `set-newsroom-desk`, `set-studio-frame` | set foregrounds (see Virtual sets) |

Built in, HTML (`html:<name>`): `lower-third-glass`, `lower-third-bold`,
`lower-third-line` (`name`, `title`), `ticker-crawl` and `ticker-flip`
(`label`, `items` split by `|`), `score-bug-live` (teams, scores, a `clock`
that runs itself while `running` is yes), `logo-bug-shine`, `countdown-ring`
(`duration` mm:ss or `until` hh:mm), `logo-spin-3d` (WebGL), `title-card-3d`,
`starting-soon`, `background-gradient`, `background-particles`, and the set
backgrounds `set-newsroom` and `set-studio`. Ask `list_templates` for each
one's fields.

Every one also has `accent`, `text` and `panel`, three colours. Set them per
graphic in `fields`, or once for the whole station in the config's
`[graphics]` section, which every template falls back to.

Each is laid out on a 1920 by 1080 canvas with a transparent background and
sits inside title safe, so placed over the whole canvas it lands where a
broadcast graphic belongs. Only the part with something in it costs anything
to draw.

## Put one on air

Five calls. Names in `code` are MCP tools; the CLI is at the end.

1. Add the graphic as a source, with its words:

   ```
   add_source {"id": "breaking", "uri": "template:breaking-news",
               "params": {"fields": {"label": "BREAKING", "headline": "Storm warning for the coast tonight"}}}
   ```

2. Put it on the scene that is on air, hidden, with its way in and out:

   ```
   add_scene_item {"scene": "studio", "content": {"source": "breaking"}, "name": "breaking bar",
     "transform": {"position": {"x": 0, "y": 0}, "frame": {"w": 1920, "h": 1080}},
     "visible": false,
     "enter": {"type": "slide", "edge": "left", "duration_ms": 400, "easing": "ease-out"},
     "exit": {"type": "fade", "duration_ms": 300}}
   ```

   Use the canvas size `core_info` reports for `frame`. Leave `transform` out
   and it lands in a grid cell instead, which is wrong for a lower third.

3. Look before it airs (next section).
4. Show it: `set_scene_item {"scene": "studio", "item": "breaking bar", "props": {"visible": true}}`.
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

## Look at your own work

Always look once before you tell anyone it is done.

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

## Write your own SVG template

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

## Write an HTML template

Copy a pack design: `get_template {"name": "lower-third-glass"}` gives its
whole `html`. The rules (all of them, with an example of each kind, are in
`docs/reference/graphics-for-agents.md`):

1. One `.html` file laid out on a 1920 by 1080 page.
2. In `<head>`, a JSON block declaring the fields:
   `<script type="application/json" id="gmx-template">{"title": "...", "category": "lower-third", "out_ms": 600, "fields": {"name": {"label": "Name", "default": "Ada"}, "accent": {"type": "color", "default": "#e4572e"}}}</script>`.
   Add `"opaque": true` for a full screen design and `"fps": 20` for a slow one.
3. `html, body { margin: 0; background: transparent; }` unless opaque.
4. Show a field with `data-field="name"` (text, or an `<img>`'s picture) or
   `var(--name)` in CSS. A script reads `e.detail` of the `gmx:update` event.
5. Hidden by default, shown under `.gmx-in`: the mixer adds `gmx-in` to
   `<html>` when it goes on the programme and `gmx-out` when it comes off.
   `out_ms` is how long the way out takes.
6. Nothing from the network: no web fonts, no CDN, no `https://`. Pictures and
   font files go in the media library and are named by file name.
7. Move with `transform` and `opacity`; run a loop only between `gmx:in` and
   `gmx:out`.

Check, then save:

```
check_template {"html": "<the page>"}        -> {"ok": false, "problems": [{"problem": "...", "fix": "..."}]}
save_template {"name": "ours-strap", "html": "<the page>"}   -> uri html:ours-strap.html
```

Fix every problem the check names and check again until `ok` is true. Place
it like an SVG one, with `"exit": {"type": "hold", "duration_ms": <out_ms>}`
so it stays drawn while its own way out plays. To see it off air, set
`params.cue` to `"in"` on the source; `"auto"` follows the programme again.

## Virtual sets

A set is a scene from the `virtual-set` layout: background, presenter (keyed
or cut out), foreground, in that order. The presenter stands on the bottom
edge, in the middle, at 90 percent of the height; a foreground covers only
the bottom (a desk from about y 820 down) and is transparent elsewhere.

```
add_source {"id": "set-bg", "uri": "html:set-newsroom", "params": {"fields": {"station": "NEWS 24", "accent": "#d4202c"}}}
add_source {"id": "set-desk", "uri": "template:set-newsroom-desk", "params": {"fields": {"station": "NEWS 24", "accent": "#d4202c"}}}
create_scene_from {"layout": "virtual-set", "name": "Newsroom", "sources": ["set-bg", "cam1", "set-desk"], "settings": {"screen": "green"}}
```

`screen` is `green`, `blue`, or `none` for a person with no screen behind
them. Use the same colours in both halves.

## Safe areas, on 1920 by 1080

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
* **An HTML graphic shows nothing.** It is out: the item is not on the
  programme, or the page has no `.gmx-in` rules. Set `params.cue` to `"in"` to
  look at it, and run `check_template` on it.
* **An HTML graphic covers the picture with black or white.** Its page paints a
  background. `check_template` says which line.
