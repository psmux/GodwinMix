# Change scene with a transition

A take is a cut: the next frame is the new scene. That is the right answer most
of the time and it costs nothing. When you want the change to be seen rather
than not noticed, name a transition.

```bash
gmx ctl take --scene "wide" --transition fade
```

That is a 300 millisecond crossfade. Everything else on this page is a
variation on it.

Light leaks, glitches, film burns, stingers with alpha, luma wipes and shader
transitions from a pack are on their own page:
[use transitions and effects from packs](transitions-from-packs.md). A take
names one the same way, `--transition light-leak`.

## What ships

| Name | What it looks like | Params |
|---|---|---|
| `cut` | the next frame is the new scene. The default, and what a take has always been | |
| `fade` | both scenes on the canvas, one dissolving into the other | `easing` |
| `move` | anything in both scenes travels from where it was to where it is going; everything else fades | `easing` |
| `stinger` | a clip plays over the top and the scenes swap underneath it | `clip`, `cut_at_ms`, `luma` |
| `wipe` | an edge crosses the canvas with the new scene behind it | `direction`, `easing` |
| `slide` | the new scene slides in over the old one | `direction`, `easing` |
| `push` | the new scene pushes the old one off the far edge | `direction`, `easing` |
| `zoom` | the new scene grows out of a point, the centre unless you name one | `x`, `y`, `easing` |
| `zoom-out` | the old scene shrinks into a point with the new one already under it | `x`, `y`, `easing` |
| `box` | a box opens out of a point with the new scene inside it | `x`, `y`, `easing` |
| `dip` | out to a colour, then in from it. Black unless you say otherwise | `colour`, `easing` |

```bash
gmx ctl take --scene "wide" --transition wipe --direction up
gmx ctl take --scene "wide" --transition slide --direction right --easing ease-out
gmx ctl take --scene "wide" --transition dip --colour white --duration 800
```

Over the protocol the same three are:

```json
{"type": "wipe", "params": {"direction": "up"}}
{"type": "slide", "params": {"direction": "right", "easing": "ease-out"}}
{"type": "dip", "duration_ms": 800, "params": {"colour": "white"}}
```

* `direction` is the way the new scene travels: `left` (the default, in from
  the right edge), `right`, `up` or `down`. It is what `plugins/wipe` means by
  the word too.
* `x` and `y` are where a zoom or a box starts, as fractions of the canvas
  from 0 to 1. Both are 0.5 by default, which is the centre.
* `colour` is `black`, `white` or `#rrggbb`. `color` is read too.
* `easing` is `linear`, `ease-in`, `ease-out` or `ease-in-out`. The default is
  `ease-in-out`, the curve `fade` has always had, so a fade taken the way it
  always was looks the way it always did.

A word nobody knows is refused with the words that would have worked, in the
message and in `data`: a wipe asked to go sideways answers with
`data.directions`, and an easing called `bounce` with `data.easings`.
`program.transitions` (and the `list_transitions` tool, through
`search_tools`) lists every transition this core takes, with the params each
one reads.

There is no iris. A circle cannot be cut out of a picture with a crop, and
drawing one means a mask over the whole canvas every frame, which is the
second compositing pass the software path does not add. `box` is the
rectangle that a crop can cut.

A `transition` plugin adds its own name to that list. `plugins/wipe` ships in
this repository as the worked example, and it is called `wipe` too: when it is
installed and running, `--transition wipe` is the plugin's, because an
operator who installed it asked for it. Remove the plugin and the name is the
built in one again.

## How long it takes

```bash
gmx ctl take --scene "wide" --transition fade --duration 800
```

Or, over the protocol, as an object:

```json
{"method": "program.take",
 "params": {"scene": "wide", "transition": {"type": "fade", "duration_ms": 800}}}
```

A name on its own takes 300 ms. A duration of 0 is a cut whatever the name
says, which is the documented way to ask for one from a client that always
sends a transition field. The ceiling is ten seconds: both scenes are on the
canvas for the whole of a transition, so one that never ends is a scene that
never leaves.

## Move, and why it needs your item ids

`move` matches the items of the outgoing scene against the items of the
incoming one and travels the ones that are in both:

```bash
gmx ctl take --scene "presenter-full" --transition move --duration 500
```

If `presenter-full` is `wide-and-inset` with the inset grown to full screen,
`move` grows the inset. It knows which item is which because a layout change
keeps item ids: an item copied from one scene to another with
`scene.item.copy`, or a layout applied with `scene.apply_layout`, carries its
id across. Two scenes built independently have no ids in common, so `move`
falls back to matching by source, which does the right thing for the camera
that is in both and fades everything else.

Nothing to match is not an error. A `move` between two scenes with nothing in
common is a crossfade.

## Stingers

A stinger is a clip that plays over both scenes, with the cut happening
underneath it at the moment it covers the canvas.

```json
{"method": "program.take",
 "params": {"scene": "half-time",
            "transition": {"type": "stinger",
                           "duration_ms": 1200,
                           "params": {"clip": "stinger.mp4", "cut_at_ms": 600}}}}
```

* `clip` is a source already in the mixer, or a file path or URI the core adds
  for the length of the transition and takes away after.
* `cut_at_ms` is when the scenes swap underneath. Half way by default, which
  is where most stingers have their cover.
* `luma` keys the clip's black out, so its own luminance decides what it
  draws through. On by default.

The scenes do not dissolve under a stinger: they swap on one frame, which is
what makes it look like an edit rather than a mix.

## Naming one in a collection

A scene collection can carry its own transitions, so a church that has settled
on a 400 millisecond dissolve calls it `house` and every operator means the
same thing by it:

```json
{"transitions": [
  {"name": "house", "type": "fade", "duration_ms": 400},
  {"name": "sting", "type": "stinger", "duration_ms": 1200,
   "params": {"clip": "stinger.mp4"}}
]}
```

```bash
gmx ctl take --scene "wide" --transition house
```

A name the collection knows wins over a built in one, so a collection can give
`fade` a duration of its own. A duration on the call still overrides it once,
without redefining anything.

## Taking over the top of one

A take during a transition ends it where it stands and lands on the newest
scene. Nothing waits, nothing queues, and the older transition abandons its
curves rather than fighting the new ones. That is what an operator pressing a
button twice expects, and it is what the `transition_id` on
`event/program.took` is for: two takes in quick succession are told apart by
that number rather than by timing.

## On a schedule

```bash
gmx ctl take --scene "wide" --transition fade --at 1800000
```

The take is armed on the pipeline clock and the transition starts when it
lands. Measured on this machine, a 300 millisecond crossfade was half done
within 0.0 ms of where it was armed to be, against a frame of 33.3 ms; see
[the transitions reference](../reference/transitions.md) for how that is
measured and why it is not luck.

## Show and hide an item with a transition of its own

A scene item can carry an `enter` and an `exit`. They play when the item is
shown or hidden while its scene is on air, which is the lower third that
slides in from the left when the speaker starts and slides back out when they
stop:

```bash
gmx ctl rpc scene.item.set '{"scene": "service", "item": "lower-third",
  "props": {"enter": {"type": "slide", "edge": "left", "duration_ms": 400, "easing": "ease-out"},
            "exit":  {"type": "slide", "edge": "left", "duration_ms": 300}}}'

gmx ctl rpc scene.item.set '{"scene": "service", "item": "lower-third", "props": {"visible": true}}'
gmx ctl rpc scene.item.set '{"scene": "service", "item": "lower-third", "props": {"visible": false}}'
```

| Field | What it is |
|---|---|
| `type` | `cut`, `fade`, `slide`, `zoom` or `wipe` |
| `edge` | for `slide` and `wipe`: the canvas edge it comes in from and goes out to. `left` by default |
| `duration_ms` | 300 by default, ten seconds at most |
| `easing` | as for a scene transition |
| `on_take` | also play it when a scene holding the item is taken, in place of the scene's own transition for this item |

An item that slides in ends exactly where the scene put it, because the last
frame of an entrance is the item's own placement. An item that slides out
leaves the canvas and is hidden on its last frame. An item with no `enter` or
`exit` is shown and hidden on the next frame, as it always was. A group's own
`enter` and `exit` are not played; give them to the items inside it, and
`scene check` says so.

In the web UI the same two are the Enter and Exit rows in the composer's
inspector, with Show on air and Hide on air beside them. See
[build a scene by dragging](compose-a-scene.md).

## What it costs

A transition costs only while it runs. Nothing is added to the pipeline for
one and nothing is left behind after it: every new transition moves, sizes or
trims pads and slot crops the programme already has.

Measured on this Mac (Apple silicon, software compositor, release build) at
1280x720 and 30 fps, between two full canvas test patterns: ten 500 ms
transitions, 600 ms apart, against the same ten takes as cuts. The figure is
the whole process's CPU over the run, so it includes the two sources, the
encoder and the 100 ms between transitions.

| Transition | CPU, cores | Against a cut | Largest frame interval |
|---|---|---|---|
| `cut` | 0.116 | | 33.3 ms |
| `slide` | 0.118 | +1.7 percent | 33.3 ms |
| `push` | 0.120 | +3.3 percent | 33.3 ms |
| `dip` | 0.137 | +17.8 percent | 33.3 ms |
| `fade` | 0.149 | +28.6 percent | 33.3 ms |
| `wipe` | 0.183 | +57.5 percent | 33.3 ms |
| `zoom` | 0.192 | +65.4 percent | 33.3 ms |
| `zoom-out` | 0.192 | +65.2 percent | 33.3 ms |
| `box` | 0.213 | +83.7 percent | 33.3 ms |

The programme never missed a frame in any of them. A slide and a push are the
cheapest thing a compositor does: a pad moves, nothing is blended at partial
alpha and the part off the canvas is not drawn. A dip and a fade blend. A
zoom changes a pad's size every frame, and the compositor rebuilds its scaler
for that pad when the size changes. A wipe and a box change the slot's crop
every frame, which renegotiates the slot's caps and rebuilds the converter
the same way; a box trims on four sides where a wipe trims on one, and a wipe
leaves the old scene whole underneath when the new one covers the canvas.

Both scenes are drawn for the length of any transition, so a crossfade between
two eight item scenes puts sixteen pictures on the canvas at once and the slot
pool grows to hold them. Measured here on the software compositor, six 300
millisecond crossfades between two eight item scenes left the programme's
largest inter frame interval at 33.3 ms, which is one frame at 30 fps: the
transition costs pads, and pads at alpha 0 cost nothing, and the compositor
was never late.

Run them yourself:

```bash
cargo test --release -p godwinmix-core --lib -- --ignored --nocapture what_each_transition_costs
cargo test -p godwinmix-core -- --ignored --nocapture crossfade
```

## See also

* [transitions reference](../reference/transitions.md): the built in types in
  full, item transitions, the plugin `render` contract, and the accuracy
  measurement
* [the scene document](../reference/scene-document.md): `enter` and `exit` on
  an item
* [put more than one thing on screen](scenes.md): the scenes a transition moves
  between
* [how a scene reaches the compositor](../explanation/how-a-scene-reaches-the-compositor.md):
  why a take is free, and what a transition adds to it
