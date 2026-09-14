# The designer kits

Three kits, not seven designers. A scene designer in any toolkit needs the same
three things, and each of them is a small piece of arithmetic or bookkeeping
that is wrong in a different way every time somebody writes it again:

| Kit | What it holds |
|---|---|
| protocol | the record mirror, patch application, the undo proxy, sequence numbered prediction and reconciliation |
| canvas | item boxes, handles from a plugin's `designer` block, drags, marquee, snapping, rulers, safe areas |
| schema | the UI schema layer over the data schema reader, a ranked renderer registry, the fallback chain |

They ship in three languages, and the drawing is deliberately not one of them:
what each kit answers is where to draw and what to send, and a surface draws it
with whatever it draws with.

| Where | Import | Drawing |
|---|---|---|
| the reference web UI | `ui/kits/` | `ui/kits/canvas/draw.js` (a 2D context), `ui/kits/schema/render.js` (DOM widgets) |
| any TypeScript or node surface | `import { kits } from "@godwinmix/client"` | yours |
| any Python surface | `from godwinmix import kits` | `godwinmix.kits.tkcanvas`, `godwinmix.kits.tkrender` for Tkinter |

## One source of truth, checked two ways

The algorithms are written once, in `ui/kits/`. The ports are real ports, not
bindings, because a Python surface should not need a JavaScript runtime and a
node surface should not need a browser. Three copies of one algorithm drift, so
the behaviour has a single source of truth as well:

```
ui/kits/fixtures.in.json     the cases, by hand
dev/kit-fixtures.mjs         runs the reference kit over them
ui/kits/fixtures.json        what it answered, case by case
```

Every suite replays that file:

| Suite | Runs |
|---|---|
| `ui/test/` (the page, or `dev/ui-tests.sh`) | the reference modules |
| `cd clients/typescript && npm test` | the TypeScript port, and the reference beside it, over the same cases in one run |
| `cd clients/python && python3 -m unittest discover -s tests` | the Python port |

Change an algorithm, run `node dev/kit-fixtures.mjs`, and every port that no
longer agrees fails in its own language naming the case that moved. `node
dev/kit-fixtures.mjs --check` fails instead of writing, which is what CI wants.
Floating point is compared to two decimal places, so a port that multiplies in
a different order is not failed by the last bit of a double.

Sections in the file: `mirror`, `predict`, `merge`, `gizmos`, `handles`,
`drag`, `snap`, `safe`, `schema`, `read`, `uiSchema`. Both the TypeScript and
the Python suites assert that the set of sections they replay is the whole file,
so a new section cannot be added and quietly left unreplayed.

## The protocol kit

The core is authoritative over the document. A client mirrors it, predicts its
own drags locally, and lets the core reconcile.

```js
import { SceneMirror, Prediction, UndoProxy } from "./kits/protocol/index.js";
```

```python
from godwinmix.kits import SceneMirror, Prediction, UndoProxy
```

### SceneMirror

| JavaScript and TypeScript | Python | What it does |
|---|---|---|
| `new SceneMirror({clientId})` | `SceneMirror(client_id=...)` | `clientId` is your own `source_client`, from `core.info`'s `token.id` |
| `applyView(view)` | `apply_view(view)` | take the view a mutating command answered with. That scene's subtree is replaced and no other scene is touched |
| `applyPatch(patch)` | `apply_patch(patch)` | one `event/scene.patch`. Answers `{applied, echo, gap, seq, added, updated, removed}` |
| `reset(views)` | `reset(views)` | replace everything, after a resync |
| `scenes()`, `items(scene)`, `descendants(scene)`, `record(id)`, `sceneOf(id)` | same, snake case | reading |
| `.seq`, `.unknown` | same | the last patch applied, and how many records had a kind this build does not know |

Three rules it follows, and they are the whole of it:

* **Patches, never refetching.** One patch per transaction, ended by
  `event/flush`.
* **Suppress your own echo by `source_client`.** Suppressed does not mean
  discarded: the record still lands, because the core may have clamped what you
  asked for. `echo: true` tells you the change is yours, so an in flight drag is
  not redrawn from under the hand.
* **Ignore what you do not know.** An unknown record kind is kept and not read.
  A patch in a scope this build does not draw is skipped, and the `seq` reported
  is the one the mirror actually reached. A patch that skips numbers answers
  `gap: true`, which is your cue to read the document again rather than carry a
  hole.

### Prediction

A drag cannot wait for a round trip. Not because the socket is slow, a loopback
call is tens of microseconds, but because a blocked redraw costs a frame: a
click tolerates 100 ms and a drag about 25.

```js
const seq = prediction.predict(item, props);     // draw this now
await client.call("scene.item.set", { scene, item, props, seq, duration_ms: 0 });
prediction.settle(seq);                          // the core has caught up
```

| Method | What it does |
|---|---|
| `predict(item, props)` | record the local intent, return the number to send. A second prediction for the same item replaces the first |
| `settle(seq)` | the core applied everything up to `seq`. Returns the items that are the core's again |
| `resolve(item, serverProps)` | what to draw: your own move while it is in flight, the core's record once it has caught up |
| `accepts(item, echoSeq)` | false while you hold a newer move for that item. This is what removes the rubber band |
| `reset()` | throw it all away, for a cancelled drag or a resync |
| `.busy`, `.seq`, `.acked`, `.pending` | state |

`echoSeqOf(patch)` reads the number back off a patch. The reference UI also
settles on the answer to `scene.item.set` itself, which is the echo on the RPC
transport and arrives whether or not the core echoes the number on the patch.

`mergeProps(base, next)` merges the way `scene.item.set` merges: a nested
object is merged, everything else is replaced. Doing it any other way makes the
predicted frame and the confirmed frame differ by whatever you forgot to carry.

### UndoProxy

Undo for the document lives in the core, as an inverse diff stack. A client
that kept its own would be wrong the moment a second client, an agent or the
CLI changed anything.

```js
await undo.group("Moved an item", () => drag());   // marks, runs, marks, records
await undo.undo();                                  // scene.undo
```

`mark(label)` is `scene.history.mark`, which is what folds forty moves of a drag
into one Ctrl+Z. `record(label, {offer})` pushes a line into the surface's own
undo menu, and `offer` asks for an Undo button in a toast, which the destructive
ones want.

## The canvas kit

Nothing here talks to a core or a screen. It is handed numbers and answers with
numbers, which is why the fixtures can check all three ports.

| Function | Answers |
|---|---|
| `itemRect(transform, canvas)` | the box an item occupies, the same arithmetic as `geometry.rs::item_rect` |
| `rectToTransform(rect, transform)` | the reverse: the transform a rectangle asks for, keeping the item's anchor and scale |
| `gizmosFor(designer)` | the handles this item type has: the plugin's, or the defaults. Never empty |
| `handlesFor(box, gizmos)` | where each one sits, in canvas pixels, with a cursor hint |
| `hitTest(handles, x, y, r)` | the handle under a point |
| `applyDrag(handle, start, dx, dy, mods)` | `{props, box}`: what to send and what to draw |
| `snapTargets({canvas, boxes, points})` | every line the moving box could line up with |
| `snapDelta(box, targets, opts)` | `{dx, dy, guides}`. `opts.invert` is the modifier that turns snapping off, `opts.grid` a fallback grid |
| `pointsOf(item, box, snap)` | the points a plugin's `snap.points` names, in canvas pixels |
| `safeAreas(canvas)` | the action safe and title safe rectangles, from the same two constants the validator uses |
| `ticks(length, scale)` | ruler ticks at a spacing a person reads |
| `new View(canvas, surface)` | canvas pixels to the surface the preview is drawn on and back, letterboxed the way `object-fit: contain` draws it |

`mods` on a drag: `aspect` (Shift: keep the shape, or snap a rotation to
fifteen degrees), `centre` (Alt: resize about the centre), `pointer` (where the
pointer is, which a rotation reads rather than integrating deltas), and `min`,
`max`, `from`, `step` for a dial.

Handle archetypes and the long spelling are documented where a plugin author
needs them, in
[extend-the-designer.md](../how-to/extend-the-designer.md).

## The schema kit

The data schema reader is not part of this kit: `@godwinmix/client` and
`godwinmix` already have `describeForm` and `describe_form`, and the kit
re-exports them rather than growing a fourth copy. What the kit adds is the UI
schema layer and the registry.

| Function | What it does |
|---|---|
| `layoutFor(form, ui)` | merge a form description with a UI schema into a layout tree. Every field appears exactly once; a field the UI schema forgot is appended in a section of its own |
| `defaultLayout(form)` | the layout from the data schema alone: its own groups, in order |
| `applyRules(layout, values)` | mark each node visible and enabled, from the UI schema's rules and the data schema's own `if`/`then` |
| `controlsOf(layout)` | every control node, in draw order |
| `controlFor(field, declared)` | the control a field ends up with: what was declared, or what fits |
| `CONTROLS` | the nineteen names the vocabulary defines |

The registry is how a client renders a format better than the kit does without
the plugin knowing:

```js
renderers.register({
  name: "my-colour-wheel",
  test: (node) => (node.control === "template.colour" ? HOST : 0),
  make: (node, ctx) => myWheel(ctx.value(), ctx.set),
});
```

The highest rank wins, ties go to whoever registered last, and zero means "not
this one". `BASE` is what the kit's own widgets rank at, `SPECIFIC` the
`template.*` tier, `HOST` above both. `registerTable(registry, table, rank)`
registers a control name to widget maker table in one call.

`editorFor(spec)` in the browser kit walks the whole fallback chain and answers
`{mode, el, read, why}`, where `why` is the sentence the composer prints under
the inspector so an operator knows whether they are looking at the plugin's own
editor or the last resort.

## Writing a designer in Python

`examples/tkinter-designer.py` is the worked example: a few hundred lines, an
MJPEG preview behind a `tkinter.Canvas`, handles from each plugin's `designer`
block, an inspector generated from its schema, and `scene.edit.begin` with Apply
and Discard. Every item type from every plugin appears in it with its editor
generated, and there is no HTML anywhere.

```bash
python3 examples/tkinter-designer.py --url http://127.0.0.1:8080 --token your-token
```

The two Tk modules are separate files on purpose: `godwinmix.kits` imports no
`tkinter` at all, so a headless script, a Qt surface or a Godot bridge uses the
arithmetic without a display.
