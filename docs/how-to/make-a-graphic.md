# Make a graphic

A lower third on air, from nothing, in about twenty minutes. You need a text
editor and a browser. There is no Rust in a graphic and no build step.

A graphic is an [OGraf](https://ograf.ebu.io/) template: a JSON file saying
what words go in it and a web component that draws them. GodwinMix hosts the
format rather than inventing one, so what you write here plays in any OGraf
host and a template somebody else wrote plays here.

## 1. Put the one that ships on a scene

Before writing your own, see one work.

```sh
gmx plugin add ./plugins/ograf
gmx ctl graphic list
```

```
ograf/lower-third            Lower third              1 step(s)  colour, name, side, title
```

Put it on a scene over a camera:

```sh
gmx ctl scene new "Wide" cam1
gmx ctl scene add "Wide" --graphic ograf/lower-third --name "speaker strap"
```

It lands in the frame the plugin asks for, which for a lower third is the
bottom left. Now put words in it and bring it on:

```sh
gmx ctl graphic apply ograf/lower-third --set name="Ada Lovelace" --set title=Analyst --play
```

```
ograf/lower-third on speaker strap, playing
  colour           "#1f6f4f"
  name             "Ada Lovelace"
  side             "left"
  title            "Analyst"
```

`--play` brings it on, `--stop` takes it off. Change the words while it is on
air by leaving both off: that is `updateAction`, which does not replay the
animation.

Take the scene and it is on the programme:

```sh
gmx ctl take --scene "Wide"
```

## 2. Write one of your own

```sh
gmx plugin new my-strap --kind graphic
cd my-strap
```

You get a working graphic, not a skeleton:

```
my-strap/
  gmx-plugin.toml        the manifest: one graphic provide, no [run] at all
  graphic.ograf.json     what it is called and what words go in it
  graphic.mjs            the web component that draws them
  preview.html           open this in a browser while you work
  ui/icon.svg            the tile in the add gallery
  check                  what to run before you install it
  README.md, AGENTS.md, skills/
```

There is no `[run]` block because a graphic has no process. The graphics host
already installed is what serves it.

### Look at it while you work

```sh
gmx-ograf --serve --root .
```

```
serving on http://127.0.0.1:7841
  http://127.0.0.1:7841/graphic/my-strap/my-strap
```

Open that address. Reload after every edit; there is nothing to rebuild. Put
words in without a mixer by hand:

```
http://127.0.0.1:7841/graphic/my-strap/my-strap?values={"name":"Ada"}&play=1
```

`preview.html` does the same with buttons, which is easier when you are
working on the animation.

### The manifest

```json
{
  "id": "my-strap",
  "name": "My strap",
  "main": "graphic.mjs",
  "stepCount": 1,
  "schema": {
    "type": "object",
    "properties": {
      "name": {
        "type": "string",
        "title": "Name",
        "description": "The big line.",
        "default": "",
        "examples": ["Ada Lovelace"]
      }
    }
  }
}
```

`schema` is the whole of what a person or an agent can fill in. Every client
renders its inspector from it, so a `title` and a `description` on each
property is what a stranger sees beside the box. A `default` is what the
graphic shows before anybody has said anything. `format: "color"` gets a
colour picker; an `enum` gets a dropdown.

`stepCount` is how many steps `playAction` walks through. One means in and
out, which is what a lower third wants. Three would be a three line credit
roll that advances a line at a time.

### The component

```js
export default class MyStrap extends HTMLElement {
  async load(params) { this.write(params.data); return { status: 0 }; }
  async updateAction(params) { this.write(params.data); return { status: 0 }; }
  async playAction(params) { this.on(params.step === 1); return { status: 0, duration: 400 }; }
  async stopAction() { this.on(false); return { status: 0 }; }
}
```

Four methods, each answering `{ status: 0 }`. `load` puts the words in and
shows nothing. `playAction` brings it on. That separation is the whole point:
a graphic is loaded when the scene is built, minutes before it is played.

Two rules the template already follows and worth keeping:

* **Paint nothing you do not mean.** The page's background is transparent and
  what is behind a graphic is the mixer's business.
* **Show nothing rather than something empty.** A strap with no name hides its
  bar. Every first template shows an empty box on air once.

### Check it

```sh
./check
```

It holds you to the four OGraf methods, checks the manifest has what it needs,
and checks the module the manifest names is actually there.

### Install it

```sh
gmx plugin add .
gmx plugin test .
```

`gmx plugin test` is the conformance harness. A graphic has no process and no
media, so it checks the manifest, every schema and every `SKILL.md`, and says
so rather than pretending to check frames.

Now it is in the gallery beside the one that ships, and everything in step 1
works with `my-strap/my-strap` in place of `ograf/lower-third`.

## 3. One name, every strap

A show has a speaker. Their name is in the lower third, in the title card and
in the credit, and typing it three times is how it ends up spelled two ways on
air. Write the field as a binding instead:

```sh
gmx ctl scene set "Wide" "speaker strap" # in the designer, set name to {{speaker}}
```

or over the protocol:

```json
scene.item.set {
  "scene": "Wide",
  "item": "speaker strap",
  "props": { "content": { "graphic": "ograf/lower-third", "params": { "name": "{{speaker}}" } } }
}
```

Then one call changes all of them:

```json
scene.params.set { "values": { "speaker": "Grace Hopper" } }
```

A parameter that does not exist yet is made, typed from the value you give.
A binding nobody has filled in shows as `{{speaker}}` rather than blank, so a
half filled show says what is missing.

## 4. From an agent

The same three calls, and the shape is discover by name then fill by name.

```json
scene.graphic.list {}
scene.item.schema { "type": "my-strap/my-strap" }
scene.apply_graphic { "graphic": "my-strap/my-strap", "values": { "name": "Ada Lovelace" }, "play": true, "frame": true }
```

`frame: true` answers with a still as well as the records, so the model can
check its own work. A field the schema has not got is refused with the field
names that would have worked, rather than written and ignored.

## What will not work yet

A graphic with a soft edge, a rounded corner or a gap you should see the
camera through. The mixer's canvas is I420 and carries no alpha, so the page's
own background covers the picture inside the item's frame. An opaque shape, a
bar or a full frame card, is right on air today.
[The reference page](../reference/graphics.md#transparency-and-what-it-costs)
says exactly what the change is and what it would cost. Design against a solid
bar until then, which is what most lower thirds are anyway.

## When something is blank

* `gmx ctl graphic apply <id> --set ...` with no `--play` loads the words and
  shows nothing, on purpose. Add `--play`.
* Open the host's own page in a browser. If it is blank there it is the
  template, and the browser's console says why.
* `program.take` refuses a scene whose graphic has no source rather than
  putting a hole on air, and names the source that is missing. That means the
  page could not be rendered: see
  [web page sources](../reference/web-page-sources.md).

## See also

* [Graphics reference](../reference/graphics.md): the OGraf subset, the
  actions, the host's routes, and the alpha path.
* [Share a collection](share-a-collection.md): sending a show with its
  graphics to somebody else.
* [Compose a scene](compose-a-scene.md): where a graphic sits among everything
  else on the canvas.
