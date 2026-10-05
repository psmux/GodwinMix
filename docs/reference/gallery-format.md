# The gallery's item format

Every designed thing the Graphics gallery lists is a folder with a
`graphic.toml` in it. An agent's `gallery.save`, the Import button, a drop
onto the gallery, an exported zip and a starter design shipped with the mixer
all write or read this one shape, so a design made by any of them reaches the
others unchanged.

This page is the contract. The methods that act on it are in
[the gallery methods](gallery.md).

## Where the folders are

```
<gallery dir>/
  .gmx-gallery              marker: the media library does not list this folder
  storm-lower-third/        one folder per item; the folder name is the id
    graphic.toml
    graphic.svg
  studio-blue/
    graphic.toml
    plate.png
    desk.png
  .previews/                pictures drawn for the gallery, safe to delete
  .shipped/                 starter designs written out when first drawn
  exports/                  zips written by gallery.export
```

The gallery directory is `[gallery] dir` in the config. Left out, it is a
folder called `graphics` inside the media library (`[media] dir`), so it
moves with the library and the desktop app keeps it in the same place as
every other file it holds. Paths in `graphic.toml` are relative to the item's
folder and use `/`, so a folder copied between Windows, macOS and Linux reads
the same on each.

An id is a slug: lower case letters, digits and dashes, at most 64
characters, made from the name when nobody gives one (`Storm warning, lower
third!` becomes `storm-warning-lower-third`). A folder whose name is not a
slug is not listed.

## graphic.toml

```toml
name = "Storm warning lower third"
kind = "template"
file = "graphic.svg"
zone = "lower-third"
description = "Red strap with a warning label, for weather breaking news"
tags = ["news", "weather", "red"]
origin = "agent"
made_by = "claude-code"
saved = "2026-10-05T14:02:11Z"

[values]
label = "STORM WARNING"
headline = "Gales on the coast from six tonight"
```

| Key | Required | What it is |
|---|---|---|
| `name` | yes | What people call it. |
| `kind` | yes | One of the kinds below. |
| `file` | for every kind but `ticker`, `text` and `set` | The main file, relative to the folder. |
| `zone` | no | Where it goes when placed: `full`, `lower-third`, `bug`, `top`, `bottom`, `center`, `overlay`. Worked out from the kind and the picture's shape when left out. |
| `description` | no | One or two sentences. Searched by `gallery.list`. |
| `tags` | no | Words to find it by. |
| `moves` | no | `true` when it moves by itself. Worked out from the kind when left out. |
| `transparent` | no | `true` when the picture under it shows through. Worked out from the file when left out. |
| `origin` | no | `shipped`, `agent` or `uploaded`. Default `agent`. A folder under `graphics/starters/` is `shipped` whatever it says. |
| `made_by` | no | Who saved it, in a word. |
| `saved` | no | When, RFC 3339. |
| `[values]` | no | Field values for a template or an OGraf graphic, over the field defaults. |
| `[source]` | for `ticker` and `text` | `uri` and `params`, exactly as `source.add` takes them. |
| `[set]` | for `set` | See below. |

Unknown keys are kept and ignored, so a newer mixer's item still lists on an
older one.

## The kinds

| Kind | Main file | Drawn by | Placed as |
|---|---|---|---|
| `template` | an SVG with `{{fields}}` | the mixer, `template:` | a source over the canvas, with the item's values as `params.fields` |
| `image` | PNG, WebP, JPEG, or an SVG with no fields | the mixer, `image/source` | a source in the zone |
| `clip` | WebM (VP8 or VP9, alpha or not), MOV (ProRes 4444, QuickTime Animation, PNG), MP4 | the mixer's clip source, looped | a source in the zone |
| `html` | `index.html`, with anything it loads beside it | the browser source | a web source loading the item's folder from the mixer |
| `ograf` | `graphic.ograf.json` beside its web component | the OGraf plugin | a web source, as the OGraf plugin serves it |
| `ticker` | none: `[source]` with a `ticker:` uri | the mixer, `ticker/source` | a source in the bottom strip |
| `text` | none: `[source]` with a `text:` uri | the mixer, `text/source` | a source in the zone |
| `set` | none: `[set]` | the layout `virtual-set` | a new scene, through `scene.create_from` |
| `transition` | a clip, a still or a shader | the transition methods | not placed; picked for a take |
| `effect` | a clip or a still | the effect methods | not placed; fired on demand |

`transition` and `effect` are listed, previewed, imported, exported and
deleted like every other kind. Placing one is refused with the name of the
method that plays it. The `[transition]` and `[effect]` tables in their
`graphic.toml` belong to those methods and the gallery passes them through
untouched.

### A set

A virtual set is a scene, not a separate feature. The gallery keeps the
pictures and the settings, and one click hands them to `scene.create_from`
with the layout `virtual-set` and the chosen camera.

```toml
name = "Blue news studio"
kind = "set"

[set]
background = "plate.png"       # behind the presenter
foreground = "desk.png"        # in front of them, transparent elsewhere; optional
layout = "virtual-set"         # the default
[set.settings]
presenter_scale = 0.8          # 0.3 to 1
presenter_x = 0.62             # 0 left to 1 right
screen = "green"               # green, blue, or none for a cut out with no screen
```

`background` and `foreground` are files in the folder. A set's lower third
area is the layout's own; place a `lower-third` item on the new scene for the
words.

## Pictures the gallery shows

The gallery draws a still for each item on demand, the first time a person
opens the gallery or an agent asks `gallery.preview`, and keeps it in
`.previews/` until the item changes. Nothing is drawn while nobody looks.

An item may carry its own:

| File | Used for |
|---|---|
| `preview.jpg`, `preview.png` or `preview.webp` | the still, for a kind the mixer cannot draw by itself (`html`, `ograf`, `transition`, `effect`) |
| `preview.webm` or `preview.mp4` | a short loop the gallery plays while a pointer rests on the card or a finger holds it |

A `clip` needs neither: the gallery takes a frame from the middle of it, and
plays the clip itself as the moving preview.

## Starter designs

Starters ship inside the mixer from `graphics/starters/<id>/` in the
repository: the same folder and the same `graphic.toml`, compiled in through
the table in `crates/godwinmix-core/src/gallery/starters.rs` (one line per
file, as the template pack is). They list as `origin = "shipped"`, are read
only, and are written out under `.shipped/` the first time one is drawn or
placed. Duplicate one to change it.

Keep a starter small: SVG, HTML and pictures, not long clips. Every byte of
one is in every copy of the binary.

## The zip

`gallery.export` writes a zip holding one folder per item, exactly as above,
plus `gallery.json` at the top:

```json
{"format": "godwinmix-gallery", "version": 1, "items": ["storm-lower-third", "studio-blue"]}
```

The members are stored, not compressed: pictures and clips are compressed
already. `gallery.import` reads this zip, and also a zip or a folder holding
one item, a lone SVG, HTML page, picture or clip, and an OGraf package.
