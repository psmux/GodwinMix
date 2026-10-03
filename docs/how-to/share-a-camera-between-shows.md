# Share a camera between shows

You run two shows on one machine, say the Sunday service and a separate feed
for the overflow room, and both want the same camera. Add it to both. There is
nothing else to do: the camera is opened once, by whichever show asked for it
first, and the other show reads the same picture from it.

Shows themselves arrive separately, in the station work; until then the same
holds for two mixers started on one machine, which is how it was tested.

## Add the camera in each show

In each show, add the camera from **Add a source** and pick it by name, the way
[Use a webcam](use-a-webcam.md) describes. What makes two sources the same
camera is the device you picked, not the name you give the source, so one show
can call it Pulpit and the other Wide.

The first show to add it opens the camera. The second one does not open
anything; its source reads the frames the first one is already decoding. Most
operating systems will not let two programs open one camera at once anyway,
and on the ones that do, the second decode would cost as much as the first.

The same goes for:

* a screen capture: two sources capturing the same monitor, with the same
  region and cursor setting, share one capture;
* a stream on a channel: two shows taking the same channel stream decode it
  once, pictures and sound together.

A microphone or another sound device is not shared. Every desktop system lets
several programs open one at the same time, it costs almost nothing to open,
and each show keeps its own trim and mute.

## What decides the picture

The show that opened the camera ran it with its own settings, so its
resolution and frame rate are the ones everybody gets. The other show scales
that picture to its own canvas. If you need a different size in each show,
set the size on the show that opens the camera first, or give each show its
own camera.

Everything after the camera is still each show's own: crop, scale, filters,
where it sits in a scene, and the fader.

## When a show stops

If the show that opened the camera stops, or crashes, the other show takes the
camera over by itself. The picture holds its last frame for a moment and then
carries on. Measured on a MacBook Pro with its built in camera, the gap was
0.4 to 0.7 s, depending on how busy the machine was: the time to start the
camera again, not a restart of the source, and well inside the two seconds
after which a source counts as stalled. A channel stream comes back when its
next keyframe arrives, so the gap there is up to one keyframe interval longer.

## What it saves

Measured with two mixers both showing the MacBook Pro camera at 1080p30, the
two together used 10.3% of one core with the camera shared against 14.9% with
each opening it, and one camera process instead of two. A 720p30 channel
stream read by two shows cost the second show 2.5% of one core instead of
11.9%. Reading the shared picture adds about a fifth of a millisecond.
[The frame bus](../explanation/frame-bus.md) has the numbers and how they were
taken.

## Where it does not happen

* On Windows every show opens its own camera, as before. The frame bus has no
  Windows transport yet. Two shows asking for one camera there will find the
  second one cannot open it on most machines. Inside one show the mixer says
  so straight away: adding a camera that is already a source is refused with
  the id of the source that has it (`data.source`), and the way to show that
  camera in a second scene is to add that source to it.
* A source placed on another machine (a node) opens its own device on that
  machine.
