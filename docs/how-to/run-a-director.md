# Let the mixer cut by itself

A church service with nobody at the desk. A classroom. A conference room. A
stream that runs for eight hours and has one person watching it who also has
another job.

`gmx-director` reads the mixer's state every couple of seconds, decides which
source should be on air, and takes it. Start with the rules, which need no
model and no account anywhere. Add a model later if the rules are not enough,
and read the cost table before you do.

## The rules, first

```sh
cargo build --release -p gmx-director
dev/plugins.sh build --release
gmx plugin add ./plugins/director
```

Run it beside the mixer. The mixer does not yet start `service` plugins by
itself, so this is how it runs today:

```sh
export GODWINMIX_URL=http://127.0.0.1:8080
export GODWINMIX_TOKEN=your-token
gmx-director --dry-run
```

`--dry-run` decides and logs and takes nothing. Leave it running for ten
minutes and read the log:

```
gmx-director: directing http://127.0.0.1:8080 on the rules (8s hold, dry run)
gmx-director: hold: 'cam1' has been on for 2s of 8s
gmx-director: would take cam2: 'cam2' is moving (0.41) and 'cam1' is not (0.02)
gmx-director: hold: 'cam2' is doing its job
```

Every line says what it decided and why. When the log reads the way a director
would, drop `--dry-run`.

## What it decides, in order

1. **What is on air has failed, stalled, gone away, or lost its picture.** Cut
   now, whatever the hold says. A black programme is the one thing worse than a
   shot held too long.
2. **The shot has not been held long enough.** Hold. `--min-hold` is 8 seconds
   by default and is the single most important number here: a programme that
   changes shot every few seconds is unwatchable.
3. **Something is moving and what is on air is not.** Cut to it. "Moving" is a
   motion score the mixer computes from a source's last two frames, 0 to 1, and
   the threshold is `--motion-delta`, 0.15 by default.
4. **The shot has been held for `--slow-look` seconds** (45 by default). Move
   on to the next live source, going round a rota, so a service does not sit on
   one camera for an hour.
5. Otherwise hold.

A source is a candidate only if it is live, has a picture, has sent a frame
recently, and is in `--source` when you named any.

With one live source the programme never moves, whatever the rules say.

### When there is no motion score

The motion number comes from the mixer's snapshot tracker, which runs only when
something is looking at the pictures. On a headless core with no multiview
subscriber there is no motion at all, and rule 3 never fires.

That is the ordinary case, and it is why rule 4 exists. A director on a
headless core alternates between live sources on the slow look timer, cuts away
from anything that breaks, and holds otherwise. It is not a clever director,
but it is a working one, and it costs nothing.

If you want rule 3, keep a client subscribed to the multiview, or turn the
snapshot tracker on in the config.

### The mixer's own rules sit underneath

A core with `min_hold_ms = 8000` refuses a faster take whatever the director
decides, and it says so:

```
gmx-director: hold: the core is holding the programme: the shot on air has been
up for 131 ms and this core holds a shot for 500 ms, so there are 369 ms left.
```

That is the safety layer doing its job. The director reads it and does not
fight it.

## Tuning it

| It does this | Change this |
|---|---|
| Cuts too often | `--min-hold` up, or `--motion-delta` up |
| Sits on one camera | `--slow-look` down |
| Cuts to the scoreboard | `--source cam1 --source cam2`, so it can only take those |
| Takes a camera that is showing a frozen frame | `idle_ms` down from 2000 |
| Will not take a source at all | ask `explain` why |

`explain` is the plugin's tool:

```
explain {state: {program: "cam1", sources: [{id: "cam1", state: "live"},
                                            {id: "cam2", state: "live"}]},
         held_secs: 60}
```

It answers with the take it would make and the sentence saying why, without
making it.

## Then with a model

The rules cannot read a room. A model can look at the state, and with a
mosaic, at the picture, and say "the speaker moved to the lectern".

`--llm` names a command. Not a vendor, not an API key: a command. The director
writes the prompt on its stdin, closes it, and reads one JSON object off its
stdout:

```json
{"take": "cam2", "reason": "the speaker moved to the lectern"}
```

Anything that reads a prompt and writes an answer fits:

```sh
gmx-director --llm 'claude -p --output-format text' \
             --goal 'Show whoever is speaking. Cut to the scoreboard when the score changes, hold it ten seconds, then go back.'
```

```sh
gmx-director --llm 'ollama run llama3.2'
```

A command that can itself reach the mixer over MCP can look things up before
answering, and the prompt tells it so. Nothing requires that; a command that
only reads the prompt works exactly as well.

### The model never gets the last word

Whatever the command answers goes through the same rules before it reaches the
mixer. A model cannot take a source that is not live, cannot take one outside
`--source`, cannot take one whose picture has stopped, and cannot cut faster
than `--min-hold`. Every refusal is logged with what the model said:

```
gmx-director: hold: the model named 'cam9', which is not a source. The sources
are: cam1, cam2
```

And if the command is slow, missing or broken, the director waits
`llm_timeout_ms` and then decides on the rules for that cycle:

```
gmx-director: the model did not answer inside 10000 ms; deciding on the rules
for this cycle
```

This is not caution for its own sake. The record is that an unattended agent
accepts a zero exit code as a state change and then invents a theory when
reality disagrees; that it behaves differently when it believes the scenario is
real, and guesses wrong most of the time; and that humans approve a dangerous
agent action 86 percent of the time. Deterministic layers hold. The model
proposes; the rules decide.

## What a model costs

From `09-builders.md`, a decision loop at 1 Hz with 3,000 tokens in and 150
out, per hour of programme:

| Model | Uncached | With prompt caching |
|---|---|---|
| Opus 5 | $67.50 | $23.76 |
| Sonnet 5 | $27.00 | $9.50 |
| Haiku 4.5 | $13.50 | no caching below a 4,096 token prefix |
| Gemini Flash Lite | $1.30 | |

And what a picture costs, if you send one. Claude charges an image at
⌈w/28⌉ × ⌈h/28⌉ tokens:

| Picture | Tokens |
|---|---|
| 1920x1080 | 2,691 |
| 1280x720 | 1,196 |
| 640x360 | 299 |
| 320x180 | 84 |

Which gives the two numbers that should decide your design:

| What you do | Per hour |
|---|---|
| Watch 1080p at 1 fps on Opus 5 | $48 |
| One 320x180 frame every 5 s on Haiku 4.5 | $0.06 |

A numeric telemetry line is about 36 tokens: 75 times cheaper than a 1080p
frame, and it carries freeze and silence, which a still cannot.

Time to first token matters as much as the price. Gemini Flash is 0.7 s,
Sonnet 5 at default effort 1.8 s, Opus 5 at default effort 10.5 s. A director
that decides every two seconds and waits ten for an answer is not a director.

`gmx-director` sends no picture at all and writes the state as short plain
lines rather than a JSON document, which is why its prompt is a few hundred
tokens. That is a deliberate choice against the table above: at 1 Hz, the
difference between a compact prompt and a generous one is the difference
between a service that costs pennies and one that costs more than the camera.

Three practical conclusions:

* Start with the rules. They cost nothing and they are right most of the time.
* If you add a model, start with the cheapest one and a `--interval` of 5 or
  10. A director that decides every ten seconds with a hold of eight is doing
  the same job for a sixth of the money.
* Only reach for a picture when the numbers genuinely cannot answer the
  question, and then send a small one, rarely.

## Running it as a plugin

Everything above is `gmx-director` run by hand, because the mixer does not yet
instantiate `service` plugins. The settings are the same either way: install it
with `gmx plugin add ./plugins/director` and set them with
`plugin.settings.set`, and every surface renders the same form from
`schemas/director.json`.

## The Python example

`examples/ai-director.py` is the teaching version of this loop: 290 lines in
one file, with the Anthropic SDK in it and nothing hidden. It is documented in
`docs/agents.md` and it is not going away. Read it to understand the loop.

Run the plugin on a show instead, because it is a process the core supervises,
it restarts when it falls over, its settings are a schema every surface
renders, it holds the programme on rules when the model is wrong, and it costs
nothing when there is no model at all.

## Reference

* `plugins/director/README.md`: every setting, every rule, the recorded live
  test output.
* `docs/agents.md`: the decision loop, `agent.state`, and the short list of
  things an agent must not do.
* `docs/reference/agent-state.md`: what the director reads, and what it costs
  to read.
