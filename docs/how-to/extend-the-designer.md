# Make your plugin editable in every designer

Your plugin ships a settings schema already. Add six lines to its manifest and
it gets handles on the canvas, an inspector with real controls, an icon in the
add gallery and a place in the tray, in the web composer, in the Tkinter
example, in a Qt or Godot surface somebody else wrote, and in an agent's view of
the scene. You write no HTML and no Python. It is data.

## The block

```toml
[[provides]]
kind = "source"
id = "source"
settings = "schemas/source.json"          # the data schema you already have

[provides.designer]
icon = "ui/icon.svg"                       # the tile in the add gallery
thumbnail = "ui/thumb.svg"                 # what it looks like in the tray
ui = "schemas/source.ui.json"              # optional: how to lay the form out
default_frame = { w = 480, h = 270 }       # where a new one lands
gizmos = ["corner", "rotate"]              # which handles it has
snap = { bounds = true }                   # what it lines up by
editor = "ui/editor.js"                    # optional: your own web component
```

Only `source`, `filter` and `graphic` provides may carry one. Every path must
exist and must be inside the plugin's directory; `gmx plugin check` says so if
it is not.

## Handles

`gizmos` is a list. The short spelling is a name, and each name is a set of
handles:

| Name | What it puts on the canvas |
|---|---|
| `cage` | the body of the item, for moving it. Added for you whatever else you ask for |
| `corner` | four corners that resize, keeping the opposite corner still |
| `edge` | four edge handles that resize one side |
| `scale` | both of the above |
| `crop` | four edge handles that trim the content instead of resizing the box |
| `rotate` | a handle above the top edge |

Leave `gizmos` out and your item gets the default set: move, resize from any
corner or edge, rotate. That is the right answer for most source types, which
is why it is the default rather than something you have to write.

The long spelling from 11 section 5 is a table per handle, and you can mix the
two in one list:

```toml
gizmos = [
  "corner",
  { kind = "dial", anchor = [0.4, 0.4], action = "set", target = "params.key_tolerance", cursor = "ew-resize" },
]
```

* `kind` is what it looks like: `corner`, `edge`, `rotate`, `dial`, `cage`.
* `anchor` is where it sits, normalised about the item's centre: `[-0.5, -0.5]`
  is the top left corner of the box, `[0, -0.62]` is above the top edge, `[0.5,
  0.5]` the bottom right. Normalised, so it is right at any canvas size and
  after any resize.
* `action` is what dragging it does: `move`, `scale`, `crop`, `rotate` or `set`.
* `target` is the property it drives, as a dotted path into the item:
  `transform.frame`, `crop.right`, `transform.rotation`, `params.anything`.
* `cursor` is a hint, and the client picks a sensible one when you leave it out.

A client turns a drag on one of those into a named command with the property
you asked for. It never sends a client's idea of a coordinate, which is why the
same block draws the same handles in a browser and in a Tkinter window.

## Snapping

```toml
snap = { bounds = true, points = ["params.anchor_point"] }
```

You answer with geometry. `bounds` means this item type lines up by its box,
which is nearly always what you want. `points` names properties holding a
normalised point inside the item, and each becomes something other items can
line up with.

Thresholds, the modifier that turns snapping off, the grid and the drawing of
the guides are all the client's business, not yours. That split is what lets a
Raspberry Pi use a wider threshold than a workstation without every plugin
author being asked about it.

## The inspector, and the fallback chain

The client tries four things in order and the last one always works, so a
missing editor never blocks anybody.

**1. Your own editor**, when `editor` names one and the operator trusts the
plugin. It is an ES module served from `/plugins/<you>/ui/`, and it defines a
custom element:

```js
// ui/editor.js
export const tag = "gmx-chroma-editor";     // say what you defined

class ChromaEditor extends HTMLElement {
  setSchema(schema) { this.schema = schema; }
  setValue(value) { this.value = value; this.render(); }
  read() { return this.value; }             // what Apply sends
  render() { /* your controls, your markup */ }
}
customElements.define(tag, ChromaEditor);
```

Export `tag` so the host knows what you registered; without it the host looks
for `gmx-<plugin>-editor`. Fire a `change` event when the operator changes
something and the host will ask `read()` for the new value. If the module fails
to load, the console says so and the chain carries on to the next link, which
is exactly what should happen.

**2. Your UI schema**, when `ui` names one. JSON Schema says presentation is out
of scope and it is right: a number between 0 and 1 might be a slider, a spin box
or a dial, and nothing in the data schema says which. So say it here.

```json
{
  "type": "vertical",
  "elements": [
    { "type": "group", "label": "Key", "elements": [
      { "type": "control", "scope": "#/properties/key", "control": "template.chroma" },
      { "type": "control", "scope": "#/properties/tolerance", "control": "slider" }
    ]},
    { "type": "control", "scope": "#/properties/spill", "control": "slider",
      "rule": { "effect": "show", "condition": { "scope": "#/properties/key", "equals": "green" } } }
  ]
}
```

Containers are `vertical`, `horizontal`, `group` and `tabs`. A leaf is a
`control` with a `scope` naming a property of the data schema. The controls are:

```
text  textarea  number  slider  integer  boolean  select  multiselect
radio  colour  file  font  source  unit  json
template.colour  template.curve  template.chroma  template.meter
```

The `template.*` four are the host's, not yours: you ask for a colour picker or
a chroma picker and whatever surface is rendering you provides the best one it
has. A `rule` shows, hides, enables or disables an element from another
property's value.

A property you forget to mention still appears, in a section at the end.
Forgetting a field in the UI schema should give you an ugly form, never a
setting nobody can reach.

**3. Your data schema**, with default widgets. This is what you get with no
`ui` at all, and for most plugins it is enough: titles, descriptions, enums,
minimums, `x-gmx-unit` for a suffix and `x-gmx-group` for a section.

**4. A JSON box.** No schema at all, and the operator edits the values
directly. Nothing is ever uneditable.

## Try it without writing a plugin

The composer reads the designer block through `plugin.describe`, so anything
you can install you can see. The quickest loop is `gmx plugin new --kind
source`, add the block to the manifest it writes, `gmx plugin add ./your-plugin`
and reload the page.

The two kits that read your block are documented in
[designer-kits.md](../reference/designer-kits.md), including how to check your
manifest against them from a script.

## What a designer tool looks like

A new operation (an auto layout, "make a two box from the selection") is not a
designer feature at all: it is a `service` plugin that registers commands under
`scene.<you>.*`. Every client generates its menus from the command list, so your
operation appears in the web palette, in the TUI and in an agent's tool search
without any of them knowing your name.
