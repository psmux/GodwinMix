# The scene document

A scene collection is one JSON file: the canvas, the parameters, the scenes,
the transitions and the assets. This page is the field by field reference.

The machine readable version is `schemas/scene.schema.json`, JSON Schema draft
2020-12, generated from the Rust types and committed, so a client in any
language validates against the same thing the core reads. A test fails the
build when the two drift. `gmx scene schema` prints the current one.

## Two shapes of the same document

On disk it is a nested tree: scenes hold items, items hold children. It is
pretty printed with one property per line and the keys in a fixed order, so a
diff in git reads like the change it describes.

In the core and on the wire it is a flat record store: one record per scene and
per item, each with an id, a parent and a fractional order. Moving an item
writes one record. Inserting between two siblings renumbers nothing. Two
clients editing two items never touch the same path, which is what lets
`event/scene.patch` be a patch rather than a whole document.

They are projections of each other and the conversion is lossless in both
directions.

```
gmx scene import-tree scenes.json    # the tree, as records
gmx scene export-tree records.json   # the records, as a tree
```

## A complete example

```json
{
  "schemaVersion": 1,
  "id": "0192f3a4-1b2c-7d3e-8f40-51a2b3c4d5e6",
  "name": "Sunday service",
  "canvas": { "width": 1920, "height": 1080, "fps": 30 },
  "params": {
    "$schema": "https://json-schema.org/draft/2020-12/schema",
    "type": "object",
    "required": ["speaker"],
    "properties": {
      "speaker": { "type": "string", "default": "", "title": "Speaker" }
    }
  },
  "scenes": [
    {
      "id": "0192f3a5-0000-7000-8000-000000000001",
      "name": "Wide with lower third",
      "items": [
        {
          "id": "0192f3a6-0000-7000-8000-000000000002",
          "name": "stage",
          "content": { "source": "cam-wide" },
          "transform": {
            "position": { "x": 0, "y": 0 },
            "rotation": 0,
            "scale": { "x": 1, "y": 1 },
            "anchor": { "x": 0, "y": 0 },
            "frame": { "w": 1920, "h": 1080 },
            "fit": "cover",
            "align": "center"
          },
          "crop": { "left": 0, "top": 0, "right": 0, "bottom": 0 },
          "opacity": 1,
          "blend": "normal",
          "visible": true,
          "locked": false,
          "audio": "follow"
        },
        {
          "id": "0192f3a8-0000-7000-8000-000000000003",
          "name": "corner",
          "content": {
            "children": [
              {
                "id": "0192f3a9-0000-7000-8000-000000000004",
                "name": "pulpit",
                "content": { "source": "cam-pulpit" },
                "transform": {
                  "position": { "x": 1400, "y": 40 },
                  "frame": { "w": 480, "h": 270 },
                  "fit": "cover"
                },
                "filters": [
                  { "type": "chroma/filter", "params": { "key": "#00ff00" } }
                ]
              }
            ]
          },
          "opacity": 0.9
        },
        {
          "id": "0192f3a7-0000-7000-8000-000000000005",
          "name": "lower third",
          "content": {
            "ref": "0192f3b0-0000-7000-8000-000000000006",
            "overrides": {
              "0192f3b1-0000-7000-8000-000000000007": {
                "params": { "name": "{{speaker}}" }
              }
            }
          },
          "transform": {
            "position": { "x": 96, "y": 880 },
            "frame": { "w": 900, "h": 140 },
            "fit": "contain"
          },
          "audio": "never"
        }
      ]
    }
  ],
  "transitions": [
    {
      "id": "0192f3c0-0000-7000-8000-000000000008",
      "name": "fade",
      "type": "fade",
      "duration_ms": 300
    }
  ],
  "assets": {
    "0192f3d0-0000-7000-8000-000000000009": {
      "path": "assets/logo.png",
      "sha256": "..."
    }
  }
}
```

Every field but `id`, `name`, `content` and the top level block has a default,
so an item written by hand can be three lines and still mean something exact.

## The top level

| Key | Type | What it is |
|---|---|---|
| `schemaVersion` | integer | the document format. 1 today. An older document is migrated on read; a newer one is refused with a message saying to update GodwinMix |
| `id` | id | this collection's own id |
| `name` | string | what a person calls it |
| `canvas` | object | `width`, `height`, `fps`. The output raster |
| `params` | JSON Schema | what is fillable in this document, in OGraf's shape. Readable on its own, so a client or an agent sees what it can set before setting it |
| `scenes` | array | the scenes, in order |
| `transitions` | array | named transitions, optional |
| `assets` | object | files this collection carries, by id, optional |

## Ids

Every node carries a UUID assigned once and never reused. The wire protocol and
every override address by id; `name` is free to change and nothing depends on
it.

An id minted here is version 7, which is time ordered, so records sort into
creation order and a burst of them stays in order. An id derived from a layout
is version 8: applying a layout twice has to land on the same items, and a
derived id has no clock in it. Both print in the usual hyphenated form.

Source ids are different and stay slugs: `cam-wide`, not a UUID. An operator
types those.

## A scene

| Key | Type | Default | What it is |
|---|---|---|---|
| `id` | id | | |
| `name` | string | | |
| `items` | array | | bottom of the stack first, the way the compositor takes them |
| `color` | string | absent | a colour every client, the tally and the Stream Deck agree on |

## An item

One placement of content. Not a "layer": that word is already the superimposed
page.

| Key | Type | Default | What it is |
|---|---|---|---|
| `id` | id | | |
| `name` | string | absent | free to change |
| `content` | object | | one of the four shapes below |
| `transform` | object | identity | where it sits and how big it is |
| `crop` | object | nothing trimmed | how much of the content's own pixels to cut |
| `opacity` | number | 1 | 0 to 1. A first class field, not a colour filter |
| `blend` | string | `normal` | `normal`, `add`, `screen`, `multiply`, `lighten`, `darken`, `subtract` |
| `visible` | boolean | true | |
| `locked` | boolean | false | the designer will not move it |
| `audio` | string | `follow` | `follow`, `always`, `never`. A source is audible when any live item of it says so |
| `filters` | array | empty | this placement's filter chain |
| `bind` | object | empty | layout bindings; see below. Empty in a resolved scene |

### `content`

Four shapes, told apart by their keys.

```json
{ "source": "cam-wide" }
```
A source id, a slug.

```json
{ "ref": "<scene id>", "overrides": { "<item id>": { "params": {} } } }
```
Another scene in this collection, with sparse overrides addressed by the
target's item id. A reference cycle is refused when the document is read, with
the path of scene names in the message.

```json
{ "graphic": "lowerthird/graphic", "params": { "name": "Ada" } }
```
A graphic template, by its plugin qualified id.

```json
{ "children": [ ... ] }
```
A group. No special type and no special rules: the group's transform multiplies
into each child and its opacity into each child's alpha, at apply time. That is
the whole difference from OBS, where a group is a modified scene, and it is
what removes a decade of transform corruption bugs.

### `transform`

| Key | Type | Default | What it is |
|---|---|---|---|
| `position` | `{x, y}` | 0, 0 | canvas pixels, of the anchor point |
| `rotation` | number | 0 | degrees, clockwise, about the anchor |
| `scale` | `{x, y}` | 1, 1 | |
| `anchor` | `{x, y}` | 0, 0 | normalised inside the item's own box: `0,0` top left, `0.5,0.5` centre, `1,1` bottom right |
| `frame` | `{w, h}` | absent | the rectangle the content is fitted into, in canvas pixels. Absent means the content's own size, scaled |
| `fit` | string | `none` | how the content fills the frame |
| `align` | string | `center` | where it sits inside the frame when it does not fill it |

### The `fit` vocabulary

SVG's words. They replace OBS's seven bounds types one for one, and they map
onto `sizing-policy` on a `glvideomixer` pad.

| `fit` | What it does | OBS |
|---|---|---|
| `none` | the content's own size, scaled by `scale`; the frame is ignored | `OBS_BOUNDS_NONE` |
| `stretch` | fills the frame exactly, distorting the aspect ratio | `OBS_BOUNDS_STRETCH` |
| `contain` | fits inside the frame, keeping the aspect ratio, leaving space on two sides | `OBS_BOUNDS_SCALE_INNER` |
| `cover` | fills the frame, keeping the aspect ratio, cropping the overflow | `OBS_BOUNDS_SCALE_OUTER` |
| `fit-width` | scales so the width matches; the height falls where it falls | `OBS_BOUNDS_SCALE_TO_WIDTH` |
| `fit-height` | scales so the height matches; the width falls where it falls | `OBS_BOUNDS_SCALE_TO_HEIGHT` |
| `max` | like `contain`, but never scales up past the content's own size | `OBS_BOUNDS_MAX_ONLY` |

### The `align` vocabulary

Nine keywords, for where the content sits inside its frame once `fit` has
decided how big it is. `top-left`, `top-center`, `top-right`, `center-left`,
`center`, `center-right`, `bottom-left`, `bottom-center`, `bottom-right`.

`align` places content inside the frame. `anchor` decides which point of the
item sits at `position`. They are different jobs and an item uses both.

### `crop`

```json
{ "left": 0.0833, "top": 0, "right": 0.0833, "bottom": 0 }
```

Fractions of the content's own size, 0 to 1, trimmed before `fit` runs. They
are fractions and not pixels so a crop survives a canvas change: the same
document at 1080p and at 720p crops the same part of the picture. OBS crops in
pixels, which is why an OBS collection moved between canvas sizes loses its
crops.

### `filters`

```json
[ { "type": "chroma/filter", "name": "Key", "enabled": true, "params": { "key": "#00ff00" } } ]
```

A filter belongs to the item, not to the source. A camera keyed in one scene is
not keyed in the others, which is the thing people spend a nested scene per
placement working around in OBS. An importer that brings OBS filters across
copies each one onto every placement and says so in its report.

### `bind`

Only in a layout preset. A geometry path against a small arithmetic expression
over the layout's params and `W` and `H`, the canvas size in pixels.

```json
"bind": {
  "position.x": "W - inset*W - gap*W",
  "position.y": "H - inset*H - gap*H",
  "frame.w": "inset*W",
  "frame.h": "inset*H"
}
```

The expression language is numbers, parameter names, `+ - * /`, brackets and a
leading minus. Nothing else: no functions, no comparisons, nothing that can
loop. The paths that can be bound are `position.x`, `position.y`, `frame.w`,
`frame.h`, `scale.x`, `scale.y`, `anchor.x`, `anchor.y`, `rotation`, `opacity`
and the four `crop.*`.

`layout::apply` evaluates them, resolves `{{param}}` in every string, and
leaves `bind` empty on the scene it produces. `gmx scene layout <name>
--values a=cam1,b=cam2` does it from the command line.

## Transitions and assets

```json
{ "id": "...", "name": "fade", "type": "fade", "duration_ms": 300 }
```

```json
{ "<id>": { "path": "assets/logo.png", "sha256": "...", "size": 20480 } }
```

An asset path is relative to the collection's own directory, always. OBS stores
absolute paths, which is why every commercial OBS scene bundle ships a relink
wizard.

## Validating

```
gmx scene validate scenes.json
gmx scene validate scenes.json --report json
```

Findings are data, not refusals. Each has a `severity`, a stable `code`, the
scene and items it is about, one sentence naming the state and the next step,
and a `detail` block with the numbers.

| Code | Severity | What it means |
|---|---|---|
| `scene.ref` | error | a reference cycle, or a reference to a scene that is not here |
| `scene.duplicate_id` | error | two nodes share an id |
| `scene.off_canvas` | warning | an item is entirely off the canvas |
| `scene.off_canvas_partly` | info | an item hangs over the edge |
| `scene.hidden` | warning | an item is completely covered by an opaque one above it |
| `scene.action_safe` | warning for a graphic, info otherwise | it reaches outside the middle 93 percent |
| `scene.title_safe` | info | it reaches outside the middle 90 percent |

Nothing here stops a document being saved or taken to air. An item parked off
the canvas is a legitimate thing to build, and a validator that refuses one is
a validator people turn off.

## Migrations

`schemaVersion` has a table of steps written before it was needed. Reading a
document runs every step from its version up to this build's, in order. A
document from a newer build is refused with a message that says to update
GodwinMix rather than half reading it.

Version 0 means a document written before the version was stamped. Its step to
version 1 changes nothing; it exists so the machinery has run at least once.

## The flat projection

```json
{
  "schemaVersion": 1,
  "id": "...",
  "name": "Sunday service",
  "canvas": { "width": 1920, "height": 1080, "fps": 30 },
  "records": [
    { "id": "<scene>", "order": "V", "kind": "scene", "name": "Wide" },
    {
      "id": "<item>",
      "parent": "<scene>",
      "order": "V",
      "kind": "item",
      "content": { "type": "source", "source": "cam-wide" },
      "transform": { "...": "..." },
      "opacity": 1
    }
  ]
}
```

A record has an `id`, a `parent` (absent for a scene), an `order`, and a `kind`
of `scene` or `item`. A group's children are records whose parent is the group,
and its own content is `{ "type": "group" }`.

`order` is a fractional key: a string that sorts between its neighbours, so an
insert writes one record and only that record. Siblings sort by `order`, then
by `id` to break a tie. A key never ends in the lowest digit, because `"1"` and
`"10"` would otherwise be the same number with nothing between them.

`gmx scene schema --flat` prints the schema for this shape.

## What the live core adds

A document read by a running core behaves in three ways the file alone does not
say.

**The running canvas wins.** A collection authored at 1080p and opened on a core
running 720p is the ordinary case, not an error: the `canvas` block is replaced
by the one the core is on, and layout presets resolve against that. Geometry
written as literal pixels is not rescaled, which is why `fit` and a `frame` are
worth using over bare positions.

**Not every item reaches the compositor.** An item whose content is a `graphic`
is skipped: the graphics host is a later release, and a placement with nothing
behind it would be a black rectangle over the picture. An item naming a source
this core does not have is skipped too, and `program.take` refuses the scene
before it goes on air rather than putting a composition with holes in it on the
programme. `scene.validate` reports both.

**Three of the seven fits share a policy.** The compositor pad has `none`,
`keep-aspect-ratio` and `scale`. `contain` and `max` become
`keep-aspect-ratio`; `stretch`, `cover`, `fit-width` and `fit-height` become
`scale`, with the item's own crop taking the overflow. The document keeps the
distinction, so a GPU compositor that has more policies, or a client drawing its
own preview, still has all seven.

Rotation is the other narrowing: `videoflip` turns by quarters, so a `rotation`
between the right angles is rounded to the nearest one on this path. The
document keeps the number.

## `sources`: names, colours and tray folders

```json
"sources": {
  "cam-wide": { "name": "Wide", "color": "#2f6f4f", "group": "cameras" }
}
```

Not a source list: the mixer owns those, and this block only says what this
collection calls them. It is where `source.set` and `source.group` write, and it
is why every client, the tally and an agent show the same label for a camera.
`group` is a tray folder, a tag for finding things, and has nothing to do with a
group on the canvas.

An entry for a source the core does not have is kept rather than pruned: a
collection moved between machines should not forget what the cameras were
called because one of them was not plugged in that day.

## Where it lives

Beside the runtime store, as `<stem>.scenes.json`, written through a temporary
file and a rename every time something changes. A file that will not parse is a
loud failure and never a silent new document: somebody's show is in it.
