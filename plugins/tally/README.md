# gmx-tally

Red lights on the camera that is on air.

Tally lamps do not speak JSON. What they speak, almost without exception, is
TSL's Under Monitor Display protocol, and version 5 is the one every current
lamp, tally interface, multiviewer and router understands. This plugin
subscribes to `event/tally` on the mixer and turns every change into a TSL UMD
v5.0 packet on UDP or TCP.

## In four hours, from nothing

**1. Find out what your tally interface is listening on**

An address and a port. 8900 is the port TSL's own examples use and what most
interfaces ship set to.

**2. Build and install it**

```sh
cargo build --release -p gmx-tally
dev/plugins.sh build --release
gmx plugin add ./plugins/tally
```

**3. Say which lamp watches which source**

```toml
address = "10.0.0.30:8900"
lamps = [
  { source = "cam1", index = 0, label = "CAM 1" },
  { source = "cam2", index = 1, label = "CAM 2" },
]
```

`index` is the number set on the lamp itself. It defaults to the position in
the list, and `label` defaults to the source id, so the shortest useful setting
is `lamps = [{source = "cam1"}, {source = "cam2"}]`.

**4. Find out which lamp is which**

```
test_lamp {index: 0, colour: "red", hold_ms: 3000}
```

It lights index 0 red for three seconds and puts it out. This is how an
engineer on a ladder finds lamp 3 without a camera going on air, and it is the
first thing to run when the lamps are dark.

## What it sends

One packet per lamp whose colour moved, and the whole set every `refresh_secs`
so a lamp that was power cycled catches up without anybody taking a source.

A packet carries the screen, the UMD index, a control word and the label. The
control word puts the colour on the right hand lamp and on the text and leaves
the left hand lamp off, which is the convention for a rig where one colour
means "on air". Some rigs put preview on the right instead; swap
`program_colour` and `preview_colour` if yours does.

A source with no entry in `lamps` lights nothing. That is deliberate: a mixer
with twelve sources and four lamps should not scatter eight packets at indices
nobody assigned.

## Settings

| Key | Default | What it is |
|---|---|---|
| `protocol` | `udp` | `udp` or `tcp` |
| `address` | `127.0.0.1:8900` | where the tally interface is |
| `screen` | 0 | the display group; a rig with one interface never changes it |
| `lamps` | none | `{source, index, label}` per lamp |
| `program_colour` | `red` | what a source on air shows |
| `preview_colour` | `green` | what a source on preview shows |
| `brightness` | 3 | 0 dimmest to 3 brightest; lamps that ignore it stay at full |
| `unicode` | false | UTF-16LE labels, for anything outside ASCII |
| `refresh_secs` | 10 | resend everything this often; 0 turns it off |

UDP is what a rack of lamps usually wants. TCP is what a multiviewer or a
router usually wants; the sender opens the connection on the first packet, not
at startup, so an interface powered on after the mixer does not stop the mixer
from starting, and it re-opens on the next send after a failure.

## Running it by hand

The mixer does not yet instantiate `service` plugins itself, so this is how it
runs against a live core today:

```sh
gmx-tally --url http://127.0.0.1:8080 --token TOKEN \
          --to 10.0.0.30:8900 \
          --lamp cam1:0:'CAM 1' --lamp cam2:1:'CAM 2'
```

`--tcp` switches the protocol, `--help` lists the rest. When the core does
instantiate services, neither this binary nor `gmx-plugin.toml` changes.

## Tests

**Offline and conformance**, needing no core:

```sh
gmx plugin test --offline plugins/tally
gmx plugin test plugins/tally
```

**Unit**: `cargo test -p gmx-tally`, 32 tests. The TSL codec is tested against
the specification byte by byte: the byte count is everything after itself, the
control word packs its four two bit fields where the specification says, bit 15
is clear for a display message, every colour survives a round trip, a UTF-16
label sets the flag, an ASCII packet replaces what a lamp cannot show rather
than letting the length shift, and a truncated packet decodes to nothing rather
than a guess. The board that decides which lamps changed is tested without a
socket, and the UDP and TCP senders are tested against real loopback sockets.

**Live**, against a real core started the way `dev/smoke.sh` starts one:

```sh
dev/integrations-live.sh --only tally
```

`dev/tsl-listen.py` binds a UDP port and decodes each packet the way a tally
interface would. The run on 2026-09-14, macOS arm64, with cam2 on air and then
a take of cam1:

```
a TSL packet arrives with the right lamp bits             ok
    screen=0 index=0 right=off text=off left=off brightness=3 label='CAM 1'
    screen=0 index=1 right=red text=red left=off brightness=3 label='CAM 2'
    screen=0 index=0 right=red text=red left=off brightness=3 label='CAM 1'
```

## When it goes wrong

* **Every lamp is dark.** Check `address` and `protocol` against what the
  interface is listening on, then run `test_lamp {index: 0}`. Every send
  failure is logged with the address and the operating system's own message.
* **One lamp is wrong.** Its `index` does not match what is set on the lamp.
  `test_lamp` on each index in turn finds it in a minute.
* **The colours are swapped.** Some rigs put preview on the right hand lamp.
  Swap `program_colour` and `preview_colour`, or set one to `off`.
* **Labels show as question marks.** The lamp wants UTF-16: set
  `unicode = true`.

## A note for whoever merges this

The core pushes `event/tally` on the back of `event/program.took`, and the
function that sends it returns early if the client did not also subscribe to
`program.took` (`crates/godwinmix/src/control/ws.rs`, `deliver`). A client that
subscribes to `tally` alone gets the tally once, on connect, and never again.
This plugin subscribes to `program.*` as well, which a tally client wants
anyway, so it is right either way. It is worth knowing before somebody else
loses an afternoon to it.
