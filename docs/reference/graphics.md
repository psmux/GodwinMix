# Graphics

A graphic is a template with words in it: a lower third, a strap, a score
bug, a title card. GodwinMix does not define a template format. It hosts
[OGraf](https://ograf.ebu.io/), the EBU's open graphics format, so a template
written for another OGraf host plays here and one written here plays there.

For a designed graphic that holds still between changes, the mixer also draws
SVG templates itself with no browser, which costs a fraction of a page per
graphic; see [graphic templates](graphic-templates.md).

A graphic reaches the programme as an ordinary source. The graphics host
serves each placement as its own web page, a browser source renders that page,
and the slot pool draws it like a camera. Nothing in the compositor knows what
a graphic is.

```
  item {graphic: "ograf/lower-third", params: {name: "{{speaker}}"}}
       |
       |  the schema's defaults, the item's params, then the
       |  collection's parameters resolved
       v
  source "graphic-lower-third-9f2c41ab"
       |
       |  browser/source on http://127.0.0.1:7841/graphic/ograf/lower-third
       v
  a slot on the programme compositor
```

## The OGraf subset this host reads

A graphic is a directory holding a `graphic.ograf.json` and the module it
names. These are the keys this build reads; everything else in the file is
carried through untouched, so a key a later OGraf release adds reaches a
client that understands it without the core being upgraded first.

| Key | Required | What it means here |
|---|---|---|
| `id` | yes | The graphic's own id. The provide id in `gmx-plugin.toml` is what a scene names. |
| `name` | yes | What a picker puts under the tile. |
| `main` | yes | The module, relative to the manifest. Served at `/graphic/<plugin>/<id>/<main>`. |
| `description` | no | One line under the name. |
| `version` | no | Recorded in a collection export. |
| `stepCount` | no, defaults to 1 | How many steps `playAction` walks through. One means in and out. |
| `supportsRealTime` | no, defaults to true | Whether it can go on air. |
| `supportsNonRealTime` | no, defaults to false | Not used by this host yet. |
| `schema` | no, defaults to empty | JSON Schema for the graphic's own data. What the inspector renders and what `scene.apply_graphic` fills by name. |

The module's default export is a custom element class with four methods on it,
each answering `{ status: 0 }`:

| Method | When it is called |
|---|---|
| `load({data})` | once, when the page comes up or the graphic is loaded into a placement |
| `updateAction({data})` | new words while it is on air, with no animation |
| `playAction({data, step})` | bring it on, or move to a step |
| `stopAction({data})` | take it off |

`dispose()` is called where a host has one. Nothing else in the specification
is required and nothing else is read.

## The actions

Four verbs, and they are OGraf's own with `Action` taken off.

| Verb | Through the scene | Through the tool |
|---|---|---|
| load | `scene.item.add {content: {graphic}}` does it | `{action: "load", instance, graphic, values}` |
| update | `scene.apply_graphic {graphic, values}` | `{action: "update", instance, values}` |
| play | `scene.apply_graphic {graphic, values, play: true}` | `{action: "play", instance, step?}` |
| stop | `scene.apply_graphic {graphic, stop: true}` | `{action: "stop", instance}` |

`scene.apply_graphic` is the one to reach for. It finds the placement, fills
the fields by name, pushes the values to the host and answers with the values
as the graphic will render them. The tool is the lower level, for a client
that already has the instance id and wants to step through a graphic with a
`stepCount` above one.

The instance is the source id the placement resolves to: the graphic's provide
id and eight hex characters from the item's id, for example
`graphic-lower-third-9f2c41ab`. It is legible in `source.list`, stable when the
item is renamed, and different for every placement, which is what lets the
same template be on the canvas twice with different words in it.

## Where the values come from

Three layers, lowest first.

1. The OGraf schema's own `default` for each property.
2. The item's `content.params`.
3. Any `{{name}}` in a string, resolved against the collection's parameters.

So `scene.params.set {values: {speaker: "Grace Hopper"}}` changes every strap
on every scene whose `name` field is written `{{speaker}}`, in one call. A
binding nobody has filled in is left showing as `{{speaker}}` rather than
blanked, so a half filled graphic says what is missing.

## Transparency, and what it costs

An OGraf graphic is drawn with its transparency: a soft edge, a rounded
corner, a drop shadow or a gap shows the picture under it. Its source is a
`browser/source` with `transparent: true`, which the scene adds by itself.
The page is rendered by the browser renderer in graphic mode: only the part
of the page with something in it crosses to the mixer, only when it changed,
already in AYUV, and the overlay board blends that part over the programme
after the compositor (see `overlay` in the core). A graphic that holds still
sends nothing and costs the blend of its own box.

The same works for any web page: add it with `params.transparent = true`
(see [web page sources](web-page-sources.md#transparent-pages)). And a
graphic written for this mixer rather than for OGraf can be an HTML template,
which is the same page with its fields and its way in and out driven by the
mixer itself; see [graphics for agents](graphics-for-agents.md).

What it cannot do, as for every transparent source: it is drawn over every
opaque item, whatever its place in the stack. Among transparent items the
stack order holds. On a GPU graphics entry the board does not draw, and the
graphic goes through the compositor flat.

### The key colour fallback

`chroma/filter` on an item's filter chain still keys a page served on a solid
colour, for a page that cannot be made transparent. It is not needed for a
page drawn with `transparent: true`.

## The host

`plugins/ograf` is the first party host. It serves on 127.0.0.1 and nothing
else, because the only client is a browser the mixer started on this machine
and a graphics host reachable from the network is a way to put words on
somebody's programme.

| Route | What it is |
|---|---|
| `GET /` | what it can serve and what it is driving, for a person |
| `GET /health` | `{ok, graphics, instances, base}` |
| `GET /graphic/<plugin>/<id>?instance=<i>` | the page a browser source loads |
| `GET /graphic/<plugin>/<id>/manifest.json` | the OGraf manifest |
| `GET /graphic/<plugin>/<id>/<file>` | the graphic's own files |
| `GET /state/<instance>` | what that placement is showing now |
| `GET /events/<instance>` | the actions, as `text/event-stream` |

One page per placement, never one page with four graphics on it: a template
that throws takes its own page down and not the other three.

The state is kept in the host and not in the page. A browser source is a
process the supervisor may restart, and if the words lived in the page a
restart would put a blank strap on air. The page fetches `/state/<instance>` on
connect, so a restart is invisible.

Its one setting is `port`, default 7841. Set it to 0 for any free port, which
is what to use when two mixers share a machine. A port that is taken falls
back to a free one and the log says which; `{action: "where"}` always answers
with the truth.

## What a scene document holds

```json
{
  "id": "0192f3a4-...",
  "name": "speaker strap",
  "content": {
    "graphic": "ograf/lower-third",
    "params": { "name": "{{speaker}}", "title": "Analyst" }
  },
  "transform": { "position": {"x": 120, "y": 760}, "frame": {"w": 960, "h": 200} }
}
```

`content.graphic` is the plugin qualified provide id. `content.params` is the
graphic's own data and is validated against its OGraf schema when
`scene.apply_graphic` writes it: a field the schema has not got is refused with
the field names that would have worked.

An item added with no transform takes the frame its plugin's
`[provides.designer] default_frame` asks for, scaled to the canvas, because a
lower third dropped into the next free cell of a grid is a lower third in the
wrong place.

## Failure, and what it looks like

* **The graphics host is not running.** `scene.apply_graphic` still writes the
  words, and says in the log that it could not push them. The graphic comes up
  filled in when the plugin starts.
* **The page cannot be rendered.** A browser source needs the sidecar or
  `wpesrc`; see [web page sources](web-page-sources.md). The item is on the
  scene and its source is missing, so `program.take` refuses the scene by name
  rather than putting a hole on air.
* **Nothing is showing.** A graphic that is loaded but has not been played is
  showing nothing on purpose. `{action: "status"}` says which. The example
  graphic also hides its bar when it has no name and no title, rather than
  showing an empty one.

## See also

* [Make a graphic](../how-to/make-a-graphic.md), from `gmx plugin new` to a
  lower third on air.
* [Scene commands](scene-commands.md), for `scene.apply_graphic` and
  `scene.item.schema` in the table with everything else.
* [Web page sources](web-page-sources.md), for what renders the page.
