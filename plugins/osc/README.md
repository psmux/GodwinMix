# gmx-osc

Control GodwinMix from any OSC surface, and send tally and programme back out.

A tablet running TouchOSC or Open Stage Control, a QLab cue stack, a lighting
desk, a Companion generic-OSC button: they all speak OSC, and this turns each
message into the same call on `/rpc` that the web UI makes.

## In four hours, from nothing

You need a running mixer and something that sends OSC. If you have neither, the
second half of this page runs both with Python.

**1. Build and install it**

```sh
cargo build --release -p gmx-osc
dev/plugins.sh build --release
gmx plugin add ./plugins/osc
```

**2. Point your surface at it**

The plugin listens on UDP port 9000 by default, which is what TouchOSC, Open
Stage Control and QLab all offer first. Set your surface's host to the machine
the mixer is on and its port to 9000.

**3. Make a button send `/program/take` with the text `cam1`**

Press it. The source goes on air.

That is the whole of it. Everything below is detail.

## Every address it answers

| Address | Argument | What it does |
|---|---|---|
| `/program/take` | source id, as a string | `program.take` |
| `/program/take/<id>` | optional button | the same, for a surface that cannot send a string |
| `/program/slate` | optional button | cut to the slate |
| `/program/revert` | optional button | `program.revert` |
| `/scene/take` | scene name, as a string | `program.take` with `scene` |
| `/scene/take/<name>` | optional button | the same |
| `/source/<id>/take` | optional button | `program.take` |
| `/source/<id>/audio/gain` | decibels, 0 is unity | `source.audio.set` |
| `/source/<id>/audio/fader` | 0.0 to 10.0, 1.0 is unity | `source.audio.set` |
| `/source/<id>/audio/mute` | 1 mutes, 0 unmutes | `source.audio.set` |
| `/output/<id>/reconnect` | optional button | `output.reconnect` |
| `/output/<id>/remove` | optional button | `output.remove` |

A momentary button sends 1 on the way down and 0 on the way up. The bridge acts
on the 1 and ignores the 0, so one press is one take.

Decibels are converted for you. The core's fader is linear from 0.0 to 10.0
with 1.0 at unity, so `-6` arrives as about `0.501` and `0` as `1.0`. Anything
at or below `-90` dB is silence rather than a very small number, which is what
an operator pulling a fader to the bottom means.

`/scene/take` sends `program.take` with `scene`. Until the scene server lands a
scene name is read as a one item scene, which is to say as a source id. The
address is here now so a cue stack written today keeps working.

## Every address it sends

Set `send_to` to one or more `address:port` and it sends:

| Address | Arguments |
|---|---|
| `<prefix>/tally/<source id>` | `1` programme, `2` preview, `0` off, then the same word as a string |
| `<prefix>/program` | the source id on air, or an empty string for the slate |
| `<prefix>/source/<id>/state` | `connecting`, `live`, `stalled` or `failed` |
| `<prefix>/output/<id>/state` | `connecting`, `live`, `reconnecting` or `failed` |

`prefix` defaults to `/gmx`. An empty prefix gives bare addresses.

The number and the word go out together so a surface can bind a lamp to
whichever it finds easier. Nothing is sent unless `send_to` has something in
it, so a bridge that only listens costs nothing.

## Settings

Every one of them applies while the plugin runs. Changing `listen` rebinds the
socket without restarting the process.

| Key | Default | What it is |
|---|---|---|
| `listen` | `0.0.0.0:9000` | where to receive OSC. A bare port means every address on this machine |
| `send_to` | none | where tally and programme go |
| `send_tally` | true | send `<prefix>/tally/<id>` |
| `send_program` | true | send the programme and the source and output states |
| `prefix` | `/gmx` | in front of every outgoing address |
| `allow_from` | empty | addresses allowed to send commands |

**`allow_from` is empty by default and that means anyone who can reach the port
can take a source.** OSC over UDP has no authentication of its own. On a show
LAN behind a router that is the right default and the thing that makes the
plugin usable in four minutes. On a network you do not own it is wrong: put the
addresses of your surfaces in `allow_from` and bind `listen` to one interface.

## Running it by hand

The mixer does not yet instantiate `service` plugins itself, so this is how it
runs against a live core today:

```sh
gmx-osc --url http://127.0.0.1:8080 --token TOKEN \
        --listen 0.0.0.0:9000 --send-to 10.0.0.7:9001
```

`--help` lists the rest. `GODWINMIX_URL` and `GODWINMIX_TOKEN` work as they do
everywhere else. When the core does instantiate services, neither this binary
nor `gmx-plugin.toml` changes.

## The tool

`send_osc {address, args, to}` puts one message on the wire so you can prove
the path to a tablet before a show, without leaving the mixer.

## Tests

**Offline and conformance**, needing no core:

```sh
gmx plugin test --offline plugins/osc
gmx plugin test plugins/osc
```

```
  ok   manifest               osc v0.2.0: 1 provide(s), 1 tool(s), every path and schema in place
  ok   spawn                  hello in 39 ms (the limit is 5 s), api 1, transport container
  ok   media                  a service provide carries no media, so checks 2, 3 and 6 do not apply
  ok   configure              17 example(s), every one answered

osc/bridge is conformant
```

**Unit**: `cargo test -p gmx-osc`, 34 tests. The OSC codec round trips every
argument type, flattens a bundle, and refuses a truncated packet with the byte
offset. The address map is tested without a socket: every address, the button
release that must not take twice, the decibel conversion, and the refusal that
lists the addresses that would have worked.

**Live**, against a real core started the way `dev/smoke.sh` starts one:

```sh
dev/integrations-live.sh --only osc
```

A ten line Python sender in `dev/osc-take.py` sends `/program/take cam2` and
the test then reads `program.get` until the programme is `cam2`.
`dev/osc-listen.py` binds the outgoing port and prints what comes back. The run
on 2026-09-14, macOS arm64:

```
gmx-osc listens on 53289                                  ok
an OSC take lands as event/program.took                   ok
tally and programme come back out as OSC                  ok
    /gmx/tally/cam1 [0, 'off']
    /gmx/tally/cam2 [0, 'off']
    /gmx/program ['cam2']
    /gmx/tally/cam1 [0, 'off']
```

## When it goes wrong

* **Nothing happens when a button is pressed.** Check the surface is sending to
  the right machine and to the port in `listen`, and that `allow_from` is empty
  or has the surface's address in it. Every refused packet is logged with the
  address it came from.
* **`-32004 not found`.** The source id in the address does not exist. The
  core's error names the ids that would have worked.
* **An address that means nothing** is logged once with the list of addresses
  that do work. It is not silently dropped.
* **A surface that is switched off** is not an incident. Sending to a port with
  nothing on it earns an ICMP unreachable that the kernel reports on the next
  send; the bridge retries once and carries on, and it sends from a separate
  socket so a sleeping tablet can never stop the listener that takes sources.
