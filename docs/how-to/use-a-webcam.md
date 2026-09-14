# Use a webcam

You have a USB camera, or the one built into the laptop, and you want it on the
programme. Ten minutes, including the part where the operating system asks
whether you meant it.

## Before you start

The camera has to work outside the mixer first. Open Photo Booth, Cheese, or
the Windows Camera app and check you can see yourself. If you cannot, nothing
here will help; fix that first.

You also need GStreamer's good plugins, which you have if the mixer is running
at all, and the camera plugin, which ships in this repository.

## 1. Install the plugin

```sh
./plugins/camera/build
gmx plugin add ./plugins/camera
```

`./build` compiles the plugin and stages its binary at `plugins/camera/bin/`,
which is where the manifest says it is. `gmx plugin add` copies the whole
directory into your plugin folder and registers it with the core, live, with no
restart.

```
added camera 0.1.0: camera/source, camera/devices
```

## 2. Find out what cameras you have

```sh
gmx plugin describe camera
```

The plugin also contributes a tool an agent can call, `list_cameras`, which
answers with one entry per camera:

```json
{"cameras": [{"id": "6C707041-05AC-0010-0008-000000000001",
              "name": "MacBook Pro Camera", "api": "avf",
              "width": 1920, "height": 1080}]}
```

If the list is empty, one of three things is true: nothing is plugged in,
another program has the camera open, or the mixer has not been granted camera
permission. On macOS and Windows the permission is granted to the program that
started the mixer, which may be your terminal.

## 3. Add it

```sh
gmx ctl source add cam1 --type camera/source
```

That is the whole command. With no settings it takes the first camera on the
machine and scales it to the canvas.

```sh
gmx ctl status
```

```
cam1   camera/source   live    1280x720@30
```

Then put it on air:

```sh
gmx take cam1
```

To pick a particular camera, use the id from `list_cameras`:

```toml
[[sources]]
id = "cam2"
type = "camera/source"
params = { device = "/dev/video2", label = "Stage wide" }
```

Settings travel in a `params` table, which is what the config file, the API and
the UI's settings drawer all fill in. The command line adds a source without
them; `source.add` over the API takes them with it:

```sh
curl -s -X POST http://127.0.0.1:8080/rpc -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"source.add","params":
       {"id":"cam2","type":"camera/source","params":{"device":"/dev/video2"}}}'
```

## What the settings do, and what they do not

| Setting | What it changes |
|---|---|
| `device` | which camera |
| `resolution` | what you ask the camera for |
| `framerate` | what you ask the camera for |
| `label` | what the operator sees |

`resolution` and `framerate` are requests to the camera, not to the programme.
The picture is scaled and retimed to the canvas whatever the camera sends, so
these change what the camera costs and not what the audience sees. Leave them
empty unless you have a reason: the plugin asks for the canvas size first,
which is usually both the right shape and the cheapest.

That last point is worth a sentence. Ask a modern laptop camera for nothing in
particular and it may well offer you a portrait mode first, and you get a tall
strip in the middle of a wide canvas. Asking for the canvas size is what stops
that, and it is what the plugin does when you leave `resolution` empty.

## What about the camera's microphone

It is a separate source, on purpose:

```toml
[[sources]]
id = "cam1mic"
type = "audio-device/source"
params = { device = "HD Pro Webcam C920" }
```

Keeping them apart is what lets you take the picture from the camera at the
back of the room and the sound from the desk, which is what every church with a
sound engineer actually wants. See
[the audio-device plugin](../../plugins/audio-device/README.md).

## When it does not work

Ask the mixer what it thinks:

```sh
gmx plugin stats camera
```

| What it says | What it means |
|---|---|
| `no data from the camera after 5 s` | something else has the camera, or a permission prompt was never answered |
| `no device matches '...'` | the message lists every camera the machine has. Copy one of those ids |
| `would not agree on a format` | the camera will not do the `resolution` or `framerate` you asked for. Clear both |

This is what a working camera looks like through the conformance harness, on a
MacBook Pro with its built in camera:

```
$ gmx plugin test ./plugins/camera
  ok   manifest               camera v0.1.0: 2 provide(s), 1 tool(s), every path and schema in place
  ok   spawn                  hello in 9 ms (the limit is 5 s), api 1, transport unixfd
  ok   playing                reached PLAYING within the timeout
  ok   video caps             video/x-raw, format=(string)I420, width=(int)1280, height=(int)720, ...
  ok   video buffers          89 buffers, none out of order
  ok   audio buffers          not declared, not expected
  ok   stop                   the pipeline is in NULL and the kind let go
  ok   configure              19 example(s), every one answered
  ok   kill                   killed mid stream, back in 242 frames, away for 102 ms at this
                              source's own end
  ok   footprint              no process, so nothing to measure
camera/source is conformant
```

Ninety buffers in three seconds at thirty frames a second is every frame, and
`unixfd` means the picture reaches the core without being copied. The kill
check replaces the process under a running pipeline and the picture comes
back; the gap it reports is at the camera's own end, and the compositor's
freeze frame is what keeps the programme steady while it happens.

## Taking it off again

```sh
gmx ctl source remove cam1
gmx plugin remove camera
```

In that order. Removing the plugin does not stop a source that is using it.

## Next

* [Capture the screen](capture-the-screen.md), for lyrics and slides.
* [Record to a file](record-to-a-file.md).
* [The first party plugins](../reference/plugins.md).
