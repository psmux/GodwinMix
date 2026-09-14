---
name: auto-director
description: Direct a GodwinMix programme automatically, choosing which source is on air. Use when an operator wants the mixer to cut by itself: a church service with nobody at the desk, a classroom, a conference room, an unattended stream. Rule based by default with no model and no API key; point the llm setting at a command to have a model propose the shot, which is then checked against the same rules. Covers every setting, what the rules are, and how to watch it before trusting it.
---

# The automatic director

One service plugin, no media. It reads `agent.state` every couple of seconds
and calls `program.take` when the rules say to.

## Watch it before you trust it

```
plugin.settings.set {plugin: "director", settings: {dry_run: true}}
```

It then decides and logs and takes nothing. Ten minutes of that log tells you
whether the hold and the movement threshold are right for the room. Turn it off
when the log reads the way a director would.

## The rules, in the order they are applied

1. What is on air has failed, stalled, gone away or lost its picture: cut now,
   whatever the hold says. A black programme is the one thing worse than a shot
   held too long.
2. The shot has not been held for `min_hold_secs`: hold.
3. Something is moving more than what is on air, by more than `motion_delta`:
   cut to it.
4. The shot has been held for `slow_look_secs`: move to the next live source,
   going round a rota so an operator watching can predict what happens next.
5. Otherwise hold.

A source is a candidate only if it is `live`, has a picture, has sent a frame
within `idle_ms`, and is in `sources` when that list is not empty. With one live
source the programme never moves.

`motion` comes from the core's snapshot tracker. On a core where nothing is
watching, there is no tracker and no motion, and rules 1, 2, 4 and 5 carry the
whole job. That is the ordinary headless case and it works.

## Settings

| Key | Default | What it is |
|---|---|---|
| `interval_secs` | 2 | seconds between decisions |
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

`llm` names a command. The director writes the prompt on its stdin and reads
one JSON object off its stdout:

```json
{"take": "cam2", "reason": "the speaker moved to the lectern"}
```

Any command that reads a prompt and writes an answer fits. A command that can
itself reach the mixer over MCP can look things up before answering.

Two things hold whatever the command is. The director waits no longer than
`llm_timeout_ms` and decides on the rules alone when the wait runs out. And the
answer goes through the rules above before it reaches the mixer: a model cannot
take a source that is not live, cannot take one outside `sources`, cannot take
a frozen one, and cannot cut faster than `min_hold_secs`.

A cycle costs a prompt of a few hundred tokens. `docs/how-to/run-a-director.md`
has the hourly cost table.

## The tool

`explain {state, held_secs}` says what the director would do with a state, and
why, without doing it.

## When it goes wrong

* It never cuts: check the log. Every cycle logs `hold:` with the reason, and
  the reason names the rule.
* It cuts too often: raise `min_hold_secs`, or raise `motion_delta`.
* It sits on one camera: lower `slow_look_secs`.
* It will not take a source: that source is not `live`, has no picture, is
  frozen, or is not in `sources`. `explain` says which.
