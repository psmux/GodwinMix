# AGENTS.md

For a coding agent changing this plugin. Read this before touching anything.

## What this is

A GodwinMix `service` plugin in Rust that decides which source is on air. Rule
based by default; able to consult a command named in the `llm` setting, whose
answer is then put through the same rules.

It runs as a sidecar the core starts and also by hand with `--url` and
`--token`. `main.rs` chooses on `PluginEnv::started_by_core()`.

## Build and test

```sh
cargo test -p gmx-director                      # 50 unit tests
dev/plugins.sh build
gmx plugin test --offline plugins/director
gmx plugin test plugins/director
dev/integrations-live.sh --only director        # 30 s against a real core
```

## Where things are

| Path | What it is |
|---|---|
| `src/rules.rs` | the decision. No clock, no socket, no model. **The file that matters** |
| `src/settings.rs` | `schemas/director.json`, read into a struct |
| `src/llm.rs` | the prompt, the command, and reading its answer |
| `src/service.rs` | `agent.state` in, `program.take` out |
| `src/main.rs` | the two modes, the `Service` trait, the `explain` tool |

## The rules that matter

1. **Every decision goes through `rules::decide` or `rules::check`.** Those two
   functions take a `View` and a `Settings` and answer a `Decision`. They touch
   nothing else, which is why every rule is a unit test. A new rule is an arm
   there and a test beside it, never a condition in `service.rs`.
2. **A model's proposal is checked, never trusted.** `rules::check` refuses a
   source that is not live, one outside `sources`, one whose picture has
   stopped, and any take inside `min_hold_secs`. The requirements are explicit
   that the deterministic layer holds the programme, not the model. Do not add
   a path that reaches `program.take` without going through `check`.
3. **A model that does not answer costs one cycle, not the show.**
   `llm::consult` has a timeout and the caller falls back to
   `rules::decide`. Keep that.
4. **Rule 1 beats the hold.** A source that failed, stalled, vanished or froze
   is cut away from at once. A black programme is worse than a shot held too
   long.
5. **`motion` is often absent.** On a headless core there is no snapshot
   tracker, so `Shot::motion` is `None` and rules 1, 2, 4 and 5 do the work. A
   new rule must behave when every motion is `None`.
6. **The hold is measured from what the audience sees.** `Memory::held_secs`
   restarts when the programme changes under us, so a take made from the web UI
   or over OSC resets it too.
7. **The prompt is small on purpose.** A 1 Hz loop at 3,000 tokens in is $67.50
   an hour on Opus 5. `llm::describe` writes short lines, not a JSON document,
   and no picture is sent at all. Read `docs/how-to/run-a-director.md` before
   making it bigger.

## Changing it

* **A new rule**: an arm in `rules::decide`, in the documented order, with a
  sentence saying why that a person can read in a log, plus a test. Then update
  the ordered list in the README and in `SKILL.md`.
* **A new setting**: `schemas/director.json` with a `description`, a `default`
  and at least one example, then `Settings::from_value`.
* **A different prompt**: `llm::prompt`. Keep the answer shape
  `{"take", "reason"}`: `examples/ai-director.py` uses it, so a prompt written
  for one works with the other.
* **A new signal** (audio silence, a scene change, a schedule): add it to
  `Shot` or `View`, read it in `service::view_from`, and use it in a rule.
  `agent.state` is the only thing the director reads, so the signal has to be
  in that document first.

## What not to do

* Do not make the director subscribe to the event stream. It polls
  `agent.state` on its own tempo, which is what `docs/agents.md` recommends and
  what costs the core least.
* Do not let the director keep its own idea of the mixer's state beyond
  `Memory`. The mixer is the state; a director that keeps a copy is a director
  that argues with reality.
* Do not delete `examples/ai-director.py`. It is the teaching version and the
  README says so.
* Do not edit `tests/transcript.jsonl` to make a failing check pass.
