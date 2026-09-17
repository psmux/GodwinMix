# Build a scene by dragging

Two minutes, no training, no reading. Everything on this page is done with a
mouse or a finger in the web UI at `http://your-mixer:8080`, and every gesture
is one command on the same protocol the CLI and an agent use, so nothing you do
here is locked to this page.

If you would rather type, [scenes.md](scenes.md) does the same things from the
command line.

## The two minute path

Open the page. Inputs are tiles on the left, scenes are tiles on the right.

**1. Pick the inputs.** Press on empty space in the tray and drag a box over
the tiles you want. That is the same sweep a file manager does, and Ctrl held
while you sweep adds to what you already had. Ctrl and click picks them one at
a time; Shift and click takes a run.

**2. Drag them into Scenes.** Drop them on the empty space under the scene
tiles. A scene appears with all of them on it, laid out by how many you dropped:
one fills the canvas, two make a two box, three a three box, four a quad, more a
grid. No dialog asks you anything, because there is nothing it would need to
know.

**3. Name it.** Press F2 with the new tile selected (Enter on a Mac keyboard
does the same), type, press Enter. Escape puts the old name back.

**4. Colour it.** Right click the tile and pick a swatch. The name and the
colour live on the document, so the terminal UI, a Stream Deck, the tally and an
agent all see the same ones. "Put the orange one on air" is a sentence an agent
can act on.

**5. Put it on air.** Tap the tile. The picture cuts on the next frame and the
tile takes a red frame. On the tab strip it takes two presses instead: a tap
brings the scene up in the preview monitor, and Cut or Auto under the monitors
puts it out. See [preview-monitor.md](preview-monitor.md).

That is the whole path. If something went wrong, Ctrl+Z takes it back, and a
delete offers an Undo button in the toast rather than asking you first.

## The rest of the grammar

| Gesture | What happens | The command underneath |
|---|---|---|
| tap a tile | it goes on air | `program.take {scene}` |
| tap a tab | it comes up in preview; Cut or Auto sends it | `scene.preview.set`, then `program.take {scene}` |
| the + on a tab | the add picker, and what it makes lands in that scene | `source.add`, then `scene.item.add` |
| drag inputs onto empty space | a new scene, laid out by count | `scene.create_from` |
| drag an input onto a scene tile | it joins that scene, in the next free slot | `scene.item.add` |
| drag an item chip onto another scene | it moves there; hold Alt to copy | `scene.item.move`, `scene.item.copy` |
| F2, or click the name | rename | `scene.rename` |
| a swatch from the right click menu | recolour | `scene.rename {color}` |
| Ctrl+C then Ctrl+V | a duplicate | `scene.duplicate` |
| "Copy the layout", then "Paste the layout onto this" | the target's items take the first scene's positions and sizes | `scene.layout.copy`, `scene.layout.paste` |
| Delete | removed, with an Undo in the toast | `scene.remove`, then `scene.undo` |
| Ctrl+Z, Ctrl+Shift+Z | undo, redo | `scene.undo`, `scene.redo` |
| double tap, or Enter | the composer opens | `scene.edit.begin` |

Copying a layout is the one worth knowing about. It matches items by name
first and by slot order second, and anything that matches nothing is left
exactly where it was. So if last Sunday's scene has items called `pulpit` and
`lyrics` and this week's has the same two names, pasting the layout moves both
and touches nothing else.

## The composer

Double tap a scene tile. The composer opens as a modal, on a copy of the scene,
and Full screen makes it fill the window.

Nothing you do in it reaches air. You are editing a draft (`scene.edit.begin`),
and Apply writes it back; Discard throws it away; closing the window discards
it too. If you want your changes to go out as you make them, the Edit on air
switch at the bottom says so in as many words, and it is off by default because
the usual mistake in OBS is editing the scene that is currently going out.

What you see behind the handles, in the order the composer tries them:

1. **The armed scene, live.** Arm this scene, and the picture is the scene
   itself, composited, at preview rate.
2. **A still.** One frame over the protocol, when a mosaic is running.
3. **The programme, with this scene's layout drawn over it.** Always available,
   and honest: the line under the toolbar tells you which of the three you are
   looking at, because placing an item against the wrong picture is worse than
   placing it against a grey box.

On the canvas:

* Click an item to select it, Ctrl or Shift and click to add, or sweep over
  several. Arrow keys nudge by a pixel, Shift and an arrow by ten.
* Drag the middle to move it. The corners and edges resize; Shift keeps the
  shape, Alt resizes about the centre. The handle above the top edge rotates,
  and Shift snaps that to fifteen degrees.
* Items line up with each other and with the canvas as you drag, and a red
  guide shows what they lined up with. Hold Ctrl to turn that off.
* Safe areas are on by default: the middle 93 percent is action safe, the
  middle 90 title safe, the same two numbers `scene.validate` warns about.

The toolbar is one button per operation rather than a row of number boxes.
Align, space evenly, fit, cover, arrange in a grid, match size, group, ungroup,
bring to front, send to back. Each is a single command in the core, so it cannot
land twelve pixels out and an agent asked to do the same thing gets the same
arithmetic.

The panel on the right is the selected item: its name, opacity, fit, blend mode
and whether its sound is heard, then whatever the plugin behind it offers, then
its filters. Filters hang on the item and not on the source, so a camera keyed
in this scene is not keyed in every other one.

## Tapping straight to air

The tab strip arms rather than cuts, because a strip that sits under the thumb
cannot afford to put something out every time somebody looks at a scene. A
volunteer working alone with three cameras usually wants the single tap back:
"Tap cuts directly", in the Scenes heading, gives it to them and is off by
default. It is a choice per device, not part of the collection, so the desk in
the gallery and the laptop at the back can differ. The number keys cut directly
whatever it is set to.

Tiles are unchanged: a tap on a tile still goes on air, and in producer mode it
arms, as it always did.

## When there is no scene server

An older core has no `scene.*` methods. The Scenes panel says so in one sentence
and the rest of the page carries on: tapping an input still puts it on air,
because a source is a one item scene.
