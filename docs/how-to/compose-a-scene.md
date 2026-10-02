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
tile takes a red frame.

That is the whole path. If something went wrong, Ctrl+Z takes it back, and a
delete offers an Undo button in the toast rather than asking you first.

## The rest of the grammar

| Gesture | What happens | The command underneath |
|---|---|---|
| tap a tile | it goes on air | `program.take {scene}` |
| tap in Studio mode | it goes into Preview instead, and Take sends it | `scene.preview.set` then `program.take {scene}` |
| drag inputs onto empty space | a new scene, laid out by count | `scene.create_from` |
| drag an input onto a scene tile | it joins that scene, in the next free slot | `scene.item.add` |
| drag an item chip onto another scene | it moves there; hold Alt to copy | `scene.item.move`, `scene.item.copy` |
| F2, a double click on the name, or Rename in the right click menu | rename | `scene.rename` |
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

## What each scene looks like

With the input tiles on Live (the choice above the Sources tray, or Tile
pictures in Settings), each scene tab and tile shows a small moving picture of
what that scene puts together, three times a second. The page draws it from the
same mosaic the input tiles use, in the boxes the scene's items sit in, so the
mixer encodes nothing more for it. It runs only while the Scenes panel is on
screen: a panel scrolled away or behind another tab asks for nothing. On icons,
labels or snapshots the scenes show no picture and cost nothing.

A source the mixer does not have is missing from the picture, the same as it is
missing from the programme when that scene goes on air. Graphics and text are
not drawn, because the mosaic has no picture of them.

A new scene starts empty. **New scene** makes one, makes it the scene you are
working on, and Sources shows it with nothing in it and an Add sources button.

## A scene with a source that is not running

A yellow **!** on a scene's tab or tile means it draws a source that is not
running: one that failed, or one the mixer does not have at all. Hover it to
see which. Click it for the Fix dialog, which lists each one by name with the
reason and a button: Retry, Try again, Put back, a way to install what is
missing, another camera, or Remove when there is nothing the page can do. The
boxes are all ticked, and **Remove from this scene** at the bottom takes the
ticked ones out in one step. Undo in the toast, or Ctrl+Z, puts them back.

## The composer

Press the pencil on a scene: it is beside each tab in the tab view and on the
face of each tile in the tile view. Its tooltip says "Edit the layout", because
a pencil beside a name reads as rename to nearly everyone and this one is not
that. A double tap on the tab, or on the face of the tile, opens the composer
too; a double click on the tile's name renames instead. The composer opens as a
modal, on a copy of the scene, and Full screen makes it fill the window.

Nothing you do in it reaches air. You are editing a draft (`scene.edit.begin`),
and Apply writes it back; Discard throws it away; closing the window discards
it too. If you want your changes to go out as you make them, the Edit on air
switch at the bottom says so in as many words, and it is off by default because
the usual mistake in OBS is editing the scene that is currently going out.

What you see behind the handles is the draft you are editing, composited by the
mixer and streamed at 960 wide: drag an item and the video moves with it, which
is what makes this a place to design in. The composer asks the preview to draw
its draft (`scene.preview.set` with `draft`), through the same placements the
programme would get, so what you see is what Apply gives. What is armed stays
armed, so a take with no argument does what it did, and applying or discarding
the draft hands the preview back. It costs a preview compositor for as long as
the composer is open and nothing after.

On a mixer that cannot draw a draft, the composer falls back, in this order:

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
and whether its sound is heard, then how it comes on and goes off, then
whatever the plugin behind it offers, then its filters. Filters hang on the
item and not on the source, so a camera keyed in this scene is not keyed in
every other one.

A chroma key shows its controls under its name: the key colour with **Find**
and **Pick**, similarity, edge softness, spill, feather and the four edges of
the garbage matte. They apply on air as you drag. The composer's own picture
does not run item filters, so it shows the camera unkeyed; the programme shows
the key. A presenter in a designed studio is one step with **Virtual set** in
the Scenes panel: see [put a presenter in a virtual set](virtual-set.md).

**Enter** and **Exit** are the item's own transitions: None, Fade, Slide, Zoom
or Wipe, with the edge a slide or a wipe uses, a length and an easing. **Show
on air** and **Hide on air** under them show or hide the item on the scene
itself, not on the draft you are editing, so the programme plays the Enter or
the Exit; the draft is told the same, so applying it later does not undo the
button. Tick **Also when a scene holding it is taken** and the item plays its
Enter and Exit on a take too, in place of the scene's transition. A lower
third that slides in from the left is Enter: Slide, Left, and Exit: Slide,
Left. See [change scene with a transition](transitions.md#show-and-hide-an-item-with-a-transition-of-its-own).

## Studio mode

**Studio mode**, under the Programme monitor (or the producer switch in
Settings), changes a click from "put it on air" to "put it in Preview". A
click on a scene tab does it too. Take, Cut and the transition sit between
Preview and Programme. See
[Use the mixer controls](preview-monitor.md#studio-mode).

## When there is no scene server

An older core has no `scene.*` methods. The Scenes panel says so in one sentence
and the rest of the page carries on: tapping an input still puts it on air,
because a source is a one item scene.
