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
4. **Filter.** An item carrying filters gets them on its own slot's chain,
   between the flip and the compositor pad, and only if the chain it wants is
   different from the chain it has. See below.
5. **Hide the rest.** Every slot nobody claimed goes to alpha 0.

## Per item filters

A filter belongs to the item, not to the source. That is the whole difference
from `filter.add`, which puts one on a source and therefore on every picture of
it: with per item filters the same camera drawn twice can be keyed in one place
and clean in the other, which is what the reference lower third and the
picture in picture both want.

```text
  pgm-vtee-{source} =|=> gate > q > crop > flip > [chroma] > vmix pad
```

`scene.item.filter.add`, `.set` and `.remove` write the filter onto the item in
the document; the slot pool puts it on the chain the next time the scene is
applied, which for a scene that is on air is straight away, under
`with_pad_blocked` on that slot's own queue. The programme keeps aggregating
its other pads throughout.

Two rules make that cheap enough to do on a running programme:

* **The chain is compared before it is touched.** The visibility tick reapplies
  the scene twice a second; a chain that has not changed takes no pad block. A
  filter renamed is the same filter, because what is compared is the type and
  the parameters. A chain whose filters are the same types in the same order
  with new parameters is changed in place through each filter's configure,
  with no pad block either: that is a key's slider dragged on air.
* **A chroma key does not feed the pad.** Its result has alpha and the
  compositor's I420 has none, so the key hands the camera frame and its matte
  to the overlay board, which draws it after the compositor in stacking order
  with the other transparent items, and sends the pad an empty gap buffer per
  frame. A gap and not nothing: a pad that has had buffers and then gets none
  is waited for, a second of programme at a time. See
  [the chroma key](../reference/chroma-key.md).
* **A disabled filter is not in the chain at all.** Turning one off costs one
  pad block and then nothing, rather than an element seeing every frame and
  deciding not to act on it.

A slot rebound to a different source loses its chain, because the chain
belonged to the item that asked for it and a chroma key must not follow a slot
onto the next camera.

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

## The one thing flattening cannot express

A filter over a composited group: a blur across three items at once, which is
not three blurs. The filter has to see the group as one picture, so the group
gets a compositor of its own:

```text
 cam-a tee =|=> gate > q > crop > flip > sub pad 0 \
 cam-b tee =|=> gate > q > crop > flip > sub pad 1  >-- sub compositor --> caps
 cam-c tee =|=> gate > q > crop > flip > sub pad 2 /                        |
                                                                           v
                  programme slot:  gate > q > crop > flip > [blur] > vmix pad
```

This is the expensive path, and it is the one place on the scene path that adds
elements to a running programme on purpose. It is built when a scene has a
group carrying an enabled filter, and taken apart the moment it does not: turn
the filter off and the group goes back to being flattened, with no element
anywhere. A scene with no filtered group never goes near it, and the check for
one is a walk of the tree that answers no for every scene anybody has built.

What it costs, from 11 section 3: a full canvas opaque pad through a second
compositor is 0.14 ms a frame. The sub compositor's output stays I420, because
AYUV is 3.5 times the price; a group that needs alpha through the whole chain
is not something this build offers, and this page says so rather than the
picture quietly costing four times what the budget allows.

The children are drawn at their canvas coordinates, so the composite is the
picture the group would have made and the filter sees what an operator sees.
Moving the group moves them on that surface exactly as it would on the canvas.

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

## Moving a slot to another source

A slot that changes hands is flushed first: the outgoing source can have a
frame parked in it, and the flush is what lets that go. Three things about how
it is flushed matter, and each one was learned the hard way on 2026-10-05,
with two phones joining a show that already had more sources than slots.

The flush is pushed out of the valve's own src pad, not sent into the queue
under it. A flush takes the segment off every pad it passes. Sent into the
queue, it went around the valve, and the valve still believed the queue had
the segment. When the same source came back to the slot it had just lost, it
brought the identical segment, the valve did not send it again, and the next
frame reached the compositor with no segment at all. `compositor` asserts on
that in `gst_video_aggregator_fill_queues` and the whole process aborts.
Adding a phone to a full pool did exactly this: the new source took the slot
on air, and the next apply gave it straight back.

The valve is shut before the flush. A frame the source's tee pushes into a
flushing slot comes back `FLUSHING`; a tee with one pad hands that up to the
source's queue, which pauses on it and never starts again, so the source stays
black everywhere it is drawn while its own pipeline reads as live. A valve that
is dropping answers `OK` whatever happened below it.

A source being added never takes a slot that is on the canvas. With the pool
full it takes one that is hidden, and if every one is showing the pool grows.

Behind all three, each slot's queue checks that a frame has a segment in
front of it, sends the valve's one down first if it has not, and drops the
frame if there is none to send (`mixer::slot_guard`). It should never fire; a
warning naming the slot says it did.

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

## Preview: the armed scene, beside the mosaic

An operator arms a scene before taking it and wants to see it, and the picture
must not come from the programme pipeline: a preview problem reaching air is
the third isolation boundary the README promises. It does not need its own
pipeline either, because the pictures are already there once, as the mosaic's
per source thumbnails.

So the preview is a second `compositor` inside the multiview pipeline, fed from
the same tile branches:

```text
 source thumb ==> proxysrc > q > rate > scale > caps > tee =|=> mosaic pad
                                                            |
                                                            +=> q > preview pad
                                                                      |
 preview compositor  <------------------------------------------------+
      |
      +--> caps --> jpegenc --> appsink   (/mjpeg/preview, scene.preview.frame)
      +--> its own tile on the mosaic
```

Every tile now ends at a `tee` with `allow-not-linked`, which costs no thread
and lets the preview take the same picture without a second decode, a second
scale or a second proxy.

Nothing exists until somebody asks. A client subscribing with `ext.preview`, or
opening `/mjpeg/preview`, or calling `scene.preview.frame`, holds a preview
subscription; the last one to let go takes the compositor, its slots and its
tile away. Arming a scene with nobody watching costs one message and a `Vec`.

`ext.preview = "full"` composites at the canvas's own size instead of the
mosaic's, so a designer's handles and a projector land on real coordinates. The
pictures in it are still the thumbnail ends: copying every source into a second
pipeline at canvas size is exactly the cost the mosaic exists to avoid, and a
full detail look at one source is a projector on that source.

Measured: the preview's whole branch taken to NULL with buffers still arriving
at its pads left the programme's largest inter frame interval at 33.3 ms, which
is one frame.

```bash
cargo test -p godwinmix-core -- --ignored --nocapture killing_the_preview
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
accurate way, and are what transitions between scenes use: see
[transitions](../reference/transitions.md). The thread is still here for a
geometry command on a scene that is on air, where the pads are already drawing
these items and there is no second scene to cross to, and as the fallback for
an element whose pads will not take a binding.

Measured: `pip-bottom-right` reapplied onto the same scene with a bigger inset
over 300 ms moves the inset through the middle (a cut would already be at the
target) with a largest inter frame interval of 33.3 ms, which is one frame, and
zero relinks.

## Transparent items

A text, a ticker, a PNG or SVG with alpha and a clip with alpha are not drawn
by the compositor at all, and the reason is the pinned output. The software
compositor's output is I420, which has no alpha plane, and a `compositor`
whose output has none refuses every input that has one: linking BGRA into it
fails with "can't handle caps" on GStreamer 1.28.7. Converting the picture to
I420 first keeps it linkable and flattens the clear parts to black, which is
the black box this section exists to prevent. Compositing the whole programme
in AYUV instead costs 2.6 times the CPU at 1080p30 (`mixer::programme_keeps_alpha`).

So the overlay board draws them, after the compositor:

```text
 sources =|=> slots => vmix --[board: blend each layer, in place]--> vmix-caps => encoder
                       ^ a transparent item's own pad: placed by the scene, fed nothing
```

A transparent source keeps its slot like any other item. The scene binds it,
writes `xpos`, `ypos`, `width`, `height`, `alpha`, `zorder` and the sizing
policy, and a transition drives the same properties. What the source sends
through the ordinary path is a carrier, its picture flattened on grey at the
canvas size, one frame held by `imagefreeze`, which keeps the supervisor, the
tile and the thumbnail working; the head of its programme branch drops it, so
the compositor pad never has a buffer and is skipped. A probe on the
compositor's src pad reads each such pad's properties back every frame and
blends the source's held AYUV picture into the I420 frame there.

The blend is the board's own, in `overlay/blend.rs`: a transparent pixel is
one comparison, an opaque one a copy, and only the edges are mixed, so what a
layer costs is proportional to the area it covers that is not clear. The
probe is installed with the first transparent source and taken off with the
last, so a show with none runs the graph it always ran.

What this cannot do: a transparent item is drawn over every opaque item,
whatever its place in the scene's stack, because the opaque ones are already
inside the frame the board draws on. Among transparent items the stack order
holds. Crop and rotation on the item are not applied, and an item inside a
group that carries a filter is not drawn, because its pad belongs to the
group's compositor rather than the programme's. On a GPU graphics entry the
board does not draw and the carrier goes through the compositor, drawn flat.

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

## What a transition adds

A transition needs both scenes on the canvas at once, which is the one thing
the ordinary path does not do. The slot pool holds the outgoing scene's slots
out of the pool for the length of the crossing and binds the incoming scene to
different ones, so slot pressure doubles and the pool grows if it has to. Every
incoming pad arrives at alpha 0, so the frame between binding and the first
sync is never the wrong picture, and the curves take it from there.

The curves are read by the compositor at the running time of each frame it
makes, so a compositor that is behind the clock still plays the whole
crossing, frame by frame, as it catches up. What it cannot do is go back: a
frame it made before the curves were bound shows the old scene whatever the
curves say. The window starts on the frame after the last one the compositor
pushed, and on a loaded machine the mixer thread can be held off long enough
that the compositor is already past that frame, or past the whole window,
when the curves go on; measured once, a 300 ms wipe drew nothing and the new
scene appeared 1166 ms in. So once the curves are bound the start is checked
again, and if the compositor got there first every curve moves on by the
frames it missed and is bound again (`Mixer::bind_in_time`). A transition on a
starved machine starts a little late and then plays whole.
Each pad property keeps one control binding for good, and a binding writes
only when its value changes, so every new crossing makes the binding forget
what it last wrote. Without that, an incoming pad whose alpha was 1 at the end
of the last crossing, hidden by hand since, was never written on a late
compositor and the new scene appeared only when the transition settled. A pad
that has no frame yet is not drawn at all: the outgoing scene plays its half
of the crossing over whatever is behind it, and the new scene appears on its
first frame, which is a cut.

A wipe and a box trim the incoming picture with the slot's own crop, so that
slot's caps change on every frame of the crossing, and the compositor
renegotiates its output each time. Renegotiating asks downstream how to
allocate, and that question waits behind the encoder's queue. It used to be
asked on every frame of a wipe, with the compositor stopped while it waited;
on a loaded machine that was the incoming slot passing nothing for half a
second. The answer cannot change while the output caps do not, so the
compositor's src pad keeps it and answers the repeats itself
(`mixer::allocation`). A wipe now costs one such question, or none, rather
than one per frame.

Measured: six 300 ms crossfades between two eight item scenes left the
programme's largest inter frame interval at 33.3 ms, one frame, with the pool
grown to sixteen.

## Where the document lives

The compositor never sees a scene document. The scene server
(`scene::server`) owns it, and what it hands the mixer is a `Vec<Placement>`:
plain numbers in canvas pixels with no groups, no references and no names. That
is why the server's own tests run in milliseconds with no pipeline, and why the
mixer's tests run against real sources with no document.
