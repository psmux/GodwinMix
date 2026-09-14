---
name: osc-bridge
description: Control GodwinMix over OSC, and send tally and programme out over OSC. Use when an operator has a TouchOSC or Open Stage Control tablet, a QLab cue stack, a lighting desk, a Companion generic-OSC button, or any control surface that speaks OSC and needs to take a source, move a fader, mute, or light a lamp. Covers every address the bridge answers, the decibel to fader conversion, and what it sends back.
---

# The OSC bridge

One service plugin, no media. It listens for OSC on UDP and turns each message
into the same call on `/rpc` that the web UI would make, and it sends tally and
programme changes back out to whatever addresses the operator configured.

## Addresses it answers

| Address | Argument | What it does |
|---|---|---|
| `/program/take` | source id, as a string | `program.take` |
| `/program/take/<id>` | optional button | the same, for a surface that cannot send strings |
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

A momentary button sends 1 when it goes down and 0 when it comes up. The
bridge acts on the 1 and ignores the 0, so a button takes once.

Decibels are converted for you: the core's fader is linear from 0.0 to 10.0 with
1.0 at unity, so `-6` arrives as about `0.501` and `0` as `1.0`. Anything at or
below `-90` dB is silence.

## Addresses it sends

With `send_to` set to one or more `address:port`:

| Address | Arguments |
|---|---|
| `<prefix>/tally/<source id>` | `1` programme, `2` preview, `0` off, then the word |
| `<prefix>/program` | the source id on air, or an empty string for the slate |
| `<prefix>/source/<id>/state` | `connecting`, `live`, `stalled` or `failed` |
| `<prefix>/output/<id>/state` | `connecting`, `live`, `reconnecting` or `failed` |

`prefix` defaults to `/gmx`. Set it to an empty string for bare addresses.

## Settings

`listen`, `send_to`, `send_tally`, `send_program`, `prefix`, `allow_from`.
Every one of them applies while the plugin runs: changing `listen` rebinds the
socket without restarting the process.

`allow_from` is empty by default, which accepts anything that can reach the
port. That is the right default on a show LAN behind a router and the wrong one
anywhere else. OSC over UDP has no authentication of its own, so on a network
you do not control, put the addresses of the surfaces in `allow_from` and bind
`listen` to one interface.

## The tool

`send_osc {address, args, to}` puts one message on the wire so an operator can
prove the path to a tablet or a lamp before a show, without leaving the mixer.

## When it goes wrong

* Nothing happens when a button is pressed: check the surface is sending to the
  machine the mixer is on and to the port in `listen`, and that `allow_from` is
  either empty or has the surface's address in it. Every refused packet is
  logged with the address it came from.
* `-32004 not found`: the source id in the address does not exist. The core's
  error names the ids that would have worked.
* An address that means nothing is logged once with the list of addresses that
  do work, not silently dropped.
