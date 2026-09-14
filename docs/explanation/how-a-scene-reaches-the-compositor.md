# How a scene reaches the compositor

A take has to be free. The encoder starts once and runs until the broadcast
ends, and everything that changes during a show happens upstream of it, in raw
video, where switching is a property change on a compositor pad. That is what
makes a cut invisible to the RTMP connection, and it is the property scenes had
to be built without spending.

Putting eight things on the canvas instead of one could easily have cost it.
Building a bin per item and linking it in would mean adding elements to a
running pipeline on every take, which is the documented hazard: the stall of
2026-09-12 was live pad addition. So the design spends pads instead of
elements.

## The shape

```
 collection (document)                        programme pipeline
 +-------------------------------+            +------------------------------------------+
 | scene "wide + lower third"    |  flatten   |  slot pool (8, grows on demand)           |
 |   item stage    -> cam-wide   | ---------> |  slot 0: gate>q>crop>flip>vmix pad 0      | <- cam-wide tee
 |   item corner (group)         |  Vec<      |  slot 1: gate>q>crop>flip>vmix pad 1      | <- cam-pulpit tee
 |     item pulpit -> cam-pulpit |  Placement |  slot 2: ...                    alpha 0   |
 |   item lower third -> ref     |  >         |  slot 3: ...                    alpha 0   |
 +-------------------------------+            |                                          |
                                              |  z: 0 slate | 1..99 retired, held         |
                                              |     1000+ live items                     |
                                              +------------------------------------------+
```

Each source's programme branch ends at a `tee` with `allow-not-linked` rather
than at a compositor pad, so one source can be on the canvas twice: a wide shot
and a cut out of the same camera. Audio does not follow: there is still one
`audiomixer` pad per source, because a source heard twice is not louder.

Applying a scene is four steps and none of them touches the graph:

1. **Flatten.** `scene::geometry::flatten` composes every group's transform into
   its children and multiplies its opacity into their alphas, references resolve
   into the items they stand for, and invisible items drop out. What comes back
   is a flat list, bottom of the stack first.
2. **Bind.** Each entry takes the slot that already holds its source.
3. **Write.** `xpos`, `ypos`, `width`, `height`, `alpha`, `zorder`, the sizing
   policy from `fit`, the alignment from `align`, the crop, the rotation.
4. **Hide the rest.** Every slot nobody claimed goes to alpha 0.

## Why binding happens when a source is added

A slot is bound to a source the moment the source is added, not the moment it is
taken. By the time anybody asks for a scene the links are already there, so a
take is property writes and nothing else, and `program.take {source: "cam1"}`
costs exactly what it cost before scenes existed. Binding later would make the
first take of every source a relink.

A relink is only needed when a scene wants a source in more places than it has
slots for. That happens under `with_pad_blocked`, on that source's own queue,
not on the programme's path: the compositor keeps aggregating its other pads and
`force-live` keeps the output on schedule, so what the block costs is that
source's own frames and nothing downstream notices.

The pool starts at eight because a hidden pad is free and there is no reason to
start smaller. It grows on demand, which is the one path that requests a
compositor pad while the programme is running, and it stays rare by
construction.

## Why groups are flattened

A group here is an item whose content is a list of children. No special type, no
special rules, and nothing below the flatten step knows one exists.

OBS made a group a modified scene and collected a decade of transform corruption
bugs: issues 2913, 4173, 5478, 9297, 9298, 9558. The shape of all of them is the
same. A group has a box of its own, the box is written back into the children
when the group moves, and the child's own position is lost or doubled. Once the
arithmetic lives in two places, one of them is eventually wrong.

Doing it once, at apply time, removes the class. Moving a group by 100 px moves
every child by 100 px on the canvas and changes no child's stored transform;
ungrouping multiplies the group's transform into each child with the same
function the compositor path uses, so it is invisible. Both are tests.

A nested scene is flattened the same way: a reference becomes a group whose
children are the target's items, with sparse overrides keyed by the target's
item id, so a target that gains an item does not silently move every override
onto the wrong entry. Cycles are refused at edit time and stop at a depth limit
if one reaches the store by hand.

The one thing flattening cannot express is a filter over a composited group, a
blur across three items at once. That needs a sub compositor built on demand and
is the expensive path; it is not in this release.

## The valve, and what it is worth

A compositor pad at alpha 0 costs nothing: the element skips conversion, scale
and blend for it. The chain feeding that pad is another matter. A queue has a
thread, and a `videocrop` and a `videoflip` see every frame whether anyone is
looking or not.

Measured here, on the software compositor at 320x180 with one `test://` source:
sixteen slots bound to that source and hidden cost **180 percent** over the one
slot baseline. The pads were free; the sixteen queue threads were not.

So each slot starts with a `valve`. Hidden, it drops the buffer where the tee
hands it over, before any of that. The same sixteen then measure **-5.85
percent** against the baseline, which is inside the noise and well inside the 2
percent the budget allows.

One slot per source keeps its valve open whatever the scene says: the slot it
was given when it was added. That is what makes the ordinary path cost exactly
what it did before the pool existed, and what makes a take show the current
picture rather than a frame from whenever the valve last closed.

## z bands

| Band | What is in it |
|---|---|
| 0 | the slate, permanently, fully opaque |
| 1 to 99 | the branch of a source being rebuilt, holding its last frame |
| 1000 and up | the live items of the scene, bottom of the stack first |

The slate at the bottom is why losing a source reveals black rather than
freezing. The band above it is the freeze frame: when a source's pipeline is
stopped for a rebuild, its slot keeps its pad and the compositor keeps drawing
the last buffer that came through it, which costs no element, no copy and no
code path that only runs during a fault. It sits under the live band so whatever
is taken next draws straight over it, and `release_retired` puts the slot back
in the pool when the hold ends.

## What a take actually costs

Measured on this machine (Apple Silicon, GStreamer 1.28), ten takes back and
forth between two eight item scenes, with the interval between consecutive
frames watched on the encoder's own sink pad:

| Measure | Result |
|---|---|
| Largest inter frame interval | 33.3 ms |
| One frame at 30 fps | 33.3 ms |
| Relinks during the ten takes | 0 |

A frame that never arrived shows as an interval of two frame durations, so a
maximum of exactly one frame means nothing was lost. Both numbers come from
`#[ignore]` tests in `mixer.rs` that print them:

```
cargo test -p godwinmix-core -- --ignored --nocapture gapless
cargo test -p godwinmix-core -- --ignored --nocapture hidden_slots
```

## Moving rather than cutting

A geometry command with a `duration_ms` on a scene that is on air eases the
pads instead of jumping them. It works because the layout kept the item ids: the
pads are already drawing these items, so there is a "from" to ramp from. Off
air it is a cut whatever the duration says, because there is nothing being
drawn.

The ramp is a short lived thread writing pad properties about sixty times a
second, guarded by the take generation so a newer take abandons it rather than
fighting it. That is the shape `ramp_volumes` already had for a take's audio
fade, and for the same two reasons: it cannot run on the mixer thread, which
has commands to answer, and it must not run on a streaming thread, which is
carrying the programme.

`GstInterpolationControlSource` bindings sampled by the aggregator are the
accurate way, and are what transitions between scenes will want. They need a
crate this build does not carry, and at sixty steps a second the difference is
not visible; the swap is one function.

Measured: `pip-bottom-right` reapplied onto the same scene with a bigger inset
over 300 ms moves the inset through the middle (a cut would already be at the
target) with a largest inter frame interval of 33.3 ms, which is one frame, and
zero relinks.

## What the compositor cannot do, and what is said instead

`compositor` has three sizing policies, so the seven `fit` keywords map onto
them: `none` stays `none`, `contain` and `max` are `keep-aspect-ratio`, and
`stretch`, `cover`, `fit-width` and `fit-height` are `scale` with the item's own
crop taking the overflow. The reference page says which is which rather than the
picture quietly being wrong.

`videoflip` turns by quarters and nothing else, so an item asking for 37 degrees
gets 0. Arbitrary rotation needs `gltransformation` and the frame on the GPU,
which is the graphics catalogue's job.

Scaling stays on the compositor pad. Moving it to the input side would make a
take a caps renegotiation across the proxy boundary, which is the traffic
`answer_negotiation_here` exists to stop.

## Where the document lives

The compositor never sees a scene document. The scene server
(`scene::server`) owns it, and what it hands the mixer is a `Vec<Placement>`:
plain numbers in canvas pixels with no groups, no references and no names. That
is why the server's own tests run in milliseconds with no pipeline, and why the
mixer's tests run against real sources with no document.
