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

Nothing else. `zorder` is deliberately not on the list: a transition that could
reorder the canvas could put an item somewhere the scene document does not
say it is.

## The built in four

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
cheap half of the AYUV price 11 section 3 puts at 3.5 times I420.

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
| absent, `"cut"`, or any `duration_ms: 0` | a cut |

`params` is the transition's own: a stinger reads `clip`, `cut_at_ms` and
`luma`; a plugin reads whatever it documents. The three fields are the three a
collection stores a named transition under, so a transition written into a
scene collection and one typed into a call are the same thing.

A name a collection knows is resolved first, then the built in four, then the
transition plugins the supervisor has running. A name nobody knows is
`-32602`, with `data.transitions` listing every name this core would have
accepted.

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

## See also

* [change scene with a transition](../how-to/transitions.md)
* [scene commands](scene-commands.md), including `program.take`
* [plugin lifecycle](plugin-lifecycle.md), for how a transition plugin is
  started and supervised
* [how a scene reaches the compositor](../explanation/how-a-scene-reaches-the-compositor.md)
