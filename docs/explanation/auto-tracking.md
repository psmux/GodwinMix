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
frame changes at once, because the lights changed or the camera moved, it
starts its background again rather than reporting the whole frame.

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

## Detection does not have to run on every frame

The tracker can move every track along its velocity on frames the detector
did not see. `FollowConfig::detect_every` runs the detector on one frame in
N, and the follower coasts on the others. At 25 frames a second, detecting on
every third frame is still eight detections a second, which is plenty for a
person walking, and costs a third.

## What it costs

Measured with the criterion bench in the crate, on one core of an AMD Ryzen 9
9950X3D. The crate README has the table. The whole chain with YOLOX nano on a
320 by 180 frame is about 5 ms, and under 2 ms detecting every third frame.
The motion chain is about 50 microseconds. The tracker, chooser, framing and
steering together are under a microsecond for five people. A Raspberry Pi 4
core is roughly ten times slower, which puts the model chain near 50 ms, so a
Pi runs the model every third frame or the motion backend.

## What it does not do yet

Keypoints are read when a detector gives them, but no backend gives them yet:
MoveNet Lightning is the planned one. The tracker does not know the camera is
moving, so a PTZ move shows up as everybody's velocity; the motion backend
resets on large moves, and the model backend does not mind. There is no
platform backend (Apple Vision, Windows ML) yet; one would sit behind a
feature, never the default.
