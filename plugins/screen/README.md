# screen

A monitor, or a rectangle of one, as a source. Lyrics from the second screen,
slides from a laptop, a tutorial with the mouse pointer in it.

## Desktop duplication, and nothing else

This captures what is on the screen. It does not hook into another program to
read what that program is drawing, so a game in exclusive fullscreen shows as
black. The answer is borderless windowed mode, and the limit is on purpose: a
capture hook is a per game, per driver, per anti cheat problem that never stops
being one, and a mixer that took it on would spend its life there instead of on
the programme.

It also captures no sound. What comes out of the speakers is a separate source
(a monitor input on Linux, a loopback device on Windows, a virtual device such
as BlackHole on macOS), added as `audio-device/source`.

## What you need

* GStreamer 1.24 or later. Linux: `gstreamer1.0-plugins-good` for X11, plus
  `gstreamer1.0-pipewire` and `xdg-desktop-portal` for Wayland. macOS:
  `brew install gstreamer`. Windows: the GStreamer runtime.
* On macOS, screen recording permission, granted to whatever started the mixer.
  Until someone answers that dialogue the picture is black and there is no
  error. System Settings, Privacy and Security, Screen Recording; then restart
  the mixer.

## Three steps

```sh
./plugins/screen/build
gmx plugin add ./plugins/screen
gmx ctl source add lyrics screen/source --set monitor=1 --set show_cursor=false
```

Ask the machine what it will do first, which also tells you which permission is
waiting:

```sh
gmx plugin describe screen          # the settings and the tool
```

## Settings

| Setting | Default | What it does |
|---|---|---|
| `monitor` | 0 | which screen, counting from 0 |
| `region` | whole monitor | `X,Y,WIDTH,HEIGHT` in screen pixels from the top left |
| `show_cursor` | true | whether the mouse pointer is in the picture. Turn it off for lyrics |
| `display` | from the environment | X11 only: which display, `:0` |
| `node_id` | empty | Wayland only: a PipeWire node the portal has granted |
| `label` | empty | a name for the operator |
| `element` | automatic | force one capture element |

Every setting except `label` reopens the capture. That costs one freeze frame,
which the core covers.

## What it uses, per platform

| Platform | Element | Notes |
|---|---|---|
| Linux, X11 | `ximagesrc` | works with no permission |
| Linux, Wayland | `pipewiresrc` | needs a node id from `xdg-desktop-portal`; see below |
| macOS | `avfvideosrc` with `capture-screen` | needs screen recording permission |
| Windows | `d3d11screencapturesrc`, then `dxgiscreencapsrc`, then `gdiscreencapsrc` | Desktop Duplication |

## Wayland, honestly

On Wayland no program may read the screen without the person at the keyboard
agreeing, and they agree through `xdg-desktop-portal`, which puts up a window
asking which screen to share. This release does not raise that dialogue itself.
Doing it means speaking D-Bus to the portal, holding the session open for as
long as the capture runs, and receiving a PipeWire file descriptor over that
socket; it is a lot of machinery to ship untested, and a headless mixer on a
server has no portal to talk to at all.

What works today: get a node id from a portal session some other program
already opened and put it in `node_id`. For example, with the portal test tools
installed:

```sh
# Prints a node id for a screen cast the portal has granted.
/usr/libexec/xdg-desktop-portal-tester screencast    # or your desktop's own helper
gmx ctl source set lyrics node_id=42
```

With no `node_id` the plugin falls past PipeWire to `ximagesrc`, which captures
the X server. Under XWayland that is usually black, and `health` says the
capture is producing nothing rather than pretending.

If you are on Wayland and this matters to you, say so on the issue tracker: the
portal handshake is the next thing this plugin needs and knowing somebody wants
it is what decides the order.

## When it is black

| What you see | What to do |
|---|---|
| `no data from the screen capture after 5 s` on macOS | answer the screen recording prompt, then restart the mixer |
| black on Wayland | set `node_id`, or run the mixer on an X session |
| black with a fullscreen game | ask for borderless windowed mode |
| the wrong screen | `monitor` counts in the platform's order, not the order on the desk. Try the next index |
| `would not agree on a format` | the `region` is off the edge of the screen. Clear it |

## Removing it

```sh
gmx ctl source remove lyrics
gmx plugin remove screen
```

## See also

* [Capture the screen](../../docs/how-to/capture-the-screen.md).
* [The first party plugins](../../docs/reference/plugins.md).
