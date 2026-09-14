# The operator eval suite

Thirty three cases that give an agent one instruction and then look at what
happened to the programme. Plus two written against methods that do not exist
yet, marked pending.

Why it is built this way is in
[`docs/explanation/evals.md`](../docs/explanation/evals.md). This page is how
to run it and how to add to it.

## Running it

```sh
cargo build --release
python3 evals/run.py --driver scripted
```

Nothing to install. Standard library Python, the binaries you just built, and
about six minutes.

```sh
python3 evals/run.py --driver scripted --tag abstain      # one group
python3 evals/run.py --driver scripted --case take-the-wide-shot --runs 1
python3 evals/run.py --driver scripted --allow-failures    # report, exit 0
```

It writes `evals/results/<date>.md` and exits non zero when a case is not at
pass^3. The nightly workflow runs it and uploads the report.

## What a run does, per case, three times

1. Start a real mixer on a free port, with the case's canvas and safety
   settings.
2. Add the case's sources and wait for the log to say they are live. Take
   whatever the case says is already on air.
3. Mark the session log. Everything before the mark is the world, not the
   agent's doing.
4. Hand the instruction to the driver.
5. Wait for the log to go quiet.
6. Read the records past the mark, turn them into state deltas, compare them
   with `expect_changes`.

Three from three or it did not pass.

## Adding a case

One JSON file in `evals/cases/`. The file name is the id.

```json
{
  "id": "take-the-wide-shot",
  "instruction": "Put the wide camera on air.",
  "tags": ["take"],
  "initial": {
    "sources": [{ "id": "cam-wide", "uri": "test://smpte" }],
    "program": "cam-close",
    "min_hold_ms": 0
  },
  "expect_changes": [{ "what": "program", "to": "cam-wide" }],
  "notes": "Why this case exists and what it would catch."
}
```

| Key | |
|---|---|
| `instruction` | One sentence, as a person would say it at a desk. |
| `tags` | `take`, `abstain`, `parameterised`, `interruption`, `sources`, `audio`, `adbreak`, `safety`, `scenes`, `pending`. The report groups by the first four. |
| `initial.sources` | Added and waited for before the mark. Use `test://smpte`, `test://ball`, `test://snow`. |
| `initial.program` | Taken before the mark. |
| `initial.program_twice` | A second take, so `program.revert` has somewhere to go. |
| `initial.min_hold_ms` | The safety hold this case runs under. |
| `initial.wait_ms` | Sleep before the mark, for a case that wants the hold to have run out. |
| `initial.gain` | Start `cam-floor` at -12 dB, so "back to 0 dB" is a change. |
| `initial.mute` | Start that source muted. |
| `initial.adbreak` | Roll a break before the mark. |
| `initial.config` | Extra TOML appended to the config. |
| `interrupt` | `{ "after_ms": 300, "instruction": "..." }`. |
| `settle_ms` | How long to wait after the instruction. Default 1200. Raise it for anything armed for later. |
| `expect_changes` | The state deltas. Empty for an abstain case. |
| `pending` | Why this case cannot run yet. It is reported and not run. |

### The deltas

| `what` | `id` | `to` |
|---|---|---|
| `program` | | source id, or `slate` |
| `source` | source id | `connecting`, `live`, `stalled`, `failed`, `removed` |
| `output` | output id | `connecting`, `live`, `retrying`, `failed` |
| `gain` | source id | `-6.0 dB` |
| `mute` | source id | `muted`, `unmuted` |
| `adbreak` | | `armed`, `on air`, `ended` |
| `hook` | hook name | `blocked` |
| `param` | `<method>.<field>` | the value, or `SCHEDULED` to accept any |

Everything but `param` comes off the event stream. `param` reads the command
record, for the few values the status document does not carry, and is kept
deliberately short: a grader reading commands is grading what was asked for
rather than what happened, which is the thing this suite exists not to do.

What one subject did is compared in order. What two subjects did relative to
each other is not, because they are two pipelines on two threads.

### Writing the expectations

Run the case once and look:

```sh
python3 evals/run.py --driver scripted --case my-new-case --runs 1
```

The report prints what differed. Do not paste the produced deltas in as the
expectation without reading them: an expectation copied from a run is a
recording of what the code does, not a statement of what it should do.

## The drivers

### `scripted`, which is what CI runs

A few hundred rules in `evals/drivers/scripted.py`. It reads the instruction,
resolves the source from the words in its id and its URI, decides what to call,
and calls it through the REST routes in `protocol.json`. No model, no key, no
network, no cost, the same answer every time.

It exists so the harness is tested. A failure in CI is the core or the harness,
never a model having a bad afternoon.

It is written against the instruction and never against `expect_changes`. If
you extend it, keep it that way: a driver that reads the expectations makes
every case pass and measures nothing.

### `claude` and `codex`, which are not run here

```sh
python3 evals/run.py --driver claude --model claude-opus-4-6
python3 evals/run.py --driver codex --runs 3
```

Both need their CLI on `PATH` and a key in the environment. The runner writes
an MCP config pointing `gmx mcp` at the case's core, hands the CLI the
instruction, and reads cost and tokens out of the JSON summary the CLI prints.

They are not in the nightly workflow. They cost money per run, and a suite
whose result moves when somebody else retrains a model is not a regression test
for a mixer. Run them when you want the number, pin the model, and put the date
and the model in the report you keep.

### Writing another one

A driver is one function:

```python
def drive(core, case, options):
    core.call("program.take", {"source": "cam-wide"})
    return {"tool_calls": [...], "cost_usd": None, "tokens": None}
```

`core.call(method, params)` speaks the same protocol as everything else, with
the path and the verb read out of `protocol.json`. Return `cost_usd` and
`tokens` where the thing you are driving reports them, and `None` where it does
not. A nil cost and a zero cost are different claims.

Add it to `DRIVERS` in `run.py`.

## The cases today

| Group | Count | |
|---|---|---|
| Taking | 8 | Put a camera up, cut to the slate, go back, two in one sentence. |
| Abstaining | 8 | A camera that is not there, a question, something already true, a rule that will refuse it. |
| Parameterised | 6 | -6 dB, +3 dB, 0 dB, a mute, an unmute, a take armed on a running time. |
| Sources | 5 | Add one, add and take, remove one, remove the one on air. |
| Ad breaks | 3 | Roll one, cut one short, start one by another name. |
| Safety | 3 | The hold refuses the second take, and two things it refuses outright. |
| Interruption | 1 | A second instruction while the first is half done. |
| Pending | 2 | A fade duration, and a picture in picture inset. |
