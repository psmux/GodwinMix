# Graphics for agents

What a model has to produce for each kind of on screen graphic, written so a
small model can follow it: short rules, one example per kind that can be
copied whole, and a check that says what to fix. The skill
`godwinmix-design` points here.

The fastest good result is never a blank page. Read a starter design with
`get_template`, change its words and colours, save it under a new name, and
check it. Every design in the pack passes the check with nothing to say.

## Pick the kind

| You want | Make | Cost on a laptop with no GPU |
|---|---|---|
| Words in a box, a crawl | a `text:` or `ticker:` source | almost nothing |
| A designed graphic that holds still: strap, bug, card, desk | an SVG template, `template:<name>` | almost nothing; drawn once per change |
| Anything that moves: an animated lower third, a ticker, a clock, a countdown, 3D, a looping background | an HTML template, `html:<name>` | a few percent of a core while it moves, next to nothing while it holds (numbers below) |
| A picture with transparency | a PNG or WebP with alpha in the media library | almost nothing |
| A pre rendered animation (from After Effects, Blender) | WebM with VP9 alpha, or ProRes 4444, in the media library | one video decode |

Start cheap. Use HTML only when the graphic really moves or needs live data
drawn by script. Movement in and out alone does not need HTML: an SVG
template on a scene item with an `enter` and an `exit` slides and fades.

## HTML templates

### The rules

1. **One `.html` file.** Laid out on a 1920 by 1080 page. The mixer scales it
   to its canvas, so a 1280 by 720 show draws the same design smaller.
2. **A `gmx-template` block in `<head>`.** JSON, saying what it is and what its
   fields are:

   ```html
   <script type="application/json" id="gmx-template">
   {"title": "My lower third", "category": "lower-third", "out_ms": 600,
    "fields": {"name":   {"label": "Name", "default": "Ada Lovelace"},
               "accent": {"label": "Accent colour", "type": "color", "default": "#e4572e"}}}
   </script>
   ```

   | Key | Meaning |
   |---|---|
   | `title`, `description` | what the list and the gallery show |
   | `category` | `lower-third`, `ticker`, `score`, `bug`, `logo`, `countdown`, `title`, `slate`, `background` or `set` |
   | `fields` | one entry a field. A name is lower case letters, digits and `_`. `type` is `text` (the default), `color` or `image` (a picture in the media library, by file name). Give every field a `default` |
   | `out_ms` | how long the way out takes, in milliseconds |
   | `opaque` | `true` for a design that covers the whole picture on purpose: a background, a title card, a slate |
   | `fps` | the most frames a second it needs, 1 to 60. Say 20 for a slow background |

   Name the colour fields `accent`, `text` and `panel` and the station's
   brand colours (`[graphics]` in the config) fill them when a source does not.
3. **A transparent page.** `html, body { margin: 0; background: transparent; }`.
   Colour only the parts of the graphic. A design with `"opaque": true` may
   paint everything.
4. **Show a field** with `data-field="name"` on the element that holds it (on an
   `<img>` it sets the picture), or as `var(--name)` in CSS. Both are filled
   when the page loads and again every time the field changes on air, with no
   reload.
5. **A way in and a way out.** Hidden by default, shown under `.gmx-in`. The
   mixer puts `gmx-in` on `<html>` when an item showing the graphic goes on the
   programme, and swaps it for `gmx-out` when it comes off. CSS transitions do
   the moving. Make `out_ms` the length of the way out.
6. **Offline.** Nothing from the network: no web fonts, no CDN scripts, no
   `https://` anywhere. The renderer refuses every host name. Use a font every
   machine has (`Inter, "Segoe UI", "Helvetica Neue", Helvetica, Arial,
   "DejaVu Sans", sans-serif`); put a font file or a picture in the media
   library and name it by file name (`src="logo.png"`).
7. **Light.** Move with `transform` and `opacity`. Run a loop (a crawl, a
   clock, WebGL) only while the graphic is in: start it on `gmx:in`, stop it on
   `gmx:out`. A page that holds still sends nothing and costs next to nothing.
8. **No `alert`, `confirm` or `prompt`.** Nobody is there to press OK.

### A script, when CSS is not enough

```js
window.addEventListener("gmx:update", e => { /* e.detail is every field: e.detail.items */ });
window.addEventListener("gmx:in",  () => { /* start a loop */ });
window.addEventListener("gmx:out", () => { /* stop it */ });
// window.gmx.fields and window.gmx.cue ("in" or "out") hold the same.
```

`gmx:update` fires once when the page has loaded and again after each change.

### Example: a lower third

The whole file. Copy it, change the words, the colours and the layout.

```html
<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<script type="application/json" id="gmx-template">
{"title": "Simple strap", "category": "lower-third", "out_ms": 500,
 "fields": {"name":   {"label": "Name", "default": "Ada Lovelace"},
            "title":  {"label": "Role", "default": "Engine research"},
            "accent": {"label": "Accent colour", "type": "color", "default": "#e4572e"},
            "panel":  {"label": "Panel colour", "type": "color", "default": "#0f172a"},
            "text":   {"label": "Text colour", "type": "color", "default": "#ffffff"}}}
</script>
<style>
  html, body { margin: 0; background: transparent; overflow: hidden; }
  body { font-family: Inter, "Segoe UI", Helvetica, Arial, "DejaVu Sans", sans-serif; }
  .strap { position: absolute; left: 120px; top: 820px; padding: 18px 40px;
           background: var(--panel); border-left: 12px solid var(--accent); color: var(--text);
           transform: translateX(-130%); transition: transform .5s ease-in; }
  .gmx-in .strap { transform: none; transition: transform .6s cubic-bezier(.2, .8, .2, 1); }
  .name { font-size: 56px; font-weight: 800; }
  .title { font-size: 30px; opacity: .85; }
</style></head>
<body>
  <div class="strap"><div class="name" data-field="name">Ada Lovelace</div>
                     <div class="title" data-field="title">Engine research</div></div>
</body></html>
```

### Example: a crawl that loops

The parts that differ from the strap: a field read by script, and a loop that
runs only while it is in.

```html
<style>
  .window { position: absolute; left: 0; right: 0; top: 990px; height: 66px; overflow: hidden; background: var(--panel); }
  .strip { position: absolute; white-space: nowrap; font-size: 32px; line-height: 66px; color: var(--text);
           animation: crawl 30s linear infinite paused; }
  .gmx-in .strip { animation-play-state: running; }
  @keyframes crawl { from { transform: translateX(0); } to { transform: translateX(-50%); } }
</style>
<div class="window"><div class="strip" id="strip"></div></div>
<script>
  window.addEventListener("gmx:update", e => {
    const items = String(e.detail.items).split("|");
    document.getElementById("strip").textContent = (items.join("   ·   ") + "   ·   ").repeat(2);
  });
</script>
```

### Example: a full screen background

```html
<script type="application/json" id="gmx-template">
{"title": "Slow glow", "category": "background", "opaque": true, "fps": 20,
 "fields": {"accent": {"type": "color", "default": "#4f46e5"}, "panel": {"type": "color", "default": "#0b1026"}}}
</script>
<style>
  html, body { margin: 0; height: 100%; overflow: hidden; background: var(--panel); }
  .glow { position: absolute; left: -200px; top: -400px; width: 1400px; height: 1400px; border-radius: 50%;
          background: radial-gradient(circle, var(--accent), transparent 65%);
          animation: drift 25s ease-in-out infinite alternate paused; }
  .gmx-in .glow { animation-play-state: running; }
  @keyframes drift { to { transform: translate(600px, 300px); } }
</style>
<div class="glow"></div>
```

### 3D

CSS 3D (`perspective`, `rotateY`, `preserve-3d`) is the cheap way and works
everywhere: a flipping ticker, a turning card, a floor grid. For a real
model, WebGL works too, with or without a GPU: with none, Chromium draws it on
the CPU (WARP on Windows, SwiftShader on Linux and macOS). Keep the canvas to
the size of the object, check `WEBGL_debug_renderer_info` for a software
renderer and halve the canvas there, and draw only while in. `logo-spin-3d`
and `title-card-3d` are whole WebGL examples in about 100 lines with no
library. There is no three.js built in, and a page cannot fetch one.

## The starter pack

`list_templates` lists them with every field. HTML designs are added as
`html:<name>`, SVG ones as `template:<name>`. The files are in the repository
under `graphics/html/` and `graphics/`.

| Name | Format | What it is |
|---|---|---|
| `lower-third-glass` | HTML | name and role on a dark glass panel, wipes in from the left |
| `lower-third-bold` | HTML | big name on a solid block with a slanted role tab, pushes in |
| `lower-third-line` | HTML | minimal: a line draws, the words rise out of it |
| `ticker-crawl` | HTML | label and headlines crawling right to left; `items` split by `|` |
| `ticker-flip` | HTML | one headline at a time, flipping over in 3D |
| `score-bug-live` | HTML | two teams, the score (a change flashes) and a clock that runs itself |
| `logo-bug-shine` | HTML | station mark in the corner with a light sweeping across every few seconds |
| `countdown-ring` | HTML | transparent countdown in a ring that empties, from a duration or to a time of day |
| `logo-spin-3d` | HTML, WebGL | the station's mark on a spinning 3D coin |
| `title-card-3d` | HTML, WebGL, opaque | full screen title beside a turning crystal |
| `starting-soon` | HTML, opaque | holding slate with a countdown |
| `background-gradient` | HTML, opaque | drifting colour loop, for behind a cut out presenter |
| `background-particles` | HTML, opaque | drifting points of light |
| `set-newsroom` | HTML, opaque | virtual set background: newsroom wall and floor |
| `set-studio` | HTML, opaque | virtual set background with depth: floor grid, turning ring |
| `set-newsroom-desk` | SVG | the newsroom's anchor desk, in front of the presenter |
| `set-studio-frame` | SVG | the studio's pillars and console edge, in front of the presenter |

The eight SVG templates for news (`news-lower-third`, `breaking-news` and the
rest) are in [graphic templates](graphic-templates.md).

## Put one on air

```
check_template {"html": "<the whole page>"}                          -> {"ok": true, ...}
save_template  {"name": "ours-strap", "html": "<the whole page>"}    -> uri html:ours-strap.html
add_source     {"id": "strap", "uri": "html:ours-strap.html", "params": {"fields": {"name": "Ada Lovelace"}}}
add_scene_item {"scene": "studio", "content": {"source": "strap"}, "name": "strap", "visible": false,
                "transform": {"position": {"x": 0, "y": 0}, "frame": {"w": 1920, "h": 1080}},
                "exit": {"type": "hold", "duration_ms": 600}}
set_scene_item {"scene": "studio", "item": "strap", "props": {"visible": true}}   # plays its way in
set_source     {"id": "strap", "params": {"fields": {"name": "Grace Hopper"}}}  # new words, no reload
set_scene_item {"scene": "studio", "item": "strap", "props": {"visible": false}}  # plays its way out
```

* Place it over the whole canvas: the design is laid out on the whole page.
* `"exit": {"type": "hold", "duration_ms": <out_ms>}` keeps the item drawn while
  the page plays its own way out, then takes it away. Without it the item goes
  at once and the way out is not seen. Add `"on_take": true` to do the same
  when another scene is taken.
* Taking a scene that shows it plays its way in, and taking it off plays its
  way out. To look at one off air, set `"cue": "in"` in its params (`"auto"`
  puts it back).
* Look before you say it is done: `snapshot {"id": "program", "width": 1280}`
  once it is shown, or `arm_preview` and `preview_frame` with `cue` held in.

## The check

`check_template` (`template.check`) reads a template the way saving and drawing
it would and writes nothing. `save_template` and `add_source` refuse a
template with an error in it, with the same message. Each problem says what
was found and what to write instead:

| Found | Fix it says |
|---|---|
| `body { background: #000 }` on an overlay | `html, body { background: transparent }`, or `"opaque": true` for a full screen design |
| `data-field="headline"` with no `headline` field declared | add it to `fields`, or use a declared one (it lists them) |
| `https://fonts.googleapis.com/...` | use an installed font, put files in the media library |
| no `gmx-template` block, or one that is not JSON | the block to paste, with double quotes and no trailing comma |
| a colour default that is not a colour | `#rrggbb` |
| `alert(` | take it out |
| (warning) nothing under `.gmx-in` | hide it by default and show it under `.gmx-in` |
| (warning) no `out_ms` | the length of the way out |
| (warning) a field declared and never shown | `data-field` or `var(--name)` |

Fix every error, check again, and save when `ok` is true.

## Pictures and video with transparency

* **PNG, WebP or SVG with alpha:** upload it (`godwinmix ctl upload logo.png`)
  and add it as a source by file name. Its clear parts show the picture under
  it.
* **Video with alpha:** WebM with VP8 or VP9 alpha, ProRes 4444 or QuickTime
  Animation. From a PNG sequence or a ProRes master:

  ```sh
  ffmpeg -framerate 30 -i frame_%04d.png -c:v libvpx-vp9 -pix_fmt yuva420p -b:v 2M -auto-alt-ref 0 sting.webm
  ```

  A numbered PNG sequence (`frames/%04d.png`) plays as a source, but flat: its
  transparency is lost on the way through the compositor. Make it a WebM
  first when it has to go over the picture.

## Virtual sets

A virtual set is a scene, made with `scene.create_from` (`create_scene_from`)
and the `virtual-set` layout. Four slots, back to front:

| Slot | What goes there | Size and transparency |
|---|---|---|
| `a`, the set | the background plate: a still, a looping clip, or an opaque HTML design | 1920 by 1080, opaque, fills the canvas |
| `b`, the presenter | the camera. It is keyed (green or blue screen) or cut out (`"screen": "none"`) | stands on the bottom edge, in the middle, 90 percent of the canvas height: the box from x 96 to 1824, y 108 to 1080 |
| `c`, the foreground | a desk, a frame: anything in front of the presenter | 1920 by 1080, transparent everywhere but the desk |
| `d`, the lower third | a source for the strap | the box from x 96 to 1248, y 799 to 972 |

Design rules for a set:

* The presenter's head and shoulders are in the middle third, from about
  y 220 down. Keep the background's busiest part (a logo, words) above or
  beside it; put the station's name high on the wall.
* The foreground covers only the bottom of the presenter: a desk top at about
  y 820 to 870 across the middle (x 300 to 1620) and its front down to the
  bottom edge. Everything else in it is transparent.
* Use the same `accent` and `panel` colours in the background and the
  foreground.
* Keep the background slow: `"fps": 20`, nothing that flickers, no light
  sweeping behind the presenter's head.

Assemble one in four calls:

```
add_source {"id": "set-bg",   "uri": "html:set-newsroom",          "params": {"fields": {"station": "NEWS 24", "accent": "#d4202c"}}}
add_source {"id": "set-desk", "uri": "template:set-newsroom-desk", "params": {"fields": {"station": "NEWS 24", "accent": "#d4202c"}}}
create_scene_from {"layout": "virtual-set", "name": "Newsroom", "sources": ["set-bg", "cam1", "set-desk"],
                   "settings": {"screen": "green"}}
take {"scene": "Newsroom"}
```

`screen` is `green`, `blue` or `none` (no screen: the person is cut out by a
model). `presenter_scale` (0.3 to 1) and `presenter_x` (0 to 1) in `settings`
move the presenter. The key is tuned on the scene's `presenter` item; see
[put a presenter on a green screen](../how-to/green-screen-presenter.md).

## What it costs

Measured on the development laptop (Intel Core Ultra 9 285H, 16 cores,
Windows 11, no GPU used by the renderer), 1080p30 canvas, a percentage of one
core for the renderer's whole process tree:

COSTS_TABLE

## What is not done

* Verified end to end on Windows only. The renderer's graphic mode and the
  kind are written for macOS and Linux too, and nothing in them is Windows
  specific, but they have not been run there.
* The renderer runs while the source exists, on air or not. A design that
  stops its loops when it is out costs little off air, but a page is still
  loaded. Remove a graphic you are done with.
* `cue` follows the programme only. The preview of a scene that is not on air
  shows an HTML graphic in its out state unless its params say `"cue": "in"`.
