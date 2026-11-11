# Let a camera follow the speaker

A preacher who walks, a teacher at a whiteboard, a panel where whoever
speaks moves. Turn tracking on and the PTZ camera keeps that person in shot
by itself, framed the way you chose, while you get on with the rest of the
service.

You need a PTZ camera added with a `source`, the mixer source that shows its
picture: [Add a PTZ camera](add-a-ptz-camera.md). The calls below are the
body of a `POST /api/v1/tool/call`, as that page shows.

## See what the camera sees

```json
{"name": "ptz/track_observe", "arguments": {"camera": "cam-pulpit"}}
```

The answer lists each person in view with an id such as `person-3`, a box
and a confidence. Nothing moves. The plugin keeps looking for 15 seconds
after the last time anyone asked and then stops, so an idle camera costs
nothing.

## Turn tracking on

```json
{"name": "ptz/track_follow", "arguments": {"camera": "cam-pulpit", "framing": "waist"}}
```

With no one named it follows the largest person in view, usually the one
nearest the camera. To choose:

* `"target": "person-3"`, an id from `track_observe`;
* `"point": {"x": 0.62, "y": 0.4}`, where somebody tapped the picture, 0 to
  1 from its top left. This is what the camera window sends;
* `"target": "central"`, whoever is nearest the middle.

## Choose the framing

| Framing | The shot |
|---|---|
| `close` | head and shoulders |
| `waist` | from the waist up, the usual shot of someone speaking |
| `full` | the whole person, for someone who walks a lot |
| `wide` | the person and the stage around them |

```json
{"name": "ptz/track_framing", "arguments": {"camera": "cam-pulpit", "framing": "full"}}
```

The shot leaves a little room above the head and a little ahead of where the
person is walking, and the camera eases in and out of every move rather than
jumping.

## Check on it, take over, turn it off

`ptz/track_state` says who is followed and how it is going. When the person
leaves the shot it says `holding` and keeps the shot where they were; after
two seconds it says `fallback-stay` and the camera stays put until somebody
is in view again.

Grab the joystick (`ptz/move`) at any time: while it is held the camera is
yours, and tracking carries on when you let go. `ptz/track_stop` turns
tracking off and leaves the camera where it is. `ptz/stop` and recalling a
preset turn it off too.

## What it costs

The plugin reads the camera's picture only while tracking or observing runs,
320 pixels wide. On a desktop CPU one frame takes about 0.2 ms to decode and
track (`cargo bench -p gmx-ptz`), so a Raspberry Pi keeps up too. On Linux and
macOS a camera whose picture is on the frame bus can be read from there
instead, set with `bus`, which skips the encode and decode altogether.

## What it cannot do yet

The detector finds people by what moves against a still background. Someone
standing very still for a long time fades from it, and while the camera
itself turns, everything in the picture moves. The tracking engine's ONNX
person detector, which does not have this limit, is not yet switched on in
this plugin.
