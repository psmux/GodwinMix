# Capture the screen

Lyrics from the second monitor, slides from a laptop, a tutorial with the mouse
pointer in it.

## What this is, and what it is not

It is desktop duplication: what is on the screen. It is not a hook inside
another program, so a game running in exclusive fullscreen shows as black. The
answer is borderless windowed mode, and the limit is deliberate: a capture hook
is a per game, per driver, per anti cheat problem that never stops being one.

It carries no sound. What comes out of the speakers is a separate source, added
as `audio-device/source`: a monitor input on Linux, a loopback device on
Windows, a virtual device such as BlackHole on macOS.

## Before you start: the permission

This is the part that catches everybody.

* **macOS** asks for Screen Recording permission the first time, and it asks
  the program that started the mixer, which may be your terminal or the Tauri
  app. Until somebody answers, the picture is black and there is no error
  anywhere. Grant it in System Settings, Privacy and Security, Screen
  Recording, then restart the mixer. The permission does not take effect until
  the program restarts.
* **Linux on Wayland** will not let any program read the screen without the
  person at the keyboard agreeing, through `xdg-desktop-portal`. See
  [Wayland, honestly](#wayland-honestly) below.
* **Linux on X11** and **Windows** need nothing.

## Three steps

```sh
./plugins/screen/build
gmx plugin add ./plugins/screen
gmx ctl source add lyrics --type screen/source
```

That takes monitor 0 with the pointer showing. Settings travel in a `params`
table, which is what the config file, the API and the UI's settings drawer all
fill in:

```toml
[[sources]]
id = "lyrics"
type = "screen/source"
params = { monitor = 1, show_cursor = false }
```

`gmx ctl status` shows `lyrics` going live. `gmx take lyrics` puts it on air.

Turn the pointer off for lyrics and slides; leave it on for a tutorial where
the audience is meant to follow the mouse.

## Which monitor

`monitor` counts from 0 in the order the operating system lists them, which is
not always the order they sit on the desk. If you get the wrong one, try the
next number.

Most platforms will not tell GStreamer what their monitors are called: only
Windows ships a device provider for screens. On macOS and X11 the answer to
"what can I capture" is one entry standing for the whole screen, and the index
is the only handle you get. The `list_screens` tool says which case you are in,
and what permission is outstanding.

## Part of a screen

```toml
[[sources]]
id = "slides"
type = "screen/source"
params = { region = "0,0,1920,1080" }
```

`X,Y,WIDTH,HEIGHT` in screen pixels from the top left. It is the portable way
to capture one window: maximise the window and capture its rectangle. Window
capture by handle is not offered, because the platforms disagree too much about
what a window handle is for it to mean the same thing twice.

## Wayland, honestly

On Wayland a screen capture goes through `xdg-desktop-portal`, which puts up a
window asking the person at the keyboard which screen to share. **This release
does not raise that dialogue itself.** Doing it means speaking D-Bus to the
portal, holding the session open for as long as the capture runs, and receiving
a PipeWire file descriptor over that socket. It is a lot of machinery to ship
untested, and a headless mixer on a server has no portal to talk to at all.

What works today: take a node id from a portal session something else has
already opened, and put it in `node_id`.

```toml
params = { node_id = "42" }
```

With no `node_id` the plugin falls past PipeWire to `ximagesrc`, which captures
the X server. Under XWayland that is usually black, and `health` says the
capture is producing nothing rather than pretending otherwise.

If you are on Wayland and this matters, say so on the issue tracker. The portal
handshake is the next thing this plugin needs, and knowing somebody wants it is
what decides the order.

## When it is black

| What you see | What to do |
|---|---|
| `no data from the screen capture after 5 s` on macOS | answer the Screen Recording prompt, then restart the mixer |
| black on Wayland | set `node_id`, or run the mixer on an X session |
| black with a fullscreen game | ask for borderless windowed mode |
| the wrong screen | try the next `monitor` index |
| `would not agree on a format` | the `region` is off the edge of the screen. Clear it |

This is a screen capture through the conformance harness, on a MacBook Pro with
the permission granted:

```
$ gmx plugin test ./plugins/screen
  ok   manifest               screen v0.1.0: 2 provide(s), 1 tool(s), every path and schema in place
  ok   spawn                  hello in 9 ms (the limit is 5 s), api 1, transport unixfd
  ok   playing                reached PLAYING within the timeout
  ok   video caps             video/x-raw, format=(string)I420, width=(int)1280, height=(int)720, framerate=(f ...
  ok   video buffers          90 buffers, none out of order
  ok   audio buffers          not declared, not expected
  ok   stop                   the pipeline is in NULL and the kind let go
  ok   configure              19 example(s), every one answered
  ok   kill                   killed mid stream, back in 243 frames, away for 40 ms at this source's own end ( ...
  ok   footprint              no process, so nothing to measure
screen/source is conformant
```

## Next

* [Use a webcam](use-a-webcam.md).
* [Record to a file](record-to-a-file.md).
* [The first party plugins](../reference/plugins.md).
