# Put more than one thing on screen

A scene is several sources on the canvas at once, each with its own place, size
and opacity. Taking one is the same instant cut as taking a single camera: the
picture changes on the next frame and nothing downstream notices.

This page builds one from the command line in five commands, reshapes it with a
layout, arms it, and undoes a change. Everything here is a `scene.*` call on the
one protocol, so the web designer, an agent and your own client do exactly the
same thing.

## Before you start

A running core and two sources. If you have no cameras to hand, the built in
test patterns are enough:

```bash
export GODWINMIX_URL=http://127.0.0.1:8080
export GODWINMIX_TOKEN=your-token

gmx ctl source add cam1 test://smpte
gmx ctl source add cam2 test://ball
```

## Five commands

```bash
# 1. A scene from the two sources. Two sources means a two box; you did not
#    have to say so.
gmx ctl scene new "wide and guest" cam1 cam2

# 2. Look at where they landed.
gmx ctl scene get "wide and guest"

# 3. Put it on air.
gmx ctl take --scene "wide and guest"

# 4. Make the guest a picture in picture in the bottom right, over 300 ms.
gmx ctl scene layout pip-bottom-right --values "a=cam1,b=cam2" \
    --scene "wide and guest" --duration 300

# 5. Changed your mind.
gmx ctl scene undo
```

`scene get` prints one line per item with the box it occupies in canvas pixels:

```
wide and guest (1920x1080)
  cam1                 cam1           0,0        960x1080  alpha 1.00
  cam2                 cam2         960,0        960x1080  alpha 1.00
```

The item names come from the sources, which is deliberate: every other command
takes the name, so you type `cam2` and not an id.

## Moving one item

`scene set` is a state assignment. What you leave out stays where it is.

```bash
gmx ctl scene set "wide and guest" cam2 --at 1400,40 --size 480x270
gmx ctl scene set "wide and guest" cam2 --opacity 0.9
```

While the scene is on air, the change is on the next frame. While it is not, it
is written to the document and applied when you take it.

## Lining things up

Three items nudged by hand never quite line up. These are one call each and
cannot be off by twelve pixels:

```bash
gmx ctl scene align "wide and guest" left cam1 cam2
gmx ctl scene grid "wide and guest" --cols 2 cam1 cam2
```

`align` takes `left`, `right`, `top`, `bottom`, `center-x` and `center-y`.
There is also `scene.item.distribute`, `fit_to_canvas`, `cover_canvas` and
`match_size` on the protocol; see
[the command reference](../reference/scene-commands.md).

## Preview, then take

Arming a scene makes it the preview. `take` with no name puts the armed scene
on air, which is the two step an operator expects.

```bash
gmx ctl scene arm "wide and guest"
gmx ctl take
```

Arming costs nothing. Nothing is composited for a preview unless something is
watching one, which is the rule the whole core follows: nothing runs unless
asked.

## Undo

Every change to a scene is undoable, exactly:

```bash
gmx ctl scene undo
gmx ctl scene redo
```

A drag in a designer is forty small moves, and it undoes as one step because the
client sends `scene.history.mark` before it starts. From the command line each
command is its own step.

## Checking your work before you say it is done

```bash
gmx ctl scene check "wide and guest"
```

It reports items off the canvas, text outside the title safe area, an item
completely hidden behind another, and references to scenes that are not there.
`nothing to fix` means there is nothing to fix.

## Editing while something else is on air

Editing is off air by default. Take a working copy, change it, and write it
back when you are ready:

```bash
DRAFT=$(gmx ctl rpc scene.edit.begin '{"scene": "wide and guest"}' | jq -r .draft)
# ... any scene.item.* call with "draft": "$DRAFT" ...
gmx ctl rpc scene.edit.apply "{\"draft\": \"$DRAFT\"}"
```

A draft of the scene that is on air is applied on the next take of that scene,
or when you apply it, and never on each keystroke. A client that wants live on
air editing asks for it with `live: true` and says so in its own interface.

## Where the scenes live

Beside the runtime store, as `<name>.scenes.json`, written every time something
changes. It is the nested form: pretty printed, one property per line, so a diff
in git reads like the picture it describes. Copy it to another machine and the
scenes go with it.

## What to read next

* [The same thing with a mouse, in the web UI](compose-a-scene.md)
* [Every scene command with an example](../reference/scene-commands.md)
* [The scene document, field by field](../reference/scene-document.md)
* [How a scene reaches the compositor](../explanation/how-a-scene-reaches-the-compositor.md)
* [Import an OBS scene collection](import-from-obs.md)
