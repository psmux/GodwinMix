# Transitions

What a transition is, what each built in one does, what a transition plugin
has to answer, and how accurately any of it lands.

The page for getting one working is
[change scene with a transition](../how-to/transitions.md).

## The shape

A transition is a set of curves, not a set of writes.

Each curve is a `GstInterpolationControlSource` bound to one property of one
compositor pad, holding timed values in running time. The aggregator calls
`gst_object_sync_values` on every pad once per output frame, with the running
time of the frame it is about to blend, so the value that lands on the picture
is the value the curve says for that picture. A frame late or early is not
possible: the number is a function of the frame.

```
 take ---> curves ---> control sources ---> compositor pad properties
             |                                   ^
             |  bound at the running time        |  sampled once per
             |  the take was armed for           |  output frame
             +-----------------------------------+
```

The properties a transition may drive:

| Property | On | Type |
|---|---|---|
| `alpha` | a compositor pad | 0 to 1 |
| `xpos`, `ypos` | a compositor pad | canvas pixels |
| `width`, `height` | a compositor pad | canvas pixels |
| `volume` | an audiomixer pad | 0 to 10, 1 is unity |
| `left`, `top`, `right`, `bottom` | a slot's `videocrop` (built in transitions only) | source pixels |

Nothing else. `zorder` is deliberately not on the list: a transition that could
reorder the canvas could put an item somewhere the scene document does not
say it is. A built in transition says which scene is drawn on top
(`Layering`), and the mixer moves the outgoing pads into a band of their own
for the window: 100 and up to go under the new scene, 101000 and up to go
over it. The next apply puts them back.

A plugin cannot drive a crop. The four crop properties are on the slot's own
element rather than on a compositor pad, and a plugin is told pad names only.

## The built in transitions

Every one takes `params.easing`: `linear`, `ease-in` (cubic), `ease-out`
(cubic) or `ease-in-out` (smoothstep, the default and the curve `fade` has
always had). A name this build does not know is refused with `data.easings`.

### `cut`

No curves. The next frame is the new scene, which is what a take has always
been and what `duration_ms: 0` means whatever type is named.

### `fade`

Every outgoing pad's `alpha` falls from where it is to nothing; every incoming
pad's rises from nothing to the alpha its scene asks for. Geometry is not
touched, so an item that is in both scenes in two different places is two
pictures crossing, which is what a dissolve looks like and why `move` exists
for when it is not what you wanted.

Audio crosses with it: each source's audiomixer pad `volume` moves between
where it is and where the new scene wants it.

### `move`

Items in both scenes travel. Matching is by item id first and by source id
second:

* **By item id**, because that is what the document means by "the same thing in
  both scenes". An item that is the inset in one scene and full screen in the
  next is one item and should travel.
* **By source**, so two scenes built independently, where nobody thought about
  ids, still move the camera that is in both rather than dissolving it into
  itself.

A matched pair drives `xpos`, `ypos`, `width`, `height` and `alpha` on the
incoming pad, starting from where the outgoing pad was. The outgoing pad steps
straight to nothing rather than lingering at half alpha underneath, because its
picture is continuing on the other pad. Anything unmatched fades.

### `stinger`

A clip drawn over both scenes, with the scenes swapping underneath it on one
frame.

| Param | Default | What it is |
|---|---|---|
| `clip` | required | a source already in the mixer, or a file path or URI the core adds for the transition and removes after |
| `cut_at_ms` | half the duration | when the scenes swap underneath |
| `luma` | `true` | key the clip's black out, so its luminance is its coverage |

The clip is added as an ordinary source under the reserved id `__stinger__`, so
it is normalised to the canvas contract like every other picture, and it is
drawn above the live band so it covers whatever is under it. The scene swap is
a step curve one microsecond wide, which is a cut in everything but spelling:
the scenes do not dissolve, because a stinger that dissolves underneath looks
like two transitions at once.

The canvas stays I420. Only the clip's own pad carries alpha, which is the
cheap half of the AYUV price 11 section 3 puts at 3.5 times I420. A clip whose
decoder gives alpha (WebM VP8 or VP9 alpha, ProRes 4444, QuickTime Animation)
is drawn by the overlay board by that alpha.

`luma` is read and, so far, does nothing: an opaque clip's black is not keyed,
and the clip covers the picture wherever it is drawn. For a clip on black, a
light leak or a burn, import it into the fx library instead, where it is drawn
with Screen, Add or a luma key and its cut point is measured; see
[transitions and effects from packs](fx.md).

### `wipe`

| Param | Default | What it is |
|---|---|---|
| `direction` | `left` | `left`, `right`, `up` or `down`: the way the edge travels, the new scene behind it |

The new scene is drawn over the old and trimmed to the part of the canvas the
edge has crossed. The trim is the slot's own `videocrop`, and the pad is moved
and narrowed by the same amount, so the picture stays where it is and is
revealed rather than squashed. When the new scene does not cover the whole
canvas, the old one is trimmed to the other side of the edge too, so a gap in
the new scene shows the canvas behind rather than the old scene through it.
When it does cover the canvas, the old scene is left whole underneath and
leaves on the last frame, which saves a second crop.

A picture that is turned (a `rotation` of 90, 180 or 270) has its crop before
the turn, so its axes are not the canvas's; such an item fades by how much of
it the edge has revealed instead. So does a filtered group.

### `box`

| Param | Default | What it is |
|---|---|---|
| `x`, `y` | `0.5`, `0.5` | where the box opens from, as fractions of the canvas |

A rectangle grows from the point to the whole canvas, with the new scene inside
it, trimmed by the slot crop the way a wipe is. The old scene stays whole
underneath and leaves on the last frame, because a crop cannot cut a hole.
There is no circular iris, for the same reason.

### `slide` and `push`

| Param | Default | What it is |
|---|---|---|
| `direction` | `left` | the way the new scene travels |

`slide` moves the new scene one canvas along the direction into place, over
the old scene, which stays where it is and leaves on the last frame. `push`
moves the old scene out ahead of it by the same distance. Only `xpos` or
`ypos` is driven.

### `zoom` and `zoom-out`

| Param | Default | What it is |
|---|---|---|
| `x`, `y` | `0.5`, `0.5` | the point the scene grows out of or shrinks into |

`zoom` scales every pad of the new scene about the point, from nothing to its
place, over the old scene. `zoom-out` scales the old scene into the point over
the new one, which is there from the first frame. A pad smaller than two
pixels is not drawn rather than drawn as a dot.

### `dip`

| Param | Default | What it is |
|---|---|---|
| `colour` | `black` | `black`, `white` or `#rrggbb`; `color` is read too |

The old scene fades out over the first half and the new one in over the
second. What shows between them is the slate, which is already at the bottom
of the canvas, full size and opaque. For black that is all there is to it. For
another colour the mixer tells the slate's `videotestsrc` to draw that colour
for the length of the dip and puts it back to black when the dip settles: a
property write on an element that is drawing a frame anyway.

### What a crop costs, and the query that had to be answered

A crop that changes size every frame renegotiates the slot's caps every frame.
Each renegotiation used to send an allocation query from the slot's flip to
the compositor, and the compositor answers a serialized query only once the
buffer queued ahead of it has been blended. The slot's thread waited a frame
or more per frame and fell behind: on the first wipe the incoming slot got
five buffers through in half a second, and the query sent as the crop settled
back never came back at all, so that slot drew nothing again. The software
compositor offers a sink pad no pool, so each slot's pad now answers the query
itself (video meta, no pool) with a probe that sees only queries. A GPU
compositor's pads are left alone, because their answer carries the context an
upload needs.

The same renegotiation has a second half on the compositor's output. Each time
the compositor takes a buffer with new caps it renegotiates its src pad, and
that sends an allocation query downstream, which waits at every `queue` until
the queue has pushed out what it holds: here, the encoder's backlog. A one
second wipe on a 320x180 debug core sent 30 of them, the longest 67 ms on a
quiet machine and 113 ms under load, and the compositor made no frame and
took no buffer from any slot while each one waited. The output caps never
change, so the answer never does: the compositor's src pad now keeps the first
answer for its caps and gives it again (`mixer::allocation`), and a RECONFIGURE
from downstream, which is an output being attached or taken away, makes it ask
again. An answer that offers a pool object is never kept, so a GPU path asks
every time as before. The test
`a_wipe_does_not_send_the_encoder_an_allocation_query_every_frame` counts what
reaches the encoder: one query, or none.

A trimmed pad is told to fill its box for the window, because the box is cut
to the shape of the trimmed picture; on the item's own policy a crop that
arrived a frame after its box letterboxed it. The next apply puts the item's
policy back.

### A binding that is reused

`GstDirectControlBinding` remembers the last value it wrote and writes again
only when the curve gives a different one. A binding reused by a later
transition still remembers where the last one ended, while the property under
it has been written by hand since. A curve that holds that same value from the
first frame, a slide's incoming alpha at 1, was never written, and a slide
after a wipe drew nothing.

The first fix moved each curve's first point by a millionth. That held only
when the compositor's first sync in the window landed before the curve's
second point, 17 ms on, and a compositor running behind the clock is already
a frame or two into the window when the curves are bound. On a loaded machine
the incoming scene of a slide or a wipe was then missing for the whole window
and appeared when the transition settled. Now every bind makes the binding
forget the value it last wrote (it is reset to the `G_MAXDOUBLE` a new binding
starts with, while the binding is disabled), so the first sync writes whatever
the curve says wherever in the window it lands, even past the end.
`mixer::transition::tests_binding` syncs a reused binding late by hand and
checks the pad is drawn.

## Item transitions

An item's `enter` and `exit` (see [the scene document](scene-document.md)) are
played by the same code as the scene transitions, aimed at one pad:

| `type` | Enter | Exit |
|---|---|---|
| `cut` | on the next frame | gone on the next frame |
| `fade` | alpha from nothing | alpha to nothing |
| `slide` | from just past `edge` to its place | from its place to just past `edge` |
| `zoom` | from nothing to its size, about its own centre | to nothing about its centre |
| `wipe` | revealed from `edge` by its slot crop | trimmed away towards `edge` |

They play when the scene on air is applied again with the item shown or
hidden, which is what `scene.item.set {props: {visible}}` does to a scene on
air. An item hidden that way keeps its slot until its exit has finished and is
then hidden; an item shown that way is bound at nothing and rises from the
first frame of its entrance, so it is never drawn in its place first. A take
or a cut during either settles it where it stands, as a take during a scene
transition does.

With `on_take`, the item's entrance plays when a scene holding it is taken
and its exit when a scene holding it is taken away, in place of the scene's
transition for that item. A take that is a cut for the scene and has such an
item becomes a window as long as the longest of them, with everything else
changing on its first frame.

## `program.transitions`

Read scope. Every name a take accepts here, in the order a take resolves them:

```json
{"transitions": [
   {"name": "fade", "origin": "built-in", "type": "fade", "params": ["easing"]},
   {"name": "wipe", "origin": "built-in", "type": "wipe", "params": ["direction", "easing"]},
   {"name": "house", "origin": "collection", "type": "fade", "params": ["easing"], "duration_ms": 400},
   {"name": "light-leak", "origin": "fx", "type": "overlay", "params": ["cut_at_ms"], "duration_ms": 2000}],
 "easings": ["linear", "ease-in", "ease-out", "ease-in-out"],
 "directions": ["left", "right", "up", "down"],
 "item_transitions": ["cut", "fade", "slide", "zoom", "wipe"],
 "edges": ["left", "right", "top", "bottom"],
 "max_duration_ms": 10000,
 "default_duration_ms": 300}
```

A running plugin whose name is a built in one replaces it in the list, as it
does in a take. An item in the fx library that takes may use is listed with
origin `fx` and its `type` is its look (`stinger`, `overlay`, `matte`,
`shader`); see [transitions and effects from packs](fx.md). Over MCP it is `list_transitions`, found through
`search_tools` rather than in the hot list.

## `program.take`

```json
{"method": "program.take",
 "params": {"scene": "wide",
            "transition": {"type": "fade", "duration_ms": 300}}}
```

`transition` takes a name or an object. The two are the same call:

| Spelling | Means |
|---|---|
| `"fade"` | `{"type": "fade", "duration_ms": 300}` |
| `{"type": "fade"}` | the same |
| `{"type": "fade", "duration_ms": 800}` | 800 milliseconds |
| absent | the scene's transition, then the default, when `fx.assign` set one; otherwise a cut |
| `"cut"`, or any `duration_ms: 0` | a cut, whatever is assigned |

`params` is the transition's own: a stinger reads `clip`, `cut_at_ms` and
`luma`, a wipe `direction`, a dip `colour`, every built in one `easing`; a
plugin reads whatever it documents. The three fields are the three a
collection stores a named transition under, so a transition written into a
scene collection and one typed into a call are the same thing.

A name a collection knows is resolved first, then a transition plugin the
supervisor has running, then an item in the fx library, then the built in
ones; a plugin named like a built in
one (`plugins/wipe` is called `wipe`) is the one that runs. A name nobody knows
is `-32602`, with `data.transitions` listing every name this core would have
accepted. A built in transition's params are checked the same way: a
direction, an easing or a colour this build does not have is `-32602` with
`data.directions`, `data.easings` or `data.colours`, and a zoom point outside
0 to 1 with `data.range`.

**Ceiling.** Ten seconds. Both scenes are on the canvas for the whole of a
transition.

**Taking over.** A take during a transition ends it where it stands: the
bindings come off, each property is left at the value its curve had reached,
and the new take owns the pads. Nothing queues.

## `event/program.took`

```json
{"source": null, "scene": "wide", "transition": "fade", "duration_ms": 300,
 "transition_id": 12, "at_running_time_ms": 1800004}
```

`transition_id` is the take's own number. Two takes in quick succession are
told apart by it rather than by timing, and it is the same number the core uses
internally to decide that an older transition has been superseded.

## A transition plugin

A `transition` provide has no media contract at all: it opens no socket, starts
no pipeline, and is never asked to `start`. Its instance is a singleton named
after its provide, started with the core by the supervisor, and it sits in
`ready` for its whole life. One method:

```
 core -> plugin  {"method": "render", "params": {
                   "from": ["sink_0", "sink_1"],
                   "to": ["sink_4", "sink_5"],
                   "progress": 0.5,
                   "running_time_ns": 1800000000}}

 plugin -> core  {"result": {"pads": {
                   "sink_4": {"xpos": -960, "alpha": 1.0},
                   "sink_0": {"xpos": 960, "alpha": 1.0}}}}
```

* `from` are the pads drawing the scene that is going away, `to` the pads
  drawing the one arriving. They differ even when the same camera is in both
  scenes, because the incoming scene is bound to slots of its own.
* `progress` runs 0 to 1 over the transition.
* `running_time_ns` is when that frame will be composited, on the compositor's
  own timeline.

A plugin may instead answer once with a curve:

```json
{"curve": [[0.0, 0.0], [0.4, 0.1], [1.0, 1.0]]}
```

which is a shape rather than a set of pad values: the core applies it as a
crossfade eased by that curve. Use it when the transition has nothing to say
about geometry.

### When `render` is called

Before the transition starts, once per frame of it, up to 64 times. Not during.

That is the whole trick, and it is why a plugin is as accurate as a built in.
The plugin sees exactly the progress and running time it would have seen frame
by frame, and what it answers becomes control points the aggregator reads on
the frame. A plugin polled during the transition would be a plugin the
compositor waits for.

The sampling has a budget of 200 milliseconds in total and 50 milliseconds per
call. A plugin slower than that has the rest of its curve drawn as a straight
line, and the log says so. A plugin that cannot be reached at all makes the
take a cut, because a take that is refused because the wipe would not answer is
the wrong trade for something going out live.

### The conformance check

`harness::check_transition` feeds a crossing of two pads out and two pads in
and asks at 0, 0.5 and 1. It checks the contract and not a shape, so a wipe, a
dissolve and a clock wipe all pass:

* `render` answers inside the deadline at all three points.
* The answer is `{pads}` or, once, `{curve}`.
* Every pad named was one the core offered. A plugin cannot invent a pad.
* Every property driven is one of the six above.
* Every value is a finite number.
* Progress 0 and progress 1 are not the same picture.

```bash
cargo build -p gmx-wipe
cargo test -p godwinmix-core --test transition_plugin -- --nocapture
```

## The fallback

A pad that will not take a control binding falls back to a thread writing
properties about sixty times a second. That happens on an element that does not
declare a property controllable, which the software `compositor` does not do
for any of the six, and which a GPU compositor might. It is less accurate by
exactly the jitter of a sleeping thread, which is why it is the fallback and
not the design.

The supervisor's 500 millisecond visibility tick, which reapplies the scene so
a stalled camera fades to the slate and comes back on its own, asks before it
writes: a property under a live binding is the transition's until it settles.

## Accuracy

The Phase 6 acceptance is that a crossfade lands on the scheduled running time
within one frame. Measured with a pad probe on the compositor's own src pad,
recording the running time of every output frame and the incoming pad's `alpha`
at the moment that frame was blended, and interpolating the crossing of 0.5
between the two frames that straddle it:

| Measure | Result |
|---|---|
| Where the fade was armed to be half done | 836.2 ms running time |
| Where it actually was | 836.3 ms |
| Error | 0.0 ms |
| One frame at 30 fps | 33.3 ms |

```bash
cargo test -p godwinmix-core -- --ignored --nocapture crossfade_lands
```

The measurement is interpolated rather than "the first frame at or past half",
which would be biased up by as much as a frame and would be measuring the frame
rate rather than the transition.

### Why the window is in the compositor's timeline and not the clock's

A live aggregator composes the frame for running time T and pushes it a latency
later, so the pipeline clock is ahead of the picture by that latency. A curve
written in the clock's timeline would start a second late on a pipeline with a
second of upstream latency. The mixer therefore reads where the compositor has
got to from a probe on its own src pad, adds one frame (the frame the probe saw
has already been blended), and starts the window there.

## What it costs

Each transition at 1280x720, against the same takes as cuts, is in
[the how to page](../how-to/transitions.md#what-it-costs): a slide and a push
cost under 4 percent more than a cut, a dip and a fade 18 and 29, a wipe 58,
a zoom 65 and a box 84, and none of them cost the programme a frame.

Both scenes are on the canvas for the length of the transition, so slot
pressure doubles and the pool grows if it has to. Measured on the software
compositor at 320x180, six 300 millisecond crossfades between two eight item
scenes:

| Measure | Result |
|---|---|
| Largest programme inter frame interval | 33.3 ms |
| One frame at 30 fps | 33.3 ms |
| Slots the pool grew to | 16 |

```bash
cargo test -p godwinmix-core -- --ignored --nocapture a_crossfade_between
```

## A transition as a WebAssembly component

A transition plugin can run inside the core as a sandboxed component instead of
as a process (tier W). It answers the same `render`, with the same two answers,
and the same rule applies: it is sampled before the window opens and never
during it. `examples/wasm-ease` is twenty lines and answers once with a curve.
See [plugins as WebAssembly components](wasm.md) and
[why WASM is not on the frame path](../explanation/why-wasm-is-not-on-the-frame-path.md).

## See also

* [change scene with a transition](../how-to/transitions.md)
* [scene commands](scene-commands.md), including `program.take`
* [plugin lifecycle](plugin-lifecycle.md), for how a transition plugin is
  started and supervised
* [how a scene reaches the compositor](../explanation/how-a-scene-reaches-the-compositor.md)
