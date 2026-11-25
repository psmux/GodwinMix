# Connect an ATEM switcher

Drive a Blackmagic ATEM switcher from GodwinMix's Outputs panel or an agent,
and, if you want, make GodwinMix and the switcher cut together.

You need the switcher's IP address. ATEM Setup shows it, and so does the
switcher's front panel on the models that have one. The computer running
GodwinMix must reach it on UDP port 9910.

## Install the plugin

```sh
cd plugins/atem && ./build
gmx plugin add ./plugins/atem
```

## Connect

Every plugin tool goes through the mixer's `tool.call`, as `atem/<tool>`;
from a shell that is `POST /api/v1/tool/call`:

```sh
curl -s http://127.0.0.1:8080/api/v1/tool/call -H "Authorization: Bearer $GODWINMIX_TOKEN" -H "Content-Type: application/json" -d '{"name": "atem/connect", "arguments": {"id": "atem-main", "address": "192.0.2.10", "name": "Main switcher"}}'
```

The rest of this page gives just the body of each call.

Then `list {}`. Within a second the switcher shows `connected: true`. If it
does not, `why` says what to check:

* **no answer**: the address is wrong, the switcher is off, or a firewall
  blocks UDP 9910. Ping the address first.
* **nothing listens**: the address answers but is not the switcher's, or
  the switcher is still starting.
* **every client slot in use**: an ATEM switcher takes a few clients at
  once. Close a copy of ATEM Software Control or another program.

You do not need to connect again after a cable is pulled or the switcher is
restarted: the plugin keeps trying, waiting a little longer each time up to
eight seconds, and `why` says what it is waiting for.

A second switcher is a second `connect` with its own id, such as
`atem-overflow`. With two, name the one you mean with `switcher` in every
call.

To keep switchers in the plugin's settings instead, which a site that does
not change wants:

```json
{"switchers": [
  {"id": "atem-main", "address": "192.0.2.10", "name": "Main switcher",
   "mode": "follow", "map": {"cam-pulpit": 1, "cam-wide": 2}}
]}
```

## Drive it

`state {}` lists the inputs with their names, and what is on programme and
preview. Then:

```json
{"input": 2}                       preview
{}                                 auto (the transition at its rate) or cut
{"keyer": 0, "action": "auto"}     dsk: the lower third on or off
{"aux": 0, "input": 10010}         aux: programme out of aux 1
{"index": 0}                       macro: run macro 1
```

As soon as a switcher is connected, a chip in the Programme monitor's
header shows its link and what it has on programme (red) and preview
(green). Press the chip, or **Edit** on its row under **ATEM switchers** in
the Outputs panel, for switcher properties: the mode, both buses, Cut and
Auto, the source map, the transition, the keyers and fade to black, the aux
outputs and the macros. **Add switcher** in the same place connects one
from the page.

## Make GodwinMix and the switcher cut together

First say which input shows which GodwinMix source:

```json
{"source": "cam-pulpit", "input": 1}
{"source": "cam-wide", "input": 2}
```

Then pick a mode with `mode {mode}`:

* **follow**: you cut in GodwinMix (the window, a Stream Deck, an agent) and
  the switcher cuts to the mapped input too. One button drives both.
* **lead**: you cut on the switcher's own panel and GodwinMix's programme
  moves to the mapped source, so GodwinMix records and streams what the
  panel chose. GodwinMix's tally, and any tally lamps it drives, follow.
* **own**: nothing moves by itself. The window's take asks whether to cut
  GodwinMix, the switcher, or both.

Follow and lead need the plugin to reach the core, which it does by itself
when the core starts it. If takes ever start bouncing between the two
(two programs both driving GodwinMix, for instance), the plugin pauses
follow and lead for three seconds and says why in the log.

## See also

* [Reference: the atem plugin](../reference/atem.md): every tool and answer
* [The ATEM network protocol](../reference/atem-protocol.md)
* [The camera and switcher controls' calls](../reference/camera-controls.md)
