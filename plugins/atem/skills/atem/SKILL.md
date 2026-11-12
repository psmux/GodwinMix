---
name: atem-switchers
description: Drive Blackmagic ATEM switchers from GodwinMix, and make GodwinMix and a switcher cut together. Use when an operator has one or more ATEM switchers on the network and wants to cut, run a transition, put a key or a lower third up, fade to black, route an aux, run a macro, or have a take on one move the other. Covers connecting a switcher, the source map, the own, follow and lead modes, and reading tally.
---

# ATEM switchers

One service plugin. It speaks the ATEM network protocol (UDP port 9910) to
each switcher and keeps its state, and it can tie a switcher to GodwinMix's
programme. Every tool is `atem/<tool>` through `tool.call`.

## Connect first

```
connect {id: "atem-main", address: "192.0.2.10", name: "Main switcher"}
list {}
```

`list` says for each switcher whether it is `connected` and, when it is not,
`why` in a sentence: no answer, nothing listening at that address, every
client slot taken, or gone quiet. It keeps trying by itself; you do not need
to connect again. Two switchers are normal: give each its own id, such as
`atem-main` and `atem-overflow`. When only one is connected, `switcher` can
be left out of every call.

## Read before you act

`state {switcher}` has everything: `inputs` (`id`, `long`, `short`), each M/E
in `mes` with `program`, `preview`, `transition` (`style`, `rate` in frames,
`position` 0 to 1), `usk` and `ftb`, then `dsk`, `aux`, `macros`, `tally` and
the `map`. Numbers count from 0 for M/Es, keyers, aux outputs and macros;
inputs are the switcher's own numbers (0 black, 1 and up the inputs, 1000
bars, 3010 media player 1, 10010 the programme).

## Cutting

* `preview {input}` then `cut {}` or `auto {}`: the usual way. `auto` runs the
  transition at its rate.
* `program {input}`: straight to air with no transition.
* `transition {style, rate, position}`: `mix`, `dip`, `wipe`, `dve`, `sting`;
  rate in frames for mix, dip and wipe; position moves the bar.
* `dsk {keyer, action}`: `on`, `off`, `auto` (its own fade, the lower third
  button) or `tie`.
* `usk {keyer, on}`, `ftb {}`, `aux {aux, input}`, `macro {index}`.

An input may be given as a GodwinMix source id once it is mapped:
`preview {input: "cam-pulpit"}`.

## Tie it to GodwinMix

```
map {source: "cam-pulpit", input: 1}
mode {mode: "follow"}
```

* `own`: only the window and agents drive the switcher.
* `follow`: every take in GodwinMix also cuts the switcher to the mapped
  input. One button drives both.
* `lead`: a cut on the switcher's panel moves GodwinMix's programme to the
  mapped source, so the panel stays in charge and GodwinMix records and
  streams.

A switcher is in one mode at a time. If takes start chasing each other
(for example two programs both driving GodwinMix), sync pauses for three
seconds and says so in the log.

## Tally

`tally {}` answers every mapped source as `program`, `preview` or `off`
across every switcher. In lead mode GodwinMix's own tally follows the
switcher by itself, because its programme does.

## When it refuses

Every refusal says what is wrong and what to do: `switcher_down` (the link
is not up: read `list`), `unknown_switcher`, `which_switcher` (two are
connected, name one), or an argument out of range with the range.
