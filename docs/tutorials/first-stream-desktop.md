# Your first stream, on the desktop app

Fifteen minutes, one camera, one destination. You need the GodwinMix app
installed and a stream key from wherever you are sending the picture (YouTube,
Twitch, your own server).

## 1. Open the app

The window asks which mixer to use.

**This computer** is already chosen. Press Connect.

The app starts the mixer, which takes a second or two, and the window fills
with the mixer's own page: a black picture across the top, an empty list of
sources on one side, an empty list of destinations on the other. The title bar
says which mixer you are looking at and what version it is.

If it asks again with something in red, the message says what went wrong. The
mixer's own account of it is in **Open logs folder** in the menu.

## 2. Add a source

Press **Add a source**. You are asked what kind: a camera or encoder sending
RTMP, a video file, a website.

For a camera or a phone encoder, give the address it publishes to, something
like `rtmp://192.168.1.10:1935/live/cam1`, and a name you will recognise. For a
video file, pick the file.

The source appears in the list with a small moving picture next to it once it
is delivering. Nothing is on air yet.

## 3. Put it on air

Click the source.

The big picture at the top is the programme: what leaves this machine. It is
now your camera. Click another source and it cuts to that one, on the next
frame, without stopping anything downstream. That is the whole of mixing.

## 4. Add a destination

Press **Add a destination**. Paste the full address your platform gave you,
which is its server and your stream key joined together:

```
rtmp://a.rtmp.youtube.com/live2/xxxx-xxxx-xxxx-xxxx
```

It goes green when the platform accepts the connection. You are live. Check
the platform's own page to see the picture arrive, which takes a few seconds.

## 5. Stop

**Quit** in the menu closes the app and stops the mixer with it, which closes
the destination properly.

Closing the window does not stop anything: the window hides, the mixer keeps
streaming, and the tray icon brings the window back. That is deliberate. A
broadcast should not end because somebody tidied their desktop.

## What next

* Your settings live in a file you can edit: **Open config folder** in the
  menu. The canvas size, the bitrate and the reconnect behaviour are all there,
  with a comment on every line.
* The same app drives a mixer running on a server. **Connect to a mixer** in
  the menu, then "Another machine" and its address. Everything above works the
  same way, because it is the same page. See
  [the desktop app](../how-to/desktop-app.md).
* Nothing here needs the app at all. `gmx` does the same things from a terminal
  and the API does them from a script, which is how a mixer with no screen is
  run.
