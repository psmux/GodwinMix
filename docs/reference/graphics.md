# Graphics

A graphic is a template with words in it: a lower third, a strap, a score
bug, a title card. GodwinMix does not define a template format. It hosts
[OGraf](https://ograf.ebu.io/), the EBU's open graphics format, so a template
written for another OGraf host plays here and one written here plays there.

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

**Today an opaque graphic is right on air and a transparent one is not.** A
bar with words on it, a full frame card, a solid strap: right. A soft edge, a
rounded corner, a drop shadow, a gap you should see the camera through: the
page's own background covers the picture inside the item's frame.

The reason is the canvas contract. Every source in the graph is converted to
I420 before it reaches a compositor (`caps.rs`), and I420 carries no alpha.
The browser sidecar can render a page with a real alpha channel
(`--transparent`, which makes it emit AYUV), and the `alpha` element behind
`chroma/filter` can key a colour out, but both are flattened by the
`videoconvert` that puts the frame back on the canvas contract before the
compositor sees it.

Measured on this machine with `gst-launch-1.0`, to be exact about it:

```
green ! alpha method=green ! videoconvert ! I420        -> Y 13 U 128 V 128   (black)
green ! alpha method=green ! videoconvert ! I420
      ! compositor over red ! I420                      -> Y 13 U 128 V 128   (black)
green ! alpha method=green ! AYUV
      ! compositor(out AYUV) over red ! I420            -> Y 81 U 90 V 240    (the red underneath)
```

The third line is the fix and it is the only line that differs. What it needs:

1. **The item filter's outgoing caps.** `plugin/filters/chroma.rs` ends its bin
   with a capsfilter pinned to `canvas.video()`, which is I420. A filter that
   declares `media.alpha = true` should be allowed to hand back an alpha format
   instead. One condition on one capsfilter.
2. **The slot's compositor pad.** `mixer/slots.rs` links each slot into `vmix`.
   A pad carrying alpha needs the compositor's negotiated output to be an alpha
   format; `compositor` refuses an AYUV sink pad while its src is pinned to
   I420.
3. **The programme compositor's output.** `mixer.rs::programme_caps` would
   become AYUV, with `videoconvert ! capsfilter(I420)` immediately after it so
   the encoder still gets exactly what it gets today and the canvas contract
   downstream of the compositor is unchanged.
4. **The source's own normaliser.** `plugin/kinds/normalise.rs` pins `vcaps` to
   `canvas.video()`. A source that declares `Capability::Alpha` (the capability
   exists already, and `layered/source` declares it) should be pinned to the
   alpha format instead, so the sidecar's AYUV survives to the slot.

The cost is the reason this is a decision and not an oversight. An AYUV frame
is 4 bytes a pixel where I420 is 1.5, so the programme compositor's working set
goes up by 2.7 times for as long as any alpha source is on the canvas, and 11
section 3 prices a full canvas opaque pad through a second compositor at
0.14 ms a frame. `mixer/group.rs` already refuses alpha for a filtered group
for the same reason and says so in its head. The honest shape is to make the
alpha format a per source decision driven by `Capability::Alpha`, so a show
with no graphic on it pays nothing, which is what "nothing runs unless asked"
means here.

Until that lands, design against a solid shape. Most lower thirds are a
coloured bar, which is why the example one is.

### The luma key fallback

`chroma/filter` is on an item's filter chain and takes `method` (green, blue or
custom), `target_r`, `target_g`, `target_b`, `angle`, `noise` and `spread`. A
graphic served on a solid key colour with that filter on its item is the
fallback the specification asks for, and the wiring is all there. It does not
yet show the camera through, for the reason above, and it is the same one
change: the measurement in the table is exactly this pipeline.

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
