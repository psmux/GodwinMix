# Control the mixer from OSC, and light the tally lamps

Two plugins. One turns OSC messages from a tablet, a cue stack or a lighting
desk into takes and fader moves. The other turns the mixer's tally into the
signal a camera's red light actually understands.

They are separate on purpose: a lot of rigs want one and not the other, and
neither should cost anything when it is not installed.

## OSC in five minutes

**1. Install the bridge**

```sh
cargo build --release -p gmx-osc
dev/plugins.sh build --release
gmx plugin add ./plugins/osc
```

**2. Run it**

The mixer does not yet start `service` plugins by itself, so run it beside the
mixer:

```sh
export GODWINMIX_URL=http://127.0.0.1:8080
export GODWINMIX_TOKEN=your-token
gmx-osc --listen 0.0.0.0:9000
```

**3. Send it something**

If you have no OSC surface to hand, this is one, in ten lines:

```python
import socket, struct

def osc(address, *args):
    pad = lambda t: (t.encode() + b"\0") + b"\0" * (-(len(t) + 1) % 4)
    tags = "," + "".join("s" if isinstance(a, str) else "f" for a in args)
    body = b"".join(pad(a) if isinstance(a, str) else struct.pack(">f", a) for a in args)
    return pad(address) + pad(tags) + body

sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
sock.sendto(osc("/program/take", "cam1"), ("127.0.0.1", 9000))
```

`cam1` goes on air. That is the whole of it.

The same file is in the repository as `dev/osc-take.py`, with the part that
waits for the mixer to confirm.

## Setting up a real surface

TouchOSC, Open Stage Control, QLab and Companion's generic OSC module all
default to port 9000, so usually the only thing to set is the host: the address
of the machine the mixer is on.

Then make buttons that send these:

| What the operator wants | The message |
|---|---|
| Take camera 1 | `/program/take` with the string `cam1` |
| Take camera 1, from a surface that cannot send strings | `/program/take/cam1` |
| Cut to black | `/program/slate` |
| Undo that take | `/program/revert` |
| Pull a fader to -6 dB | `/source/cam1/audio/gain` with `-6.0` |
| A linear fader, 0 to 1 | `/source/cam1/audio/fader` with `0.0` to `1.0` |
| Mute | `/source/cam1/audio/mute` with `1` |
| Take a scene | `/scene/take` with the scene's name |

A momentary button sends 1 when it goes down and 0 when it comes up. The bridge
acts on the 1 and ignores the 0, so a button takes once, not twice.

Decibels are converted for you. The mixer's fader is linear from 0.0 to 10.0
with 1.0 at unity, so `-6` dB arrives as about `0.501`. Anything at or below
`-90` dB is silence, which is what pulling a fader to the bottom means.

## Lamps on the surface

Set `send_to` and the bridge sends the mixer's state back, so a tablet's
buttons light up:

```sh
gmx-osc --listen 0.0.0.0:9000 --send-to 10.0.0.7:9001
```

| Address | Arguments |
|---|---|
| `/gmx/tally/cam1` | `1` programme, `2` preview, `0` off, then the same word |
| `/gmx/program` | the source on air, or an empty string for black |
| `/gmx/source/cam1/state` | `connecting`, `live`, `stalled` or `failed` |
| `/gmx/output/youtube/state` | `connecting`, `live`, `reconnecting` or `failed` |

The number and the word go out together so you can bind a lamp to whichever
your surface finds easier. `--prefix ''` drops the `/gmx`.

This works whoever took the source: the web UI, the terminal UI, a Companion
button, an agent. The mixer derives the tally and everything watching sees the
same thing.

## A word about who can send

**By default, anyone who can reach the port can take a source.** OSC over UDP
has no authentication of its own, and that default is what makes the bridge
usable in five minutes on a show LAN behind a router.

On a network you do not own, it is wrong. Name the surfaces:

```sh
gmx-osc --listen 192.168.10.5:9000 --allow 192.168.10.20 --allow 192.168.10.21
```

Binding `listen` to one interface and filling in `allow_from` are two separate
protections and you want both.

## Tally lamps in five minutes

A tally lamp, a tally interface, a multiviewer or a router almost certainly
speaks TSL UMD, and version 5 is the current one. `gmx-tally` is the
translation.

**1. Find out where the lamps are**

An address and a port. 8900 is what most interfaces ship set to.

**2. Install and run it**

```sh
cargo build --release -p gmx-tally
dev/plugins.sh build --release
gmx plugin add ./plugins/tally

gmx-tally --to 10.0.0.30:8900 \
          --lamp cam1:0:'CAM 1' \
          --lamp cam2:1:'CAM 2'
```

Each `--lamp` is `source:index:label`. The index is the number set on the lamp
itself; the label is what is written on it. Both are optional: the index
defaults to the position in the list and the label to the source id, so
`--lamp cam1 --lamp cam2` is a complete setup for a two camera rig numbered in
order.

**3. Find out which lamp is which**

Ask the plugin's tool:

```
test_lamp {index: 0, colour: "red", hold_ms: 3000}
```

Lamp 0 goes red for three seconds and then goes out. This is how somebody on a
ladder finds lamp 3 without a camera going on air, and it is the first thing to
try when the lamps are dark.

## Which protocol

UDP for a rack of lamps. TCP (`--tcp`) for a multiviewer or a router, which
usually want a connection. The TCP sender opens the connection on the first
packet rather than at startup, so an interface that is powered on after the
mixer does not stop the mixer from starting, and it re-opens after a drop.

## The colours

Red on air, green on preview. That is the broadcast convention and a rig that
uses anything else will be misread under pressure.

Some rigs put preview on the right hand lamp instead of the left. If yours
shows the wrong one, swap `program_colour` and `preview_colour`, or set one of
them to `off`.

A source with no `--lamp` entry lights nothing. A mixer with twelve sources and
four lamps should not be sending packets to indices nobody assigned.

## When something is wrong

| What you see | What it is |
|---|---|
| An OSC button does nothing | The surface is sending to the wrong host or port, or its address is not in `allow_from`. Every refused packet is logged with where it came from |
| `-32004 not found` in the log | The source id in the address does not exist. The error names the ids that would have worked |
| An address is logged as unknown | The list of addresses that do work is in the same line |
| Every lamp is dark | Wrong address or wrong protocol. Run `test_lamp {index: 0}` |
| One lamp is wrong | Its index does not match what is set on the lamp. `test_lamp` on each index in turn finds it |
| Labels show as question marks | The lamp wants UTF-16. Set `unicode = true` |

## What these cost

Neither touches the media path. Both are separate processes that make the same
RPC calls a script would, and neither can stall the programme.

The tally plugin asks the core for `ext.tally`, which is the one thing it turns
on: the core derives the tally document only because a client asked for it, and
stops when the last one goes. The OSC bridge asks for the same thing only when
`send_tally` is on.

## Reference

* `plugins/osc/README.md` and `plugins/tally/README.md`: every setting, every
  address, and the recorded live test output.
* `docs/reference/plugin-manifest.md`: what a `service` plugin declares.
* `docs/how-to/companion-and-streamdeck.md`: the other way to put GodwinMix on
  a hardware panel.
