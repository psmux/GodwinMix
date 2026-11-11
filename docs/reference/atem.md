# Reference: the `atem` plugin

`plugins/atem` drives Blackmagic ATEM switchers over the network and ties
them to GodwinMix's programme. The protocol is in `crates/godwinmix-atem`;
[atem-protocol.md](atem-protocol.md) describes the bytes. Agents and the
camera window reach every tool through the core's `tool.call` as
`atem/<tool>`.

## Settings (`schemas/atem.json`)

```json
{"switchers": [
  {"id": "atem-main", "address": "192.0.2.10", "name": "Main switcher",
   "mode": "follow", "map": {"cam-pulpit": 1, "cam-wide": 2}},
  {"id": "atem-overflow", "address": "192.0.2.11", "mode": "own"}
]}
```

`id` is a slug. `address` takes `:port` only when it is not 9910. A switcher
with a mistake is left out and named in the log; the others connect. A
settings change keeps the session of a switcher whose address did not move.
`connect` adds one while running; it is not written back to the settings.

## Tools

`switcher` may be left out of any call when only one is connected. M/Es,
keyers, aux outputs and macros count from 0; inputs are the switcher's own
numbers, or a GodwinMix source id the map names.

| Tool | Arguments | Answers (`structured_content`) |
|---|---|---|
| `list` | | `{switchers: [{id, name, address, connected, status, why, mode, product}]}` |
| `state` | `switcher` | the state below |
| `tally` | `switcher` (optional) | one switcher's `{program, preview, sources}`, or `{sources}` across all |
| `connect` | `id`, `address`, `name`, `mode`, `map` | the switcher's `list` row |
| `disconnect` | `switcher` | `{switcher, disconnected: true}` |
| `map` | `switcher`, `source`, `input` (null unmaps) | `{switcher, map}` |
| `mode` | `switcher`, `mode` (`own`, `follow`, `lead`) | `{switcher, mode, mapped}` |
| `cut`, `auto`, `ftb` | `switcher`, `me` | `{switcher, sent, me}` |
| `preview`, `program` | `switcher`, `me`, `input` | `{switcher, sent, me, input}` |
| `transition` | `switcher`, `me`, `style`, `rate`, `position` | `{switcher, sent, me}` |
| `dsk` | `switcher`, `keyer`, `action` (`on`, `off`, `auto`, `tie`), `tie` | `{switcher, sent, keyer}` |
| `usk` | `switcher`, `me`, `keyer`, `on` | `{switcher, sent, me, keyer, on}` |
| `aux` | `switcher`, `aux`, `input` | `{switcher, sent, aux, input}` |
| `macro` | `switcher`, `index`, `action` (`run`, `stop`) | `{switcher, sent, action, index}` |

A control tool answers once the command is queued for the switcher's
session, which sends it at once and resends until it is acknowledged. What
the switcher did shows in `state` a few milliseconds later. `transition`
sets rates for mix, dip and wipe; DVE and stinger rates are refused with a
sentence, as they are not implemented yet. `position` is 0 to 1.

## The state

```json
{
  "id": "atem-main", "name": "Main switcher", "address": "192.0.2.10",
  "connected": true, "status": "up", "why": null, "sessions": 1, "mode": "follow",
  "product": "ATEM Mini Pro", "model": 13, "protocol": "2.30", "firmware": "8.1.1",
  "inputs": [{"id": 1, "long": "Camera 1", "short": "CAM1"}],
  "mes": [{"me": 0, "program": 1, "preview": 2,
    "transition": {"style": "mix", "next_style": "mix", "rate": 25,
      "rates": {"mix": 25, "dip": 25, "wipe": 25}, "position": 0.0, "in_transition": false},
    "usk": [{"keyer": 0, "on_air": false}],
    "ftb": {"on": false, "fully_black": false, "in_transition": false, "rate": 25}}],
  "dsk": [{"keyer": 0, "on_air": false, "tie": false, "rate": 25, "in_transition": false, "fill": 3010, "key": 3011}],
  "aux": [{"aux": 0, "input": 10010}],
  "macros": [{"index": 0, "name": "Opening", "description": ""}],
  "macro": {"running": false, "index": null, "last_run": 0},
  "tally": {"program": [1], "preview": [2], "sources": {"cam-pulpit": "program", "cam-wide": "preview"}},
  "map": {"cam-pulpit": 1, "cam-wide": 2}
}
```

`status` is `connecting`, `syncing` (the dump is arriving) or `up`. The
camera window turns this into its own shape in
`ui/panels/cameras/shapes.js` (`atem()`).

## Why a switcher is down

`why` is one of these sentences, from `crates/godwinmix-atem/src/session/link.rs`:

| Cause | What it says to check |
|---|---|
| no answer to the hello | power, the address, a route, a firewall on UDP 9910 |
| nothing listens at the address | that it is the switcher's address, and that it has finished starting |
| every client slot in use | close ATEM Software Control, a panel or another program |
| silent for five seconds | power and the network cable |
| a command not acknowledged after ten tries | it reconnects by itself |

A switcher that is down is retried for as long as it is connected here.
`health` is degraded while any is down, naming each and why.

## Modes

| Mode | What moves what |
|---|---|
| `own` | nothing automatic. The window and agents drive the switcher |
| `follow` | an `event/program.took` in GodwinMix cuts the switcher's M/E 1 programme to the input mapped to the taken source (or scene) |
| `lead` | a programme change on the switcher's M/E 1 to a mapped input calls `program.take` with that source |

Follow and lead need the core: the plugin connects to `GMX_RPC` with its
token, subscribes to `program.*`, and looks at each leading switcher's
programme 25 times a second. Without `GMX_RPC` the tools still work and
nothing moves by itself.

**Loops.** A switcher is in one mode at a time, so no switcher both reads
and writes GodwinMix's programme. No move is made that would change
nothing (the switcher already shows the input, GodwinMix already has the
source). The dump after a reconnect is noted, not taken as a cut. And more
than six automatic moves inside a second pause follow and lead for three
seconds with a warning in the log, which catches two programs or two
plugins chasing each other.

## Tally

The core derives `event/tally` from GodwinMix's own programme, and a
plugin's `event/<name>` notices are not passed on to subscribers. So the
ATEM's tally reaches GodwinMix by two public paths:

* In `lead` mode GodwinMix's programme is the switcher's, so the core's
  tally (and every TSL lamp `plugins/tally` drives from it) follows the
  switcher with no extra step.
* `atem/state` carries `tally` per input and by mapped source, and
  `atem/tally {}` merges every switcher's. The camera window reads it with
  `atem/state` and lights a camera red or green when either GodwinMix or
  the switcher has it on air or next (`tallyOf` in
  `ui/panels/cameras/words.js`).

## Files

| Path | What it is |
|---|---|
| `src/settings.rs` | the settings, and the mode |
| `src/desk.rs` | every switcher with its name, mode, map and last macro |
| `src/view.rs` | what `list`, `state` and `tally` answer |
| `src/tools/` | the tools: `setup.rs`, `control.rs`, `keys.rs` |
| `src/sync/` | follow, lead and the guard (`mod.rs`, `guard.rs`), and the loop that talks to the core (`run.rs`) |
| `tests/` | the tools against a fake switcher on loopback, and a transcript replayed against the binary |
