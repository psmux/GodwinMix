# The gallery methods

`gallery.*` is the Graphics gallery: every designed graphic the mixer can put
on air, saved by an agent, imported by a person or shipped with the mixer.
The page's **View > Graphics**, `gmx tool`, the MCP server and any client
call the same methods. What an item is on disk is
[the gallery's item format](gallery-format.md); how to use it is
[Build your own graphics gallery with an AI agent](../how-to/build-a-graphics-gallery-with-ai.md).

| Method | MCP tool | Scope | REST | What it does |
|---|---|---|---|---|
| `gallery.list` | `list_graphics` | read | `GET /api/v1/gallery/list` | every item, or those some words find |
| `gallery.save` | `save_graphic` | operate | `POST /api/v1/gallery/save` | save one graphic of any kind |
| `gallery.preview` | `preview_graphic` | read | `POST /api/v1/gallery/preview` | a picture of an item, drawn on demand |
| `gallery.place` | `place_graphic` | operate | `POST /api/v1/gallery/place` | add an item to a scene in its zone, hidden |
| `gallery.show` | `show_graphic` | operate | `POST /api/v1/gallery/show` | show a placed item on air, or hide it |
| `gallery.edit` | `edit_graphic` | operate | `POST /api/v1/gallery/edit` | change name, tags, description, zone or values |
| `gallery.duplicate` | `duplicate_graphic` | operate | `POST /api/v1/gallery/duplicate` | copy any item, shipped ones too |
| `gallery.remove` | `remove_graphic` | operate, destructive | `POST /api/v1/gallery/remove` | delete a saved item and its files |
| `gallery.export` | `export_graphics` | operate | `POST /api/v1/gallery/export` | write items to one zip on the mixer |
| `gallery.import` | `import_graphics` | operate | `POST /api/v1/gallery/import` | take files in, each checked |

The tools are not in either MCP profile's hot list, which is full; they are
found by `search_tools` ("make a lower third", "graphics gallery") and called
by name, and the MCP server's instructions name them. Every one runs from a
shell as `godwinmix tool <tool> '<json>'`.

Four routes carry bytes rather than JSON:

| Route | What it is |
|---|---|
| `GET /api/v1/gallery/{id}/preview.jpg?width=&background=` | the preview as a JPEG, for an `<img>` |
| `GET /api/v1/gallery/{id}/files/{path}` | one of an item's own files: an HTML graphic's page for the browser source, a clip for a moving preview. Open to a process on the mixer's machine; a token with read from anywhere else |
| `GET /api/v1/gallery/exports/{file}` | a zip `gallery.export` wrote |
| `POST /api/v1/gallery/upload?name=<file name>` | the body a file, taken in as `gallery.import` takes it; answers `GalleryImported` |

## The item

What `gallery.list` answers with, one per item.

| Field | What it is |
|---|---|
| `id` | the slug every other method takes |
| `name`, `description`, `tags` | what people call it and find it by |
| `kind` | `template`, `image`, `clip`, `html`, `ograf`, `ticker`, `text`, `set`, `transition`, `effect` |
| `zone` | `full`, `lower-third`, `bug`, `top`, `bottom`, `center`, `overlay` |
| `moves`, `transparent` | whether it moves by itself, whether the picture under it shows through |
| `origin` | `shipped`, `agent` or `uploaded` |
| `fields`, `values` | a template's fields with their defaults, and the values this item fills them with |
| `uri` | the address `source.add` takes for it, when it is one source |
| `moving` | a file the page plays as its moving preview |
| `made_by`, `saved` | who saved it and when |
| `placed` | the sources on this mixer showing it now |

## gallery.list

`{query?, kind?, limit?}`. `query` is words, matched against the name, kind,
zone, tags and description, best first; "l3", "strap", "bg", "crawl", "logo"
and "studio" are read as what people mean by them. `kind` keeps one kind.
`limit` defaults to 50. The answer is `{items, dir, total, errors}`, where
`errors` lists folders that look like items and would not read.

## gallery.save

One graphic, any kind, in one call. `name` and exactly one of:

| Key | Takes |
|---|---|
| `svg` | a whole SVG. With `{{fields}}` or `<gmx:template>` it is a template and is checked the way `template.save` checks one; without, a picture, which needs a size |
| `html` | a whole page; `files` adds what it loads, by name, as text or `data:` URIs |
| `data` | a picture or clip as base64 or a `data:` URI; `filename` names it |
| `file` | a file or folder on the mixer's machine, or a media library name |
| `source` | `{uri, params}` with a `ticker:` or `text:` uri |
| `set` | `{background, foreground?, layout?, settings?}`; each picture is a gallery id, a media file, a path or a `data:` URI |

And optionally `tags` (a list, or one string with commas), `description`,
`zone`, `kind` (only to say a clip is a `transition` or an `effect`, or a page
is `ograf`), `moves`, `transparent`, `values`, `made_by` and `replace`.

The common ways of writing a key are read too: `title` for `name`, `path` for
`file`, `fields` for `values`, `position` for `zone`, `overwrite` for
`replace`, `svg_code` for `svg`, `base64` for `data`. A key it does not know is
refused with the list of keys it does.

The answer is `{item, path, redrawn, warnings, next}`. `next` says what to call
now. `warnings` holds what did not stop the save: a page or an SVG that loads
something from the network, for one. With `replace` true every source showing
the item is drawn again.

Refused, with nothing written: no graphic or more than one, an SVG that will
not read or says no size, bytes that are not a picture, clip, page or zip, an
id that ships with the mixer, an id that exists without `replace`, and a mixer
with `media.allow_upload` off. Each refusal has `data.fix` or names the field.

## gallery.preview

`{id, width?, background?, values?}`. `width` is 64 to 1920, default 960.
`background` is `checker` (the default), `black`, `white`, `grey` or
`#rrggbb`. `values` tries field words without saving them; a field the
template does not have is refused with the ones it has.

The answer is `{id, width, height, image, encoding, format, from, caption}`.
The picture is the item where it would land on a 16:9 canvas. `from` says how
it was made: `drawn` by the code that draws it on air, `frame` from the middle
of a clip, `poster` from the item's own `preview.png`, or `card`, a
placeholder naming the kind, for an HTML or OGraf graphic with no poster
(those are drawn by a browser). Over MCP it arrives as an image with the
caption beside it.

Pictures are drawn on a blocking thread and kept in `.previews/` until the
item changes. Cards for the page are drawn two at a time and a preview asked
for by this method has a lane of its own. Nothing is drawn while nobody asks.

## gallery.place

`{id, scene?, zone?, values?, visible?, camera?}`. The item becomes one
source, named by its id (a second placement shows the same source), and an
item on the scene, in its zone, with an enter and an exit that suit it.
`visible` defaults to false. Placing an item that is on the scene already
changes its `values` and nothing else.

The scene is the one asked for, else the one on air, else the armed one, else
a scene that shows just the source on air, else one made from that source
(`<source> with graphics`). A full zone item goes under everything else on the
scene.

A `set` is placed as a new scene through `scene.create_from` with the layout
`virtual-set`: the background, the `camera` (default: the source on air) and
the foreground, with the set's settings. A `transition` or `effect` is refused:
it is played by a take.

The answer is `{id, scene, source?, item?, visible, updated, new_scene, next}`.

## gallery.show

`{id, scene?, visible?}`. `id` is the gallery id, the source id or the item's
name. Shows the item (default `visible: true`) or hides it, and takes its
scene to the programme when the scene was not on air. For a set it takes the
set's scene. The answer is `{scene, item, visible, took}`.

## gallery.edit, gallery.duplicate, gallery.remove

`gallery.edit {id, name?, description?, tags?, zone?, values?}` changes a saved
item's `graphic.toml`; a value of `null` drops one. A shipped item is refused
with the next step, `gallery.duplicate`.

`gallery.duplicate {id, name?}` copies any item under a new id, the pack and
the starters included. The copy is yours to change.

`gallery.remove {id}` deletes a saved item's folder, or a library template's
file. It is refused while a source shows the item, with the sources in
`data.sources`, and for a shipped item. `dry_run: true` says what it would
delete.

## gallery.export and gallery.import

`gallery.export {ids?, path?}` writes a stored zip, as
[the item format](gallery-format.md#the-zip) describes, to `path` or to the
gallery's `exports` folder, and answers `{path, size_bytes, ids, url}`.
Without `ids` it exports every item that did not ship with the mixer.

`gallery.import {path? | data + filename, replace?}` reads a file or folder on
the mixer, or bytes, and answers `{added, refused}`. A gallery zip brings
every item in it; a folder of loose files brings each file; a folder with a
`graphic.toml`, an `index.html` or a `.ograf.json` is one item. Each refused
file has its `reason` and its `fix`. An id that is taken gets a new one unless
`replace` is true.

## Configuration

| Key | Default | What it does |
|---|---|---|
| `[graphics] gallery` | `graphics` inside `[media] dir` | the gallery's folder |
| `[media] allow_upload` | `true` | off, nothing can be saved or imported |
| `[media] max_upload_bytes` | as the media library | the largest upload, and the largest `gallery.save` body |
