---
name: ograf-graphics
description: Put a lower third, a strap, a scoreboard or any other OGraf graphic on a GodwinMix scene and drive it. Use when somebody wants words on the picture: a speaker's name, a title card, a score, a ticker. Covers placing a graphic, filling its fields by name, playing it on and taking it off, writing a graphic of your own, and what transparency costs today.
---

# Graphics

A graphic is an [OGraf](https://ograf.ebu.io/) template: a `graphic.ograf.json`
saying what words go in it, and a web component that draws them. This plugin
serves each placement as its own page, and the mixer renders that page as an
ordinary source. Nothing in the compositor knows a graphic from a camera.

## Put one on a scene

```
scene.item.add {scene: "Wide", content: {graphic: "ograf/lower-third"}, name: "speaker strap"}
```

That does three things at once: it puts an item on the scene, it starts a
browser source pointed at this host, and it loads the graphic into it. The
answer names the source id, which is what `instance` means everywhere below.

## Fill it in

Discover the fields, then fill them by name. Never by position.

```
scene.item.schema {type: "ograf/lower-third"}
scene.apply_graphic {graphic: "ograf/lower-third", values: {name: "Ada Lovelace", title: "Analyst"}}
```

`scene.apply_graphic` finds the placement for you and answers with the values
as the graphic will render them. With two straps on the canvas, add
`item: "speaker strap"` to say which.

## Play it on and take it off

```
scene.apply_graphic {graphic: "ograf/lower-third", values: {...}, play: true}
scene.apply_graphic {graphic: "ograf/lower-third", stop: true}
```

Or through the tool, when you have the instance:

```
tool.call {name: "ograf/graphic", arguments: {instance: "graphic-lower-third-9f2c41ab", action: "play"}}
tool.call {name: "ograf/graphic", arguments: {instance: "graphic-lower-third-9f2c41ab", action: "stop"}}
```

`stepCount` in the manifest says how many steps `play` walks through. One means
in and out, which is what a lower third wants.

## Bind a field to the whole show

A `{{name}}` in a field follows the collection's parameters, so one call
changes every strap that uses it:

```
scene.item.set {scene: "Wide", item: "speaker strap", props: {content: {graphic: "ograf/lower-third", params: {name: "{{speaker}}"}}}}
scene.params.set {values: {speaker: "Grace Hopper"}}
```

## Write one

```
gmx plugin new --kind graphic my-strap
cd my-strap && ./check
```

That gives you a manifest, a web component with the four OGraf methods on it,
and a preview page. Edit `graphic.mjs`, open the preview, and when it looks
right install it with `gmx plugin add ./my-strap`. The long version is in
`docs/how-to/make-a-graphic.md`.

## What transparency costs

The whole mixer graph is I420, which carries no alpha. A graphic that is an
opaque shape (a bar with words on it, a full frame card) is right on air today.
A graphic with a soft edge, a rounded corner or a gap you should see the camera
through is not: the page's own background covers the picture inside the item's
frame. `docs/reference/graphics.md` says exactly what the change is and what it
would cost. Design against a solid bar until then, which is what most lower
thirds are anyway.

## When something is blank

* `tool.call {name: "ograf/graphic", arguments: {action: "status"}}` says what
  each placement is loaded with and whether it is playing. A graphic that is
  loaded but not playing is showing nothing on purpose.
* Open the host's own index in a browser (the address is in `action: "where"`)
  to see the page outside the mixer.
* A graphic with no `name` and no `title` hides its bar rather than showing an
  empty one, so "nothing on air" can mean "nothing was filled in".
