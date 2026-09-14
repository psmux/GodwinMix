---
name: screen-source
description: Add, configure and diagnose a screen capture source in GodwinMix (a monitor, part of a monitor, lyrics or slides from a second screen). Use this when the operator asks to show their screen, a presentation or lyrics, when a screen source is black, and when you need to know what permission a platform is waiting for.
---

# Screen sources

One instance is one monitor, or a rectangle of one. It is desktop duplication:
what is on the screen. A game running in exclusive fullscreen is not on the
screen as far as the operating system is concerned, and shows as black; the
answer is to ask the player for borderless windowed mode.

## Ask the machine first

Call `gmx_screen_list_screens` with no arguments:

```json
{"element": "avfvideosrc",
 "screens": [{"index": 0, "name": "The whole screen"}],
 "note": "macOS asks for screen recording permission the first time..."}
```

Read `note` before doing anything else. On macOS and on Wayland, a capture
nobody has agreed to is a black picture with no error, and `note` says exactly
which dialogue is waiting and where to grant it.

Most platforms will not enumerate their monitors. Only Windows ships a device
provider for screens; on macOS and X11 the answer is one entry standing for the
whole screen, and `monitor` counts upwards from 0 anyway.

## Add one

```
source.add {id: "lyrics", type: "screen/source", params: {monitor: 1, show_cursor: false, label: "Lyrics screen"}}
```

| Param | What it does |
|---|---|
| `monitor` | which screen, counting from 0. 0 on a single screen machine |
| `region` | part of a screen, as `X,Y,WIDTH,HEIGHT` in screen pixels from the top left. Empty is the whole monitor |
| `show_cursor` | whether the mouse pointer is in the picture. Off for lyrics and slides, on for a tutorial |
| `display` | X11 only: which display, such as `:0` |
| `node_id` | Wayland only: a PipeWire node the desktop portal has already granted |
| `label` | a name for the operator |
| `element` | force one capture element. Only for a driver problem |

Every setting except `label` reopens the capture, which costs one freeze frame
and is reported as applied.

## Per platform

| Platform | Element | What the operator must do |
|---|---|---|
| Linux, X11 | `ximagesrc` | nothing |
| Linux, Wayland | `pipewiresrc` | go through `xdg-desktop-portal` and put the node id in `node_id`. This plugin does not raise the portal dialogue itself; see the README |
| macOS | `avfvideosrc` with `capture-screen` | grant Screen Recording to whatever started the mixer, then restart it |
| Windows | `d3d11screencapturesrc` | nothing for desktop duplication |

## When it is black

| What you see | What it is | What to do |
|---|---|---|
| black, `health` says `no data from the screen capture after N s` | a permission dialogue nobody answered | on macOS, System Settings, Privacy and Security, Screen Recording, then restart the mixer |
| black, and the operator is on Wayland | `ximagesrc` is capturing an X server that shows nothing | get a portal node id and set `node_id` |
| black, and a game is running fullscreen | exclusive fullscreen is not desktop duplication | ask for borderless windowed mode |
| the wrong screen | `monitor` counts from 0 in the platform's order, which is not always the order on the desk | try the next index |
| `the capture and the pipeline would not agree on a format` | the `region` is outside the screen | clear `region` |

## What this plugin is not

It captures no sound. What comes out of the machine's speakers is a separate
source: on Linux a PipeWire or PulseAudio monitor input through
`audio-device/source`, on Windows a loopback device, on macOS a virtual device
such as BlackHole. There is no cross platform way to do it inside a screen
capture, and pretending otherwise would make the setting mean something
different on each machine.

It does not capture one window. The platforms disagree too much about what a
window handle is; `region` over a maximised window is the portable answer.
