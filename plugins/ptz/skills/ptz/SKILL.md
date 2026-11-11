---
name: ptz-cameras
description: Move PTZ cameras from GodwinMix and let them follow a person by themselves. Use when an operator has a VISCA, ONVIF or Canon CR-N PTZ camera and wants to aim it, zoom, focus, save and recall shots, or have it track whoever is speaking. Covers adding a camera, the joystick that stops by itself, presets with pictures, and the track_ tools that say what a camera sees.
---

# PTZ cameras

One service plugin. It drives the cameras over their own protocols and, when
asked, reads a camera's picture from the mixer to keep a person in shot.

## Add a camera first

```
camera_add {id: "cam-pulpit", protocol: "visca-ip", address: "192.0.2.20", source: "pulpit"}
```

* `protocol`: `visca-ip` (Sony VISCA over IP, UDP 52381, most church PTZ
  cameras), `visca-tcp` (raw VISCA, often port 5678), `onvif`, `canon` (CR-N
  series), or `fake` (no hardware, for trying things).
* `source` is the mixer source that shows this camera. Without it presets
  have no pictures and tracking cannot see.
* `user` and `password` for ONVIF and Canon.

`camera_list` shows every camera, what it can do and its last error. When
there is only one camera, `camera` can be left out of every call.

## Moving

* `move {pan, tilt, zoom}`: speeds from -1 to 1 (right, up and tighter are
  positive). Each call lasts 300 ms. To keep moving, call it again within
  that; to stop, stop calling or send `stop`. Never leave a camera moving by
  sending one `move` and walking away: it stops by itself, which is the
  point.
* `stop`: stop now, and turn tracking off.
* `zoom {speed}`: zooms until a `zoom {speed: 0}`.
* `position` and `goto {pan, tilt, zoom}`: pan and tilt -1 to 1 across the
  camera's range, zoom 0 (widest) to 1.
* `focus {mode}`: `auto`, `manual`, `one_push`, or `near`, `far` and `stop`,
  which need manual focus first.
* `exposure {mode, iris, shutter, gain}`: the values are the camera's own
  steps, as its manual numbers them.

## Presets

* `preset_save {preset: 1, name: "Pulpit close"}` stores the shot on the
  camera and a thumbnail from the mixer beside it.
* `preset_recall {preset: 1}` moves there and turns tracking off.
* `preset_list {pictures: true}` gives each thumbnail as a data URL.

## Tracking

* `track_observe` says what the camera sees without moving it: each person's
  id (`person-3`), box (0 to 1 from the top left) and confidence. It keeps
  looking for 15 s after the last ask, then stops.
* `track_follow {framing: "waist"}` turns tracking on. Pick who with
  `target: "person-3"`, `"largest"` or `"central"`, or `point: {x, y}` where
  somebody tapped the picture. Framing is `close`, `waist`, `full` or `wide`.
* `track_framing {framing}` changes the framing while it runs.
* `track_state` says who is followed and whether the person was lost
  (`holding`, then a `fallback`).
* `track_stop` turns it off; the camera stays where it is.

Grabbing the joystick (`move`) while tracking overrides it for as long as the
stick is held, then tracking carries on. `stop` and `preset_recall` turn it
off.

## Things that go wrong

* **A camera does not move.** `camera_list` shows its `last_error`. For
  VISCA over IP check the camera's network menu has it switched on; for
  ONVIF check the login and that the camera's clock is right.
* **Tracking sees nobody.** The detector finds what moves against a still
  background, so a person standing perfectly still for a long time fades,
  and while the camera itself pans the whole picture moves. Wider framing
  helps it.
* **Presets have no pictures.** The camera needs a `source`.
