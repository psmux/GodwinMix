# Add a source

A source is anything the mixer can show: a camera, a screen, a microphone, a
clip, a web page, a feed arriving from somewhere else. This page is about
getting one in, from the page in your browser and from the command line.

Two minutes for a camera. Longer if the plugin behind it has to be installed
first, and the page does that for you.

## From the UI

Press **+ Add source** in the Sources bar, or Ctrl+N. On a mixer with nothing
in it yet the tray says so and has the same button. A scene tab's **+** opens
the same picker and puts what you add into that scene.

The picker has the categories down the left side:

| Category | What is in it |
| --- | --- |
| Cameras | Every camera this machine can see, one row each |
| Screens and windows | Monitors and what can be captured from them |
| Microphones and audio | Microphones, line inputs, sound cards |
| Video and images | The media library, and a path for anything else |
| Web pages | A URL rendered by the browser sidecar |
| Streams and feeds | RTMP, SRT, RTSP and HLS coming in, and NDI when it is installed |
| Test patterns | Bars and a tone out of the mixer itself |
| More | Whatever else the plugins on this mixer provide |

The first three list real hardware. The mixer asks every device plugin what it
can see (`device.discover`), and each answer becomes a row with the device's
name, the largest size it advertises, and one **Add** button. The categories
are drawn before that call comes back, so a slow camera never holds the modal
shut, and the row says "Looking for devices" while it waits. **Rescan** asks
again, which is what to press after plugging something in.

A device that is already a source says **Added** and its button is off. It is
matched by name, because that is the name it was added under.

The search box at the top searches everything at once, categories included, so
typing `bars` finds the colour bars and typing `rtmp` finds the address box.

### When the plugin is missing

Cameras, screens and microphones each come from a plugin. If this mixer has not
got it, the category is still there, with a sentence saying so and a button:
**Install camera support**, or screen, or audio input. Pressing it installs the
plugin over the protocol, the same thing `gmx plugin add` does, while the mixer
runs and with nothing going off air. The picker looks for devices again as soon
as the install finishes.

The mixer looks the plugin up in the marketplaces it knows and installs what it
finds. A mixer that knows no marketplace falls back to the repository the first
party plugins live in. See [Install a plugin](install-a-plugin.md) for the
other six forms `plugin.add` takes.

### Things that have to be typed

An incoming feed has an address and nobody can discover it for you. Those kinds
keep their form, reached from the tile under the category they belong to, and
the form is generated from the kind's own settings schema. That is true of a
plugin's kinds as well: the picker reads the schema from `plugin.describe` when
you open the form, so a plugin that ships a settings schema gets a real form
without writing any HTML.

A file that is not in the media library has a path rather than an address.
**Browse** under Video and images opens the box for it. The path is read on the
machine the mixer runs on, which is often not the machine the browser is on.

## From the command line

The same two things, in the same order:

```sh
gmx plugin add camera
gmx ctl source add cam1 --type camera/source
```

A kind named with `--type` needs no address. Anything with an address can skip
`--type` and let the scheme pick the kind:

```sh
gmx ctl source add feed rtmp://192.168.1.20/live/cam1
gmx ctl source add opener /srv/media/opener.mp4
```

Then put it on air:

```sh
gmx ctl take cam1
```

## From the API

`source.add` takes the URI, an optional name, and whatever else the kind
understands underneath. A candidate from `device.discover` is already in that
shape, which is why the picker can add one without asking anything:

```sh
curl -sX POST localhost:8080/api/v1/device/discover \
  -H "Authorization: Bearer $GODWINMIX_TOKEN" -d '{"timeout_ms": 2000}'
```

```json
{"candidates": [{"type": "camera/source", "name": "Logitech BRIO",
                 "params": {"device": "/dev/video0", "label": "Logitech BRIO"},
                 "confidence": 1.0}]}
```

```sh
curl -sX POST localhost:8080/api/v1/sources \
  -H "Authorization: Bearer $GODWINMIX_TOKEN" \
  -d '{"uri": "camera/source", "type": "camera/source",
       "name": "Logitech BRIO", "device": "/dev/video0"}'
```

`uri` is required and a kind named outright has no address, so the type goes
there too. The core derives the id from the name.

## When nothing turns up

* The camera list is empty. Nothing is plugged in, another program has the
  device open, or the mixer has not been granted permission. On macOS and
  Windows the permission belongs to the program that started the mixer, which
  may be your terminal. See [Use a webcam](use-a-webcam.md).
* The screen list is empty, or has one entry standing for the whole screen.
  Most platforms will not enumerate their monitors. See
  [Capture the screen](capture-the-screen.md), which also covers the Wayland
  portal and the macOS permission.
* The whole picker says it could not look for devices. The token this page is
  using has read scope and `device.discover` needs operate.
* A source appears in the tray and never goes live. `gmx ctl status` says what
  state it is in, and the alerts panel says why.

If the supervisor restarts a source, its programme and preview branches resume
with it. The source status reports video and audio again once buffers return,
including kinds with fixed pads such as test patterns.

## Next

* [Compose a scene](compose-a-scene.md) with what you just added.
* [Use a webcam](use-a-webcam.md), for the camera settings worth changing.
* [Capture the screen](capture-the-screen.md).
* [Receive a phone or an OBS stream](receive-a-phone-or-obs-stream.md).
* [Install a plugin](install-a-plugin.md).
