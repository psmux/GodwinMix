# gmx-director

The mixer cuts by itself.

A church service with nobody at the desk, a classroom, a conference room, an
unattended stream. It reads `agent.state` every couple of seconds, decides
which source should be on air, and calls `program.take`.

Out of the box it decides on rules: no model, no API key, no network beyond the
mixer. Point the `llm` setting at a command and it asks that command for an
opinion each cycle, and then puts the answer through exactly the same rules
before anything reaches the mixer.

## In four hours, from nothing

**1. Build and install it**

```sh
cargo build --release -p gmx-director
dev/plugins.sh build --release
gmx plugin add ./plugins/director
```

**2. Watch it for ten minutes before you trust it**

```
plugin.settings.set {plugin: "director", settings: {dry_run: true}}
```

It decides and logs and takes nothing. Ten minutes of that log tells you
whether the hold is right for the room:

```
gmx-director: hold: 'cam1' has been on for 2s of 8s
gmx-director: would take cam2: 'cam2' is moving (0.41) and 'cam1' is not (0.02)
gmx-director: hold: 'cam2' is doing its job
```

**3. Turn it on**

Set `dry_run` back to false. If it cuts too often, raise `min_hold_secs`. If it
sits on one camera, lower `slow_look_secs`.

## The rules

Applied in this order. Every decision is logged with the sentence that made it.

1. **What is on air has failed, stalled, gone away, or lost its picture.** Cut
   now, whatever the hold says. A black programme is the one thing worse than a
   shot held too long.
2. **The shot has not been held for `min_hold_secs`.** Hold.
3. **Something is moving and what is on air is not**, by more than
   `motion_delta`. Cut to it.
4. **The shot has been held for `slow_look_secs`.** Move to the next live
   source, going round a rota so an operator watching can predict what happens
   next.
5. Otherwise hold.

A source is a candidate only if it is `live`, has a picture, has sent a frame
within `idle_ms`, and is in `sources` when that list is not empty. With one live
source the programme never moves.

`motion` comes from the core's snapshot tracker: 0 to 1, how much a source's
picture changed between its last two frames. A static slide is near 0, a
talking head is 0.1 to 0.3, a panning camera is higher. On a core where nothing
is watching there is no tracker and no motion, and rules 1, 2, 4 and 5 carry
the whole job. That is the ordinary headless case and it works, which is why
rule 4 exists.

The core's own safety rules sit underneath all of this. A core with
`min_hold_ms = 8000` refuses a faster take whatever this plugin decides, and
`agent.state` carries a `held` field saying so, which the director reads and
does not fight.

## Settings

| Key | Default | What it is |
|---|---|---|
| `interval_secs` | 2 | seconds between decisions, never under 1 |
| `min_hold_secs` | 8 | seconds a shot is held |
| `slow_look_secs` | 45 | move on after this long; 0 turns it off |
| `motion_delta` | 0.15 | how much more movement earns a cut |
| `idle_ms` | 2000 | a picture this stale is not a shot |
| `sources` | all | the sources this director may take |
| `goal` | empty | what the programme should show, for a model |
| `llm` | empty | a command to consult; empty is rule based |
| `llm_timeout_ms` | 10000 | how long to wait for it |
| `dry_run` | false | decide and log, take nothing |

## With a model

`llm` names a command, not a vendor and not an API key. The director writes the
prompt on its stdin, closes it, and reads one JSON object off its stdout:

```json
{"take": "cam2", "reason": "the speaker moved to the lectern"}
```

Any command that reads a prompt and writes an answer fits: `claude -p`,
`ollama run llama3.2`, a shell script, a Python file. A command that can itself
reach the mixer over MCP can look things up before answering, and the prompt
says so.

Two things hold whatever the command is:

* The director waits no longer than `llm_timeout_ms` and decides on the rules
  alone when the wait runs out. A model that is slow, absent or confused costs
  one cycle its opinion and nothing else.
* Whatever it answers goes through the rules above. A model cannot take a
  source that is not live, cannot take one outside `sources`, cannot take a
  frozen one, and cannot cut faster than `min_hold_secs`. The record in the
  requirements is clear that an agent which believes an action is safe is wrong
  often enough that the deterministic layer has to be the one holding the
  programme.

`docs/how-to/run-a-director.md` has the cost table: a 1 Hz loop is $67.50 an
hour on Opus 5 and $1.30 on Gemini Flash Lite, and the numbers are why the
prompt is a few hundred tokens of plain lines rather than a JSON document with
a picture attached.

## The Python example stays

`examples/ai-director.py` is the teaching version of this loop: 290 lines in
one file, with the Anthropic SDK in it and nothing hidden. Read it to
understand the loop. It is not going away and it is not deprecated.

Run this one on a show instead, because it is a process the core supervises, it
restarts when it falls over, its settings are a schema every surface renders,
it holds the programme on rules when the model is wrong, and it costs nothing
when there is no model at all.

## Running it by hand

The mixer does not yet instantiate `service` plugins itself, so this is how it
runs against a live core today:

```sh
gmx-director --url http://127.0.0.1:8080 --token TOKEN --min-hold 6
```

`--dry-run` decides and takes nothing. `--llm 'claude -p'` consults a model.
`--help` lists the rest. When the core does instantiate services, neither this
binary nor `gmx-plugin.toml` changes.

## The tool

`explain {state, held_secs}` says what the director would do with a state, and
why, without doing it. An operator asks it before handing over a show; an agent
asks it to find out what the rules are without reading them.

## Tests

**Offline and conformance**, needing no core:

```sh
gmx plugin test --offline plugins/director
gmx plugin test plugins/director
```

The offline transcript drives `explain` through three states and checks the
decision each time, so the rules are covered by the replay as well as by the
unit tests.

**Unit**: `cargo test -p gmx-director`, 50 tests. The decision has no clock, no
socket and no model in it, so every rule is a test: the failed source that
breaks the hold, the frozen picture that is never taken to, the rota that wraps
rather than going back and forth, the single live source that is never cut away
from, and the six ways a model's proposal is refused.

**Live**, against a real core started the way `dev/smoke.sh` starts one:

```sh
dev/integrations-live.sh --only director
```

It runs for 30 seconds against two test sources with `--min-hold 3
--slow-look 6`. The run on 2026-09-14, macOS arm64:

```
the director takes on the rules inside 30 s               ok
    gmx-director: hold: the core is holding the programme: the shot on air has
      been up for 131 ms and this core holds a shot for 500 ms, so there are
      369 ms left.
    gmx-director: take cam1: the slate is up and 'cam1' is live
    gmx-director: hold: 'cam1' has been on for 1s of 3s
    gmx-director: hold: 'cam1' has been on for 2s of 3s
    gmx-director: hold: 'cam1' is doing its job
```

## When it goes wrong

* **It never cuts.** Every cycle logs `hold:` with the reason, and the reason
  names the rule.
* **It cuts too often.** Raise `min_hold_secs`, or raise `motion_delta`.
* **It sits on one camera.** Lower `slow_look_secs`. On a headless core there
  is no motion score, so rule 4 is the only thing that moves the programme.
* **It will not take a source.** That source is not `live`, has no picture, is
  frozen, or is not in `sources`. `explain` says which.
