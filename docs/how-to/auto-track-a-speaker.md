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

The first time, the plugin downloads its person detector, YOLOX nano (about
4 MB), into `~/.godwinmix/models`. Until it is loaded the motion detector
stands in, and `detector` in the answer says so:

```json
"detector": {"running": "motion", "wanted": "yolox", "stage": "loading", "why": "motion stands in while yolox downloads into ... (first use only) and loads"}
```

On a machine that is never online, copy `yolox_nano.onnx` into the models
directory by hand, or set the plugin's `detector` setting to `motion`.

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

## Make it calmer, quicker or roomier

`ptz/track_tune` changes how the camera follows while it runs. For a speaker
who paces, a slower and smoother camera with more space above the head:

```json
{"name": "ptz/track_tune", "arguments": {"camera": "cam-pulpit", "max_speed": 0.4, "max_accel": 1.2, "headroom": 0.15}}
```

A camera that keeps nudging wants a larger `dead_zone`; one that is late to
follow wants a higher `max_speed`. With only `camera` it answers with every
number and its range. An agent can do the same; the numbers are in
[the ptz reference](../reference/ptz.md#tuning-how-a-camera-follows).

## Check on it, take over, turn it off

`ptz/track_state` says who is followed and how it is going. When the person
leaves the shot it says `holding` and keeps the shot where they were; after
two seconds it says `fallback-stay` and the camera stays put until somebody
is in view again.

Grab the joystick (`ptz/move`) at any time: while it is held the camera is
yours, and tracking carries on when you let go. `ptz/track_stop` turns
tracking off and leaves the camera where it is. `ptz/stop` and recalling a
preset turn it off too.

## Tell it about the camera

So that the camera's own pan is not read as everybody walking, the plugin
needs to know how fast the camera turns and how much it sees. Give the
camera `optics` when you add it, from the camera's data sheet and the zoom
you mostly track at:

```json
{"name": "ptz/camera_add", "arguments": {"id": "cam-pulpit", "protocol": "visca-ip", "address": "192.0.2.20", "source": "pulpit",
  "optics": {"hfov_deg": 30, "vfov_deg": 17, "pan_deg_s": 60, "tilt_deg_s": 30}}}
```

Those numbers are also the defaults. They need not be exact: a quarter off
still takes most of the camera's motion out. `camera_motion` in
`track_state` shows what the plugin takes the camera to be doing.

## What it costs

The plugin reads the camera's picture only while tracking or observing runs,
320 pixels wide. On a desktop CPU one frame takes about 0.2 ms to decode and
track with the motion detector (`cargo bench -p gmx-ptz`), and about 7 ms
more with YOLOX (`cargo bench -p godwinmix-track --features tract`). A
Raspberry Pi 4 core is roughly ten times slower: there, set `detector` to
`motion`, or accept a few YOLOX pictures a second. On Linux and
macOS a camera whose picture is on the frame bus can be read from there
instead, set with `bus`, which skips the encode and decode altogether.

## What it cannot do yet

With `detector` set to `motion`, or while the model loads, people are found
by what moves against a still background: someone standing very still for a
long time fades from it, and while the camera turns the detector rests.
The plugin knows a camera's motion only from the speeds it sends; a `goto`
moves the camera without the tracker knowing, so do not `goto` while
tracking (a preset recall turns tracking off for that reason).
