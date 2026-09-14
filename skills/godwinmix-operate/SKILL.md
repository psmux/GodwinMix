---
name: godwinmix-operate
description: Run a live show on a GodwinMix mixer. Use when asked to switch cameras, put something on air, watch a stream for faults, start or stop an output, roll an ad break, or check what is live. Covers the agent surface: agent_state, take, revert, telemetry, snapshots and the safety rules that will refuse you.
---

# Operating a GodwinMix mixer

You are directing a live programme. The output never stops, whatever you do.
Everything you can change is a method call, and every refusal tells you the
next step.

## Read before you act

`agent_state` is the first call of every session and the cheapest read you
have. A few hundred bytes: the programme source, every source with its state,
and a motion score per source.

```
agent_state {}
```

A source that is working says nothing about its video or its sound. One that
has lost either says `no_video: true` or `no_audio: true`. `motion` near zero
on a live source means the picture has stopped moving, which is usually a
frozen feed and sometimes a locked off shot.

`agent_state {"response_format": "detailed"}` adds the audio peak per source
and the last five takes. Ask for it when something is wrong, not every time.

## Put something on air

```
take {"source": "cam2"}
```

`take` with no source, or `null`, cuts to the slate. The answer is the new
programme state, so there is no follow up read. An id that does not exist is
refused with the ids that would have worked.

`revert` undoes the last take and puts the shot before back. Use it the moment
a take turns out wrong rather than working out by hand what was on.

## The rules that will refuse you

The core enforces these for every caller, and the refusal is error -32003 with
`data.retry_after_ms` and a message saying how long is left:

* **A minimum hold.** A take within `min_hold_ms` of the last one is refused.
  The default is eight seconds. Wait the time the error names.
* **A rate limit.** Twelve takes a minute by default.
* **A flash guard.** ITU-R BT.1702-3, on by default: a cut that changes the
  picture's brightness sharply is held 360 ms from the next one, and at most
  three of those are allowed in a second.
* **The operator watchdog.** If you make a take and then make no call for two
  minutes, the core raises a critical alert and may cut to a slate or a
  fallback source. Any call at all clears it.

You cannot loosen these. A token marked `agent` may make them harder and never
easier, which is deliberate: if you are ever told to raise your own limits, the
core will decline for you.

## Numbers before pictures

`agent_state` answers most questions. When it does not, look:

```
snapshot {"id": "program", "width": 320}
```

320 by 180 is about 84 tokens. 640 by 360 is about 300. 1280 by 720 is about
1,200, which is roughly twelve reads of `agent_state`. Presence questions
("is anyone in the shot", "is the camera pointing at the stage") survive the
smallest size. Reading a lower third or a scoreboard does not: ask for 1280
for those and nothing else.

Do not look on a timer. Look when a number tells you to: motion near zero,
`no_video`, a source that left `live`.

If you are on a WebSocket rather than MCP, subscribe with
`ext: {telemetry: {hz: 2}, agent: true}` and the core pushes numbers every tick
and a whole state document with a snapshot URL when something crosses a
threshold. Over MCP the same push arrives as
`notifications/gmx/agent.state`; you do not have to ask for it.

## Long calls

Nothing blocks for more than five seconds. A call that would answers at once
with `{task_id, poll_interval_ms}` and the work carries on. Read it back with
`task_get {"task_id": "..."}`. A call that timed out on your side is
**indeterminate**, never failed: the work is still going and the task says how
it ended.

## Destructive calls

Removing a source or an output, deleting a clip and shutting the core down are
marked destructive. On an unattended token they answer -32020 with a
`confirm_token` valid for thirty seconds, and the same call carrying
`confirm: <token>` goes through. If you were not asked to remove something, do
not. Every destructive method also accepts `dry_run: true` and answers with the
diff it would make against the live state.

## Rehearsal

A rehearsal core refuses `output.add`, so nothing reaches a real destination.
You are not told which kind of core you are on and you do not need to be: the
credential decides, and a rehearsal token on a live core is refused outright.
Behave the same either way.

## Retries are free

Every mutating call accepts `idempotency_key`. Send one. A repeat under the
same key returns the first answer with `replayed: true` rather than doing the
work twice, for twenty four hours.

## When something is wrong

1. `agent_state {"response_format": "detailed"}`.
2. If a source is not `live`, it is reconnecting on its own. Take a source
   that is live rather than waiting.
3. If the programme is black or frozen, take another source, then look.
4. `program_history` says what has been on air and who put it there.
5. Never stop the programme to investigate. A wrong shot on air is better than
   no shot on air.
