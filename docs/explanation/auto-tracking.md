# How auto tracking works

A camera that follows the person speaking has to answer five questions on
every frame: where are the people, which of them is which, who should the
shot be on, what shot is that, and how should the camera move to get there.
The crate `godwinmix-track` answers each one in its own layer, and each layer
can be used without the others. The PTZ follow loop uses all five. A digital
crop that makes a virtual camera from one wide 4K shot uses all five too, but
takes the crop instead of the speeds. The director only needs the first two
to know who is on stage, the replay only the first to find a moment with
somebody in it, and an agent asking what a camera sees reads the tracks.

The surface is in [the reference](../reference/track.md). This page says why
it is built the way it is.

## What it had to promise

1. It runs on a laptop with no GPU, on Windows, Linux and macOS, and on a
   Raspberry Pi. So it is pure Rust, with no native runtime to ship.
2. It costs under 15 ms a frame at 320 pixels wide on one core, and nothing
   allocates per frame once it is warm. The follow loop lives next to the
   mixer, which must never stall.
3. It takes frames as they already are. A frame bus slot is NV12, a decoded
   JPEG is RGB, a preview is BGRA. Copying to one format first would cost
   more than the tracking does.
4. Nothing in it is tied to one camera, one protocol or the PTZ use.

The first two rule out the obvious tools. OpenCV and ONNX Runtime are native
libraries with their own builds per platform; the cutout already ships ONNX
Runtime, and is the reason it is not the default here. An async runtime would
tie the crate to one kind of host. So the crate depends on the standard
library alone, and its one optional backend on `tract`, which runs ONNX
models in pure Rust.

## The layers

**Detect.** A `Detector` turns a `FrameRef` into boxes with a score. There are
two backends. The `tract` backend runs YOLOX nano, a small person detector
from Megvii under the Apache 2.0 licence, fetched on first use and checked by
its SHA256. Ultralytics YOLO models are better known and were ruled out: their
AGPL licence does not fit an Apache project. The `motion` backend has no model
at all. It scales the frame down to grey, keeps a slowly learned background,
and joins the pixels that differ from it into boxes. It cannot tell a person
from a door, and a person who stands still fades into the background after
some seconds, but it costs 50 microseconds, needs nothing downloaded, and is
right for a fixed wide camera and while the model loads. When most of the
frame changes at once, because the lights changed, it starts its background
again rather than reporting the whole frame. When it is told the camera is
moving it rests instead; see "A camera that moves" below.

The PTZ plugin runs YOLOX by default and the motion backend in its place
while the model downloads or loads, and for good if it cannot be had, so a
follow starts on the first picture rather than after the download. Which one
runs, and why, is in `track_state`.

**Track.** Detections are per frame and have no names. The tracker follows
the ByteTrack method. Each track has a Kalman filter that knows its position
and velocity, so between frames it knows where to look. Detections are
matched to tracks by how much their boxes overlap, solved for the whole frame
at once with the Hungarian method rather than greedily, so two people who
cross do not swap. Confident detections are matched first; weak ones are then
allowed to keep an existing track alive but never to start one, which is what
holds a track through a frame where the detector was unsure. A track needs two
matches before it is trusted, so one false detection never becomes a person.

The tracker also keeps a small colour histogram of each person's clothes.
Someone who walks behind the pulpit is lost to the filter; when they come out
somewhere it did not predict, the overlap test fails, and the histogram is
what gives them their old id back instead of a new one. Ids are slugs,
`person-3`, and never reused.

**Choose.** Who to follow: a locked id, the largest person (usually the
nearest), the most central, the one nearest a point the operator tapped, or
the one nearest where the audio says the voice is. The choice sticks: someone
else has to score half again as well before it moves, so the shot does not
flick between two people standing together. When the target vanishes the
shot holds where they were for two seconds, then falls back to staying put,
going wide, or a preset.

**Frame.** The person's box becomes the rectangle the camera should show.
`close` is head and shoulders, `waist` is head to waist, `full` the whole
body, `wide` the person and space around them. Without keypoints the cut lines
are fractions of the body height. A box much wider than a standing person is
somebody cut off by the furniture, so their height is guessed from their
width. There is headroom above the head and lead room ahead of a moving
person, so they walk into the shot rather than out of it.

**Steer.** The same rectangle drives two outputs. For a PTZ camera, a
controller per axis turns the offset from the centre into a speed: in
proportion to the error, damped by its rate of change, limited in speed and
in acceleration so moves ease in and out. Below a dead zone an axis stays
still; once moving it carries on until the person is well inside the dead
zone, and then holds for a moment before it may move again. That is what
keeps a camera from hunting after somebody shifting on their feet. For a
digital crop, the centre and the size follow the target on a critically
damped spring, which arrives without overshoot.

## A camera that moves

The picture a PTZ camera sends is taken from a camera that the follower
itself is turning. Left alone, that is three problems. The tracker's filters
read a pan right as every person walking left, and learn that velocity; a
person who leaves the frame on the pan is predicted to keep walking, so when
the camera turns back they are somewhere else and come back as a new id. The
lead room, which leans the shot towards where a person is walking, leans it
by the camera's own speed. And the motion backend sees the whole background
slide: one box the size of the picture, which the tracker takes for a very
large person.

The follower knows what speed it last sent the camera, and the plugin knows
what the camera is really doing (the joystick wins over the follower). With
the camera's optics, its field of view and its top pan, tilt and zoom
speeds, that speed becomes the motion of the picture itself: panning right
at 30 degrees a second with a 60 degree view moves everything in the picture
left by half its width a second. That is the `EgoMotion`. The tracker
carries every box with it in each prediction, before matching, so the
filters only ever see what people do themselves; a lost person is carried
too, and is waited for where they really are. The motion backend is told the
same thing and rests while the picture moves faster than a fiftieth of a
frame a second, then learns the new view on the first still frame and finds
what moves again from the next one. The model backend looks at each frame
alone and needs nothing.

The optics need not be exact. A field of view a quarter off still takes
three quarters of the motion out, which is the difference between a track
that holds and one that does not. The field of view changes with the zoom,
so the follower keeps its own count of how far it has zoomed since the
follow started and scales the pan by it; the configured field of view is
the one at the start. What the follower cannot know about is a move it did
not ask for at a speed: a `goto` or a preset recall. A recall turns tracking
off, and a preset fallback starts the tracker afresh on the new picture.

The closed loop test in `tests/ego.rs` shows it: three people, one walking
past another and back, a simulated camera that pans and zooms as told. With
the optics, one target id throughout, three ids for three people including
the one the pan takes out of view and back, people standing still read as
still, and the camera starts twice for two walks. Without them, the same
people standing still read as walking at the camera's speed.

## Detection does not have to run on every frame

The tracker can move every track along its velocity on frames the detector
did not see. `FollowConfig::detect_every` runs the detector on one frame in
N, and the follower coasts on the others. At 25 frames a second, detecting on
every third frame is still eight detections a second, which is plenty for a
person walking, and costs a third.

## What it costs

Measured with the bench in the crate, on one core of an AMD Ryzen 9
9950X3D. The crate README has the table. The whole chain with YOLOX nano on a
320 pixel wide frame is about 7 ms, and about 2.3 ms detecting every third
frame.
The motion chain is about 50 microseconds. The tracker, chooser, framing and
steering together are under a microsecond for five people. A Raspberry Pi 4
core is roughly ten times slower, which puts the model chain near 50 ms, so a
Pi runs the model every third frame or the motion backend.

## Keypoints

Boxes say where a person is, not how they stand. MoveNet Lightning, a pose
model from Google under Apache 2.0, gives 17 body points from a square crop
around a box, and the framing uses them to put the cut at the real waist or
shoulders rather than at a fraction of the box. It costs about 12.5 ms a
person on one core, more than the box detector, so it runs on one detection
in three and the tracker moves each person's last keypoints with their box
in between. That keeps the chain near 12 ms a frame.

## What it does not do yet

The camera's motion comes from the speed it was told and fixed optics, not
from the picture itself; a camera whose top speeds or field of view are far
from what it was given is only partly corrected, and a move made without a
speed (a `goto`) is not corrected at all. There is no platform backend
(Apple Vision, Windows ML) yet; one would sit behind a feature, never the
default.
