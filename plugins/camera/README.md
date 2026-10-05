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
gmx ctl source add cam1 --type camera/source      # a live camera called cam1
```

`gmx ctl status` shows `cam1` going live. `gmx take cam1` puts it on the
programme.

To pick a particular camera, ask the machine what it has first:

```toml
[[sources]]
id = "cam1"
type = "camera/source"
params = { device = "/dev/video0" }
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
| Windows | `ksvideosrc`, then `mfvideosrc` | Kernel Streaming first: on a USB2.0 FHD UVC WebCam it listed the camera in 40 ms where Media Foundation's first probe took 2.4 s, and a `gst-launch-1.0` run to 30 frames of 1080p took 1.8 to 2.1 s against 4.6 s. Media Foundation is the fallback, for a camera Kernel Streaming cannot see, for a first frame that has not come in 3 s, and for the day GStreamer drops `ksvideosrc`, which it has marked deprecated. `mfvideosrc` also has an open startup bug (gstreamer#2748) where some cameras never produce a first frame |

You do not choose. The plugin asks GStreamer's device monitor for the camera
you named and uses whatever element that platform's provider hands back, which
is also how it knows whether your camera wants `device`, `device-index` or
`device-path`. On Windows it asks the Kernel Streaming provider alone first,
and matches the camera you named to it by its device path or its name, so a
`device` id saved from the picker (a Media Foundation path) opens through
Kernel Streaming without being changed. The `element` setting forces one, and
is there for the Windows bug and nothing else.

`start` returns at once and the camera opens behind it, so a slow camera never
runs past the five seconds the core gives `start`. `health` says the camera is
opening until the first frame, then counts frames. An open that fails is tried
again after 1, 2, 4 and so on up to 30 seconds, with the reason in `health`.

The media leaves over `unixfd` on Linux and macOS, which costs the core nothing
per frame, and over a streamable Matroska stream on a pipe everywhere else,
including Windows. The core chooses; you do not have to.

## No sound here

A webcam with a microphone in it shows up twice: once as a camera and once as a
sound input. This plugin is the camera. For the microphone, add a second source:

```toml
[[sources]]
id = "cam1mic"
type = "audio-device/source"
params = { device = "HD Pro Webcam C920" }
```

They are separate on purpose. It is what lets you take the picture from the
camera at the back of the room and the sound from the desk, which is what every
church with a sound engineer actually wants.

## One camera, several sources

Add the same camera to two sources, in one mixer or in two shows on the same
machine, and it is opened once: the first source runs this plugin, and the
others read its frames from the frame bus. The manifest's `share` line is what
asks for that. The first source's `resolution` and `framerate` are the ones
every source gets. See
[Share a camera between shows](../../docs/how-to/share-a-camera-between-shows.md).

## When it fails

| What you see | What to do |
|---|---|
| `no data from the camera after 5 s` | something else has the camera open. Close it. On macOS and Windows, check the permission prompt was answered |
| `the camera is opening` | nothing to do: the picture follows. On Windows it is usually up in a second or two |
| `would not start: another app is using it` | close the app that has the camera (a browser tab, Teams, Zoom, the Camera app). The source picks it up by itself at the next try |
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
