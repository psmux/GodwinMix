# Change scene with a transition

A take is a cut: the next frame is the new scene. That is the right answer most
of the time and it costs nothing. When you want the change to be seen rather
than not noticed, name a transition.

```bash
gmx take --scene "wide" --transition fade
```

That is a 300 millisecond crossfade. Everything else on this page is a
variation on it.

## The four that ship

| Name | What it looks like |
|---|---|
| `cut` | the next frame is the new scene. The default, and what a take has always been |
| `fade` | both scenes on the canvas, one dissolving into the other |
| `move` | anything in both scenes travels from where it was to where it is going; everything else fades |
| `stinger` | a clip plays over the top and the scenes swap underneath it |

A `transition` plugin adds its own name to that list. `plugins/wipe` ships in
this repository as the worked example; install it and `--transition wipe`
works like the four above.

## How long it takes

```bash
gmx take --scene "wide" --transition fade --duration 800
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
gmx take --scene "presenter-full" --transition move --duration 500
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
gmx take --scene "wide" --transition house
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
gmx take --scene "wide" --transition fade --at 1800000
```

The take is armed on the pipeline clock and the transition starts when it
lands. Measured on this machine, a 300 millisecond crossfade was half done
within 0.0 ms of where it was armed to be, against a frame of 33.3 ms; see
[the transitions reference](../reference/transitions.md) for how that is
measured and why it is not luck.

## What it costs

Both scenes are drawn for the length of the transition, so a crossfade between
two eight item scenes puts sixteen pictures on the canvas at once and the slot
pool grows to hold them. Measured here on the software compositor, six 300
millisecond crossfades between two eight item scenes left the programme's
largest inter frame interval at 33.3 ms, which is one frame at 30 fps: the
transition costs pads, and pads at alpha 0 cost nothing, and the compositor
was never late.

Run it yourself:

```bash
cargo test -p godwinmix-core -- --ignored --nocapture crossfade
```

## See also

* [transitions reference](../reference/transitions.md): the built in types in
  full, the plugin `render` contract, and the accuracy measurement
* [put more than one thing on screen](scenes.md): the scenes a transition moves
  between
* [how a scene reaches the compositor](../explanation/how-a-scene-reaches-the-compositor.md):
  why a take is free, and what a transition adds to it
