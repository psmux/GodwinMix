# Share a collection

Your scenes, your graphics and your pictures, in one file somebody else can
open. Five minutes, and it works in both directions.

A collection is everything the mixer knows about a show: the scenes, the items
on them, the layouts, the transitions it has named, the parameters and the
files it draws. `scene.export` writes all of it. `scene.import` reads it back.

## Write it out

```sh
gmx ctl scene export ~/sunday-service.zip
```

```
wrote /Users/you/sunday-service.zip (3 asset(s), 2 plugin(s) needed)
```

A path ending `.zip` writes a zip. Anything else writes a folder, which is
what to use when the collection lives in git and you want a readable diff:

```sh
gmx ctl scene export ~/shows/sunday
```

```
sunday/
  collection.json      the scene document, exactly as the mixer saves it
  bundle.json          what an importer needs to know before it reads it
  assets/
    media/logo.png
    media/lower-third-bg.png
```

## Open it somewhere else

```sh
gmx ctl scene import ~/sunday-service.zip
```

```
added 4 scene(s), 17 item(s): Wide, Pulpit, Lyrics, Full frame
```

The scenes are added to what is already there, never replacing it. A name
that is taken gets a free one, and the report names what the scenes ended up
called, not what they came in as.

The canvas the mixer is running at wins. A collection authored at 1080p opened
on a core running 720p is the ordinary case; the layouts resolve against the
running canvas.

## What the bundle says about itself

```json
{
  "bundle_version": 1,
  "id": "0192f3a4-1b2c-7d3e-8f40-51a2b3c4d5e6",
  "name": "Sunday service",
  "canvas": { "width": 1920, "height": 1080, "fps": 30 },
  "written_by": "godwinmix 0.2.0",
  "requires": [
    { "plugin": "ograf", "versions": "^0.2.0", "provides": ["ograf/lower-third"] },
    { "plugin": "chroma", "versions": "*", "provides": ["chroma/filter"] }
  ],
  "assets": [
    { "id": "0192f3a4-...", "path": "assets/media/logo.png", "sha256": "9f2c...", "size": 18422 }
  ],
  "skipped": []
}
```

`requires` is every plugin the scenes name, with the version range that will
do. An import on a mixer that has none of them still gets the geometry; the
report says which are missing and those items draw nothing until they are
installed:

```
added 4 scene(s), 17 item(s): Wide, Pulpit, Lyrics, Full frame
  needs a plugin that is not installed: ograf ^0.2.0
```

## When a picture is missing

Every asset is carried at a path relative to the collection, never an absolute
one, and with a sha256 beside it. That is what makes a show open on somebody
else's machine at all: OBS stores absolute paths, which is why every
commercial scene bundle ships a relink wizard and why a show copied between
two laptops opens with black rectangles.

A file that did not come across, or one whose bytes do not match, is reported
rather than guessed at, and the scenes come in anyway:

```
added 4 scene(s), 17 item(s): Wide, Pulpit, Lyrics, Full frame
  relink media/logo.png: the bundle does not carry this file (used by Wide: church logo, Lyrics: corner logo)
```

The report names the file and every item that draws it, so you know what will
be blank before you go on air. Put the file at that path beside the collection
and it is picked up on the next load.

An absolute path is refused when you export, not when somebody else imports.
The person who can fix it is the one exporting:

```
wrote /Users/you/sunday-service.zip (0 asset(s), 2 plugin(s) needed)
  skipped media/logo.png: the document gives this asset an absolute or climbing path...
```

Move the file beside the collection and export again.

## From OBS

A collection somebody has in OBS comes across in one command.

```sh
gmx ctl scene import-obs ~/Downloads/Untitled.json
```

Export it from OBS first with Scene Collection, Export. Groups become groups,
a nested scene becomes a reference, pixel crops become fractions of the source
size so they survive a canvas change, and `blend_type` is carried. Source
filters are the one thing whose meaning changes: OBS attaches a filter to a
source, so a camera keyed in one scene is keyed in all of them, while here a
filter belongs to the item. Each source filter is copied onto every placement
and the report names each copy, because a change of meaning that nobody is
told about is a change of meaning that bites on air.

See [import from OBS](import-from-obs.md) for the whole table.

## Over the protocol

```json
scene.export { "format": "zip", "path": "/tmp/show.zip" }
scene.export { "format": "zip" }
scene.export { "format": "dir", "path": "/tmp/show" }
scene.export { "format": "json" }
scene.import { "path": "/tmp/show.zip" }
```

With no `path`, a zip comes back as base64 in the answer, which is what a
client on the other side of a WebSocket wants: it has no filesystem in common
with the mixer. Past eight megabytes it refuses and tells you to give a path,
because eleven megabytes of base64 in a log helps nobody.

`format: "json"` is the scene document alone, with no assets. It is what to
read when you want to look at the document, not what to send somebody.

## Publishing one

A collection is a plugin kind on the index, so a church that has built a good
set of scenes can list it beside the plugins. See
[publish a plugin](publish-a-plugin.md) and
[the index format](../reference/index-format.md).

## See also

* [Scene commands](../reference/scene-commands.md) for `scene.export` and
  `scene.import` in the table with everything else.
* [The scene document](../reference/scene-document.md) for what is inside
  `collection.json`.
* [Make a graphic](make-a-graphic.md), since a collection usually carries one.
