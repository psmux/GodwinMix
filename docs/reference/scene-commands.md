# Scene commands

Every `scene.*` method, with one example each. These are JSON-RPC calls on
`/rpc`, REST routes under `/api/v1`, and MCP tools behind `search_tools`; the
same table generates all three, so nothing here exists on one surface and not
another.

Two rules hold throughout.

**Names, not ids.** Every command that names a scene or an item takes the name
as readily as the id, in the same field. A miss answers with the names that
would have worked:

```
there is no scene called "sundy wide". This collection has: Sunday wide, evening
```

**Every mutating call answers with the result.** The scene's records plus the
derived geometry: where each item actually lands in canvas pixels, after groups
are flattened and references resolved. No follow up read is needed, and a client
draws its handles without recomputing anything.

```json
{
  "id": "0192f3a5-…", "name": "wide and guest",
  "canvas": { "width": 1920, "height": 1080, "fps": 30 },
  "records": [ … ],
  "geometry": [
    { "item": "0192f3a6-…", "path": "cam1", "source": "cam1",
      "x": 0, "y": 0, "width": 960, "height": 1080, "opacity": 1,
      "sourceWidth": 1920, "sourceHeight": 1080 }
  ],
  "findings": []
}
```

## Scenes

| Method | What it does |
|---|---|
| `scene.list` | every scene, with its item count, its sources and whether it is armed |
| `scene.get {scene}` | one scene in full |
| `scene.add {name, color?}` | an empty scene |
| `scene.create_from {sources, layout?, name?}` | a scene from a set of sources |
| `scene.remove {scene}` | delete one (destructive) |
| `scene.rename {scene, name?, color?}` | rename, recolour, or both |
| `scene.duplicate {scene, name?}` | a copy with new ids throughout |
| `scene.validate {scene?}` | what to fix before saying it is done |
| `scene.export {format, path?}` | the whole collection: `json`, or `zip` or `dir` with its assets |
| `scene.import {path}` | read a collection bundle and add its scenes, with a relink report |
| `scene.import.obs {path}` | read an OBS collection and add its scenes |

```json
{"method": "scene.create_from",
 "params": {"sources": ["cam1", "cam2", "slides"], "name": "three up"}}
```

With no `layout`, the count picks one: one source fills the canvas, two make a
two box, three a three box, four a quad, more a grid. The items are named after
their sources.

```json
{"method": "scene.rename", "params": {"scene": "three up", "color": "#2f6f4f"}}
```

Names and colours live on the document, so every client, the tally, a Stream
Deck and an agent see the same ones.

```json
{"method": "scene.validate", "params": {"scene": "three up"}}
```

```json
{"ok": false,
 "findings": [
   {"severity": "warning", "code": "scene.title_safe", "item": "0192…",
    "message": "the item 'ticker' is outside the title safe area"}]}
```

Leave `scene` out to check the whole collection.

## Items

| Method | What it does |
|---|---|
| `scene.item.add {scene, content, name?, transform?, draft?}` | put something on the canvas |
| `scene.item.remove {scene, item}` | take it off (destructive) |
| `scene.item.set {scene, item, props, duration_ms?, easing?, seq?}` | assign properties |
| `scene.item.move {scene, item, to_scene}` | move it to another scene |
| `scene.item.copy {scene, item, to_scene}` | copy it, with a new id |
| `scene.item.reorder {scene, item, before?, after?}` | up or down the stack |
| `scene.item.bind {scene, item, prop, param}` | bind geometry to a parameter |

```json
{"method": "scene.item.add",
 "params": {"scene": "three up", "content": {"source": "cam3"}, "name": "corner"}}
```

With no `transform` the item lands in the next free cell of a grid over what is
already there, so a drop on a scene tile never needs a dialog.

`content` is `{"source": "<id>"}`, `{"ref": "<scene id>"}` for a nested scene,
or `{"graphic": "<plugin/id>"}`.

```json
{"method": "scene.item.set",
 "params": {"scene": "three up", "item": "corner",
            "props": {"transform": {"position": {"x": 1400, "y": 40},
                                    "frame": {"w": 480, "h": 270},
                                    "fit": "cover"},
                      "opacity": 0.9, "audio": "never"},
            "duration_ms": 300}}
```

`props` is partial: a key you leave out stays as it is, and calling it twice
with the same body changes nothing the second time. A nested object is merged,
so setting `transform.position` does not wipe `transform.frame`.

`audio` is `follow` (heard while visible), `always` or `never`. A source is
heard when any live item of it says so; a source drawn twice is not louder.

`seq` is a client's own sequence number, echoed on the patch, so a drag can
discard the echoes of moves it has already drawn past.

## Arranging

Each of these is one call that cannot produce an off by twelve pixels result.

| Method | Params beyond `{scene, items}` |
|---|---|
| `scene.item.align` | `edge`: left, right, top, bottom, center-x, center-y |
| `scene.item.distribute` | `axis`: horizontal or vertical |
| `scene.item.fit_to_canvas` | |
| `scene.item.cover_canvas` | |
| `scene.item.arrange_grid` | `cols` |
| `scene.item.match_size` | `to`: the item to match |
| `scene.item.group` | `name?` |
| `scene.item.ungroup` | takes `{scene, item}` |

```json
{"method": "scene.item.align",
 "params": {"scene": "three up", "items": ["cam1", "cam2"], "edge": "left"}}
```

`distribute` leaves even gaps, not even positions: items of different sizes look
wrong when their corners are evenly spaced and right when the space between them
is. It needs at least three items, because the two on the ends stay where they
are.

Grouping changes nothing about the picture: the group's transform is the
identity and the children keep their own. Moving the group by 100 px moves every
child by 100 px and touches no child's transform. Ungrouping multiplies the
group's transform into each child, which is the same arithmetic the compositor
does, so it is invisible too.

## Per item filters

A filter on an item, not on a source, so a camera keyed in one scene is not
keyed in all of them.

| Method | Params |
|---|---|
| `scene.item.filter.add` | `{scene, item, type, name?, params}` |
| `scene.item.filter.set` | `{scene, item, filter, params?, enabled?}` |
| `scene.item.filter.remove` | `{scene, item, filter}` (destructive) |

```json
{"method": "scene.item.filter.add",
 "params": {"scene": "three up", "item": "corner",
            "type": "chroma/filter", "params": {"method": "green"}}}
```

`filter` names one by its name, its type, or its position in the chain from 0.

A filter reaches the pipeline as soon as it is written: on a scene that is on
air the change is pushed straight away, so the picture keys when the call
answers rather than at the next take. It sits on that item's own slot chain,
between the flip and the compositor pad, which is what lets the same camera be
keyed in one item and clean in another. The insert is under a pad block on that
slot's own queue, so the programme loses at most one frame of that item and
nothing downstream notices. A chain that has not actually changed is not
rebuilt, so the supervisor reapplying the scene twice a second costs nothing.

A filter on a **group** is the expensive path: the group cannot be flattened,
because the filter is over it as one picture, so it gets a compositor of its
own, built on demand and taken apart when the filter goes. See
[how a scene reaches the compositor](../explanation/how-a-scene-reaches-the-compositor.md).

## Layouts and parameters

| Method | What it does |
|---|---|
| `scene.layout.list` | the built in layouts and what each takes |
| `scene.apply_layout {layout, values, scene?, name?, duration_ms?, easing?}` | apply one |
| `scene.layout.copy {scene}` | read one scene's geometry |
| `scene.layout.paste {scene, layout, match?}` | put it on another's items |
| `scene.params.get` | the collection's typed parameters |
| `scene.params.set {values}` | set their values, declaring any that are new |

```json
{"method": "scene.apply_layout",
 "params": {"layout": "pip-bottom-right",
            "values": {"a": "cam-wide", "b": "guest"},
            "scene": "three up", "duration_ms": 300}}
```

A `{{name}}` in any string property of any item follows the collection's
parameters, a graphic's fields among them, so one `scene.params.set` changes
every strap that uses it. A parameter that is not declared yet is declared by
the first value put in it, typed from that value.

## Graphics

An OGraf template placed like any other item. See
[graphics](graphics.md) for the format and
[make a graphic](../how-to/make-a-graphic.md) for writing one.

| Method | What it does |
|---|---|
| `scene.graphic.list` | every graphic this core can place, with what each takes |
| `scene.item.schema {type}` | the fields one item type takes, as JSON Schema |
| `scene.apply_graphic {graphic, values, item?, play?, stop?, frame?}` | fill it by field name, and play it on or take it off |

```json
{"method": "scene.item.schema", "params": {"type": "ograf/lower-third"}}
{"method": "scene.apply_graphic",
 "params": {"graphic": "ograf/lower-third",
            "values": {"name": "Ada Lovelace", "title": "Analyst"},
            "play": true}}
```

Fields are addressed by name and never by position. A name the schema has not
got is refused with the names that would have worked, because the caller is
often a model and a silent no op teaches it nothing. With two of the same
graphic on the canvas, `item` says which by the name you gave it; with none, an
error says how to put one there.

`scene.item.schema` answers for every item type, not only a graphic: give it a
source or filter provide and it answers that plugin's settings schema, so a
client has one call for the inspector whatever the item is.

`frame: true` answers with a still of the armed scene beside the records, so an
agent checks its own work without a second call.

## Sharing a collection

| Method | What it does |
|---|---|
| `scene.export {format: "json"}` | the scene document alone |
| `scene.export {format: "zip", path?}` | a bundle carrying the assets, hashed; base64 with no path |
| `scene.export {format: "dir", path}` | the same bundle, unpacked |
| `scene.import {path}` | read one back, with a relink report |

```json
{"method": "scene.export", "params": {"format": "zip", "path": "/tmp/show.zip"}}
{"method": "scene.import", "params": {"path": "/tmp/show.zip"}}
```

An import answers with the scenes it added by the names they ended up with, the
plugins the bundle needs that this core has not got, and a relink entry for
every asset that did not come across naming the file and the items that draw
it. The scenes come in either way: a partial import succeeds visibly. See
[share a collection](../how-to/share-a-collection.md).

Applying onto a scene that already exists keeps its item ids, so the item that
was the inset is the item that becomes full screen and the change is a property
ramp rather than a cut between two sets of items. Without `scene`, a new one is
made.

`duration_ms` only means anything on a scene that is on air: there is nothing
to ramp on pads nobody is drawing, so off air it is a cut whatever the number
says. `easing` is `linear` or `ease` and defaults to `ease`, which is smooth at
both ends.

```json
{"method": "scene.layout.paste",
 "params": {"scene": "evening", "layout": {…}, "match": "name"}}
```

`match` is `name` (item names first, slot order second) or `order`. Items that
match nothing are left exactly as they were. This is "copy the design from one
scene to another".

## Sources on the tray

| Method | What it does |
|---|---|
| `source.set {source, name?, color?}` | name and colour a source |
| `source.group {sources, name?}` | put them in a tray folder |

A tray folder is a tag on the source for finding things, not a group on the
canvas. `name: null` takes them out of the one they are in.

## Preview and take

| Method | What it does |
|---|---|
| `scene.preview.set {scene?}` | arm a scene, or clear it with no scene |
| `scene.preview.frame {width?}` | a still, base64 JPEG |
| `program.take {scene?, source?, transition?, at_running_time_ms?}` | put it on air |

```json
{"method": "program.take", "params": {"scene": "three up"}}
```

`source` is shorthand for a one item full canvas scene and is taken exactly as
it always was. With neither `scene` nor `source`, the armed scene goes on air.

`transition` is a name or an object:

```json
{"method": "program.take",
 "params": {"scene": "three up",
            "transition": {"type": "fade", "duration_ms": 300}}}
```

`cut`, `fade`, `move` and `stinger` are built in, a collection's own named
transitions resolve first, and a `transition` plugin adds its own name. A name
nobody knows is `-32602` with `data.transitions` listing every one this core
would have taken. A bare name takes 300 ms; `duration_ms: 0` is a cut whatever
the type says. The full contract is in
[transitions](transitions.md).

`scene.preview.frame` composites the armed scene and hands back one frame of
it, base64 JPEG under `image` (the field is `image` because it carries a media
type beside it, not a format name). Asking for it is what builds the preview:
the compositor is up for the length of the call and gone after, unless
something else is watching. `layout` comes back beside the picture, so a client
drawing handles has the geometry without recomputing it.

`scene.preview.set` arms a scene and costs one message: nothing is composited
for a preview until a client asks for one with `ext.preview`, opens
`/mjpeg/preview`, or calls `scene.preview.frame`.

## Editing off air

| Method | What it does |
|---|---|
| `scene.edit.begin {scene, live?}` | a working copy, and its draft id |
| `scene.edit.apply {draft}` | write it back |
| `scene.edit.discard {draft}` | throw it away |

```json
{"method": "scene.edit.begin", "params": {"scene": "three up"}}
```

```json
{"draft": "0192f3c1-…", "scene": "three up", "live": false, "view": {…}}
```

Pass that id as `draft` on any `scene.item.*` call and the change goes to the
working copy. Nothing is published while a draft is being edited: a draft is
nobody else's business until it is applied. A draft of the scene that is on air
is applied on the next take of that scene, which is what `live: false` means.

## Batches and undo

| Method | What it does |
|---|---|
| `scene.transaction.begin` | start a batch |
| `scene.transaction.commit` | apply it: one patch, one undo step |
| `scene.transaction.abort` | throw it away |
| `scene.undo` | undo the last change |
| `scene.redo` | put it back |
| `scene.history.mark {label?}` | group what follows into one undo step |

Everything between `begin` and `commit` applies on one frame or not at all.
Nothing is published until the commit, so a client never draws half a batch.

`scene.history.mark` is what makes a drag one Ctrl+Z: a designer marks before
the first move and again with no label at the end, and the moves between fold
into one step.

```json
{"method": "scene.history.mark", "params": {"label": "drag lower third"}}
```

`scene.undo` answers with the patch it applied and how many steps are left:

```json
{"patch": {"seq": 91, "scope": "document", "updated": [{"before": {…}, "after": {…}}]},
 "undo": 3, "redo": 1}
```

## Patches

Clients mirror the document off `event/scene.patch` rather than refetching it.
Subscribe to `scene.*` and they arrive on `/rpc`:

```json
{"seq": 91, "source_client": "designer-1", "client_seq": 77, "scope": "document",
 "added": [], "updated": [{"before": {…}, "after": {…}}], "removed": []}
```

One per transaction, ended by `event/flush`. Suppress the echo of your own edits
by `source_client`, ignore record kinds and trailing fields you do not know, and
you can be several versions behind without breaking.

`client_seq` is the `seq` you put on the command coming back. A drag cannot
wait for a round trip, so the kit draws the move itself and reconciles when the
echo lands; without the number it cannot tell an echo of a move it has already
drawn past from a correction, and the handle rubber bands backwards under the
cursor. `scene.item.set`, the geometry operations and `scene.item.reorder` all
carry it.

A patch names one record per thing that changed. Inserting an item at the front
of a scene changes that item's order key and nobody else's, because the key is
stored on the document rather than worked out from the array: `order::between`
splits the gap between its neighbours, and only a gap that has run out makes a
group renumber.

## Errors

Every refusal names the state and the next step (03 section 6).

| Code | When |
|---|---|
| -32004 | no scene, item, source or draft by that name; the message lists the ones that exist |
| -32602 | the params were wrong for the method, or a transition this core does not have (`data.transitions` lists the ones it does) |
| -32001 | not in a state that allows it: no transaction open, nothing to undo, no scene armed |
| -32003 | a safety rule refused the take; `data.retry_after_ms` says how long |

```json
{"code": -32004,
 "message": "the scene \"three up\" has no item called \"lower-thrid\". It has: cam1, cam2, corner"}
```
