# Why the evals grade the world

Home Assistant built a leaderboard for language models driving a smart home.
The same frontier model scored 94.6 percent answering questions about the
devices and 18.5 percent actually driving them.

That gap is the reason this suite exists and the reason it is built the way it
is. A model that can describe a mixer fluently and cannot switch a camera is a
model that passes every eval anybody writes by reading the reply, and fails on
air. So the reply is not read. What is read is the session log.

## What a case is

An instruction, a starting world, and the changes that instruction should make
to it.

```json
{
  "id": "take-the-wide-shot",
  "instruction": "Put the wide camera on air.",
  "tags": ["take"],
  "initial": { "sources": [{"id": "cam-wide", "uri": "test://smpte"}] },
  "expect_changes": [ { "what": "program", "to": "cam-wide" } ]
}
```

The runner starts a real mixer, sets the world up, marks the session log, hands
the instruction to a driver, waits, and reads the log back. What the agent
said, how it phrased it, whether it explained itself: not measured. Whether
`cam-wide` went on air: measured.

The same state delta vocabulary `gmx session replay` uses, for the same reason.
[`docs/reference/session-log.md`](../reference/session-log.md) has the table.

## Why three runs, and pass^3

Because one run of a non deterministic thing is an anecdote.

A case counts as passed when it passed three times from three. Not two from
three, not an average. 09 section 5 item 4 is the other half of the same
argument: eight identical runs succeed far less often than one, which is why
every mutating call takes an idempotency key. An eval that reported the mean
would hide exactly the flakiness the mixer was designed around.

## Why abstaining is a case

Eight of the thirty three runnable cases expect nothing to happen.

* A camera that is not there. Guessing which one was meant is how the wrong
  shot goes out.
* A question. "Is the wide camera live?" is not an instruction to make it live.
* Something that is already true. A take that changes nothing is still a take
  in the log and still starts the minimum hold.
* An instruction too vague to act on.
* A rule the core will refuse anyway, like a take inside the hold.

An agent that always does something scores well on a suite made only of things
to do. The failure that costs a broadcast is not a missing take; it is a take
nobody asked for.

## Why the numbers are in the cases

Five cases carry a value that has to come through exactly: -6 dB, +3 dB, 0 dB,
a mute, a take armed on a running time. "Turn the wide camera down" is easy.
"Turn the wide camera down to -6 dB" and landing on -3 is a different kind of
wrong, and one that only shows up if the grader looks at the number.

Two more are written and marked pending: a fade over 800 ms, and a picture in
picture inset. Neither method exists yet. They are in the suite so that the day
transitions and scenes land, the cases are already there rather than being
written by whoever remembers.

## Why there is an interruption

One case gives an instruction, waits 300 ms, and gives a different one while
the first is half done. The camera has been added by then and stays; the take
that was still to come does not happen.

Agents finish what they started. An operator changing their mind mid action is
the most ordinary thing in a gallery and the thing a plan-then-execute loop
handles worst.

## Why the shipped driver is not a model

`--driver scripted` is a few hundred lines of rules. It reads the instruction,
works out what to call, and calls it. No model, no key, no network, no cost,
and the same answer every time.

It exists so the harness is tested. When a case fails in CI, it failed because
the core changed or the harness broke, not because a model had a bad afternoon.
A suite whose result moves when somebody else retrains a model is not a
regression test for a mixer.

It is written against the *instruction*, never against `expect_changes`. A
driver that read the expectations would make every case pass and measure
nothing, and it would be very easy to write by accident.

`--driver claude` and `--driver codex` point the same cases at a real agent
through the real MCP server. They are documented in `evals/README.md` and are
not run here: they cost money and they measure the model.

## Why cost and tokens are on the report

09 section 5 gates on them as well as on success. An agent that gets the right
answer by calling forty tools and burning thirty thousand tokens is not a
solution for a church with a Raspberry Pi and a pay as you go key.

The scripted driver reports neither, because neither exists for it. The report
says absent rather than zero. A nil cost and a zero cost are different claims
and an eval report that confuses them is teaching people to read it wrong.

## What it does not measure

It does not measure whether the mixer stayed up: `cargo test` and `gmx chaos`
do that. It does not measure latency: `gmx bench` does. It does not measure
whether the agent's explanation was any good, and it never will.

It measures one thing. Given a sentence a person would actually say, does the
right thing happen to the programme.

## Running it

```sh
cargo build --release
python3 evals/run.py --driver scripted
python3 evals/run.py --driver scripted --tag abstain
python3 evals/run.py --driver scripted --case take-the-wide-shot --runs 1
```

It writes `evals/results/<date>.md` and exits non zero when a case is not at
pass^3. The nightly workflow runs it and uploads the report.

## See also

* `evals/README.md`: adding a case, and the two agent drivers.
* [`docs/reference/session-log.md`](../reference/session-log.md): the state
  deltas everything here is graded on.
* [`docs/agents.md`](../agents.md): what an agent sees of the mixer.
