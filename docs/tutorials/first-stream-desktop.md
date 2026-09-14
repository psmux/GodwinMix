# Your first stream with the desktop app

By the end of this you will have a camera or a video file on air from your own
machine, going out to an RTMP destination you choose, with a window you can
click rather than a terminal.

What you need: a Mac, a Windows PC or a Debian based Linux machine, and a place
to stream to. A YouTube or Twitch stream key will do; so will mediamtx on your
own network.

## 1. Install GStreamer

The desktop app does not bundle it yet. Bundling a trimmed GStreamer inside the
installer is planned, with a target of 150 MB for the Windows installer; until
then this is a separate step and it is the only fiddly one.

| | |
|---|---|
| macOS | `brew install gstreamer` |
| Debian, Ubuntu | `sudo apt install gstreamer1.0-plugins-base gstreamer1.0-plugins-good gstreamer1.0-plugins-bad gstreamer1.0-plugins-ugly gstreamer1.0-libav gstreamer1.0-tools` |
| Windows | The runtime and development MSIs from [gstreamer.freedesktop.org](https://gstreamer.freedesktop.org/download/), or `choco install gstreamer gstreamer-devel`. Put `C:\gstreamer\1.0\msvc_x86_64\bin` on `PATH`. |

## 2. Install GodwinMix

Download from the
[latest release](https://github.com/psmux/GodwinMix/releases/latest):

* macOS: the `.dmg`, then drag GodwinMix to Applications.
* Windows: the `.msi`.
* Debian or Ubuntu: the `.deb`, then `sudo dpkg -i godwinmix-desktop_*.deb`.

The app is not signed yet, so macOS will refuse it on first open. Control click
the app, choose Open, then Open again. Windows SmartScreen will want "More info"
then "Run anyway". Signing is planned and until it happens this is the honest
instruction rather than a pretence that it does not happen.

## 3. Start it

Open GodwinMix. The window is the same web UI the server serves, pointed at a
mixer running on your own machine on port 8080.

The window has nothing to show until a mixer answers. If you are running from a
clone rather than an installer, `dev/desktop.sh` starts a mixer first and then
the app.

## 4. Add where the programme goes

In the UI, open Outputs and add a destination:

```
rtmp://a.rtmp.youtube.com/live2/YOUR-STREAM-KEY
```

Set the policy to `cdn` for a public ingest, which backs off much harder on
reconnect than `own` does. Public ingests throttle aggressive reconnects and a
tight retry loop gets you rate limited.

The output goes live carrying black and silence. Your destination will show you
as streaming with a black picture. That is correct and it is the whole design:
the encoder starts once, before there is anything to mix, and runs until you
stop.

## 5. Add something to show

Add a source in the UI. Anything in this list works:

| What you have | What to type |
|---|---|
| A video file | the path, or drag the file onto the window |
| A camera publishing RTMP | `rtmp://its-address/live/key` |
| An IP camera | `rtsp://user:password@camera/stream1` |
| An HLS stream | `https://host/stream.m3u8` |
| A web page | `web+https://example.com/scoreboard` |

A USB webcam or the machine's own screen is not in that list yet. Capture
plugins (`gmx-camera`, `gmx-screen`, `gmx-audio-device`) are the next wave of
work and they are how those arrive. Until then, anything that can publish RTMP
to the mixer is a camera: OBS, a phone app, a hardware encoder.

## 6. Take it

Click its cell. Or press 1 to 9 for the first nine sources; 0 or Escape cuts to
black. The cut lands on the next frame and the outgoing stream does not so much
as hiccup.

## 7. Closing the window does not stop the stream

Two ways out, as buttons at the top right and in the application menu.

* **Close window** leaves the mixer running. The stream does not live in this
  window and closing it by accident must not take the programme down.
* **Stop everything** asks twice, then shuts the mixer down and exits.

## Next

* [Choose a hardware encoder](../how-to/choose-a-hardware-encoder.md), because
  the software one costs about half a core at 720p30 and your machine probably
  has better.
* [Roll an ad break](../how-to/ad-breaks.md).
* [Run it on a server instead](../how-to/headless-server.md), and point this
  same app at it by changing the address. Remote operation is the normal case,
  not a special mode.
