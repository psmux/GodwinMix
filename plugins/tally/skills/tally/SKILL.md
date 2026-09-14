---
name: tsl-tally
description: Drive hardware tally lamps from GodwinMix over TSL UMD v5, on UDP or TCP. Use when an operator has camera tally lights, a tally interface, a multiviewer or a router that takes TSL, and wants the red light on the camera that is on air. Covers the lamp mapping, the colours, the refresh, and the test_lamp tool for finding which index is which.
---

# TSL tally

One service plugin, no media. It subscribes to `event/tally` on the core's
`/rpc` and turns every change into a TSL UMD v5.0 packet.

## What it sends

One packet per lamp whose colour moved, and the whole set every `refresh_secs`
so a lamp that was power cycled catches up without anybody taking a source.

A packet carries the screen, the UMD index, a control word and the label. The
control word puts the colour on the right hand lamp and on the text; the left
hand lamp is left off, which is the convention for a rig where one colour means
"on air".

## Settings

| Key | What it is |
|---|---|
| `protocol` | `udp` or `tcp` |
| `address` | where the tally interface is, `address:port`; 8900 is the usual port |
| `screen` | the display group, 0 unless the rig has several |
| `lamps` | one entry per lamp: `{source, index, label}` |
| `program_colour` | `red` by default |
| `preview_colour` | `green` by default |
| `brightness` | 0 to 3 |
| `unicode` | UTF-16LE labels, for anything outside ASCII |
| `refresh_secs` | resend everything this often; 0 turns it off |

`index` defaults to the position in the list and `label` defaults to the source
id, so the shortest useful setting is `lamps = [{source = "cam1"}, {source =
"cam2"}]`.

A source with no entry in `lamps` lights nothing. That is deliberate: a mixer
with twelve sources and four lamps should not scatter eight packets at indices
nobody assigned.

## The tool

`test_lamp {index, source, colour, label, hold_ms}` lights one lamp for a
moment and puts it out. Give an `index` or a `source`. This is how an engineer
on a ladder finds which lamp is index 3 without a camera going on air, and it
is the first thing to run when the lamps are dark.

## When it goes wrong

* Every lamp is dark: check `address` and `protocol` against what the interface
  is listening on, then run `test_lamp {index: 0}`. A send failure is logged
  with the address and the operating system's own message.
* One lamp is wrong: its `index` does not match what is set on the lamp itself.
  `test_lamp` on each index in turn finds it in a minute.
* The colours are swapped: some rigs put preview on the right hand lamp.
  Swap `program_colour` and `preview_colour`, or set one of them to `off`.
* Labels show as question marks: the lamp wants UTF-16. Set `unicode = true`.
