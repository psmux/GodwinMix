# camera

A USB camera, a built in laptop camera or a capture card, as a source.

It captures picture only. Sound from a camera is a separate source: see
[No sound here](#no-sound-here) below.

## What you need

* A camera the operating system can already see. If Photo Booth, Cheese or the
  Windows Camera app shows it, this will too.
* GStreamer 1.24 or later with the good plugins. On Debian and Raspberry Pi OS
  that is `gstreamer1.0-plugins-good`; on macOS `brew install gstreamer`; on
  Windows the GStreamer runtime installer.
* On macOS and Windows, permission. The first time the mixer opens a camera the
  system asks, and it asks the program that started the mixer, which may be
  your terminal. If nobody answers the prompt the camera stays black.

## Three steps

```sh
./plugins/camera/build                     # stage the binary
gmx plugin add ./plugins/camera            # install it
gmx ctl source add cam1 camera/source      # a live camera called cam1
```

`gmx ctl status` shows `cam1` going live. `gmx take cam1` puts it on the
programme.

To pick a particular camera, ask the machine what it has first:

```sh
gmx ctl source add cam1 camera/source --set device=/dev/video0
```

The ids come from the `list_cameras` tool, which an agent can call and which
`gmx plugin describe camera` documents.

## Settings

| Setting | Default | What it does |
|---|---|---|
| `device` | first camera | the id from `list_cameras`, the name the system shows, or a number counting from 0 |
| `resolution` | the camera's choice | what to ask the camera for, `1920x1080`. The picture is scaled to the canvas afterwards either way |
| `framerate` | the camera's choice | what to ask the camera for. A slower camera has its frames repeated |
| `label` | empty | a name for the operator |
| `element` | automatic | force one capture element. Only for a driver bug |

`resolution` and `framerate` change what the camera sends and what it costs,
never what the audience sees. A 4K camera scaled to a 1080p canvas costs more
CPU for exactly the same picture, so leave both empty unless you have a reason.

## What it uses, per platform

| Platform | Element | Why |
|---|---|---|
| Linux | `v4l2src` | every camera on Linux is a V4L2 device |
| macOS | `avfvideosrc` | AVFoundation, which is the only way in |
| Windows | `mfvideosrc`, then `ksvideosrc` | Media Foundation first; `mfvideosrc` has an open startup bug (gstreamer#2748) where some cameras never produce a first frame, and Kernel Streaming is the older path that works |

You do not choose. The plugin asks GStreamer's device monitor for the camera
you named and uses whatever element that platform's provider hands back, which
is also how it knows whether your camera wants `device`, `device-index` or
`device-path`. The `element` setting forces one, and is there for the Windows
bug and nothing else.

The media leaves over `unixfd` on Linux and macOS, which costs the core nothing
per frame, and over a streamable Matroska stream on a pipe everywhere else,
including Windows. The core chooses; you do not have to.

## No sound here

A webcam with a microphone in it shows up twice: once as a camera and once as a
sound input. This plugin is the camera. For the microphone, add a second source:

```sh
gmx ctl source add cam1mic audio-device/source --set device="HD Pro Webcam C920"
```

They are separate on purpose. It is what lets you take the picture from the
camera at the back of the room and the sound from the desk, which is what every
church with a sound engineer actually wants.

## When it fails

| What you see | What to do |
|---|---|
| `no data from the camera after 5 s` | something else has the camera open. Close it. On macOS and Windows, check the permission prompt was answered |
| `no device matches '...'` | the message lists every camera this machine has. Copy one of those ids |
| `the camera and the pipeline would not agree on a format` | the camera will not do the `resolution` or `framerate` you asked for. Clear both |
| `this machine has no GStreamer element for capturing a camera` | the good plugins are not installed. See What you need |
| nothing at all, and `plugin.list` does not name it | the binary is not staged. Run `./plugins/camera/build`, then `gmx plugin add` again |

`gmx plugin stats camera` gives the cpu and memory this costs. A 1080p30 camera
over `unixfd` costs the core nothing for the picture itself.

## Removing it

```sh
gmx ctl source remove cam1
gmx plugin remove camera
```

In that order: removing the plugin does not stop a source that is using it.

## See also

* [Use a webcam](../../docs/how-to/use-a-webcam.md), the same thing at more length.
* [The first party plugins](../../docs/reference/plugins.md).
* `skills/source/SKILL.md`, which is what an agent reads.
