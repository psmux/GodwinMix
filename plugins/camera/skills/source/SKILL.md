---
name: camera-source
description: Add, configure and diagnose a camera source in GodwinMix (webcam, capture card, built in camera). Use this when the operator asks for a camera, a webcam or "the camera on the laptop", when a camera source is black or stalled, or when you need to know which cameras a machine has before adding one.
---

# Camera sources

A camera source is one instance of `camera/source`. One instance is one
camera. Adding the same camera twice will not work: the operating system hands
a camera to one program at a time.

## Find out what is there first

Call `gmx_camera_list_cameras` with no arguments. It answers with one entry per
camera:

```json
{"cameras": [{"id": "/dev/video0", "name": "Logitech BRIO", "api": "v4l2", "width": 1920, "height": 1080}]}
```

`id` is what goes in the source's `device` setting. If the list is empty, no
camera is plugged in, or another program has it open, or (on macOS and Windows)
the mixer has not been granted camera permission. Say which of the three you
think it is rather than adding a source that will go black.

## Add one

```
source.add {id: "cam1", type: "camera/source", params: {device: "/dev/video0", label: "Camera 1"}}
```

Every param is optional. With none at all the first camera on the machine runs
at whatever size and rate it prefers, scaled to the canvas. That is the right
first move when the operator has one camera.

| Param | What it does |
|---|---|
| `device` | the id from `list_cameras`, the name the system shows, or a number counting from 0. Empty means the first camera |
| `resolution` | what to ask the camera for, `1920x1080`. Empty lets the camera choose |
| `framerate` | what to ask the camera for. 0 lets the camera choose |
| `label` | a name for the operator |
| `element` | force one capture element. Only for a driver bug; leave it empty |

`resolution` and `framerate` are requests to the camera, not to the programme.
The picture is scaled and retimed to the canvas either way, so changing them
changes what the camera sends and what it costs, never what the audience sees.

## When it does not work

| What you see | What it is | What to do |
|---|---|---|
| `health` says `no data from the camera after N s` | something else has the device, or permission was refused | close the other program; on macOS grant camera permission to the terminal or the app running the mixer, then restart the mixer |
| `health` says `failing` and names an element | the driver refused the size or rate asked for | clear `resolution` and `framerate` and let the camera choose |
| the source never appears | the plugin is not installed for this platform | `plugin.list`; the manifest names the platforms it ships for |
| the picture is there but stutters | the camera is slower than the canvas | lower the canvas rate, or accept repeated frames, which is what happens now |

`configure` applies `label` at once. Changing `device`, `resolution`,
`framerate` or `element` restarts the capture, which costs one freeze frame and
is reported as applied, not as a restart of the process.

## What this plugin is not

It carries no sound. A camera with a microphone in it appears separately as an
audio device, and the way to use it is a second source of type
`audio-device/source` pointed at that device. Keeping them apart is what lets
an operator take the picture from one camera and the sound from a desk.

It does not capture a screen. That is `screen/source`.
