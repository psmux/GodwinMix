---
name: godwinmix-operate
description: Run a live GodwinMix video mixer as the operator. Use when the task is to direct a show, cut between cameras, add or configure a source or an output, watch what is on air, or recover a stream that has gone wrong. Covers connecting with a token over /rpc or MCP, reading the state document, taking a source to programme, when to ask for a picture instead of numbers, the safety rules the core enforces for you, and how to rehearse without touching a live output. Do not use it to write a plugin; that is godwinmix-develop.
---

# Operating GodwinMix

The mixer is a daemon. Everything a person can do from the web UI, you do with
the same methods over the same protocol. There is no separate agent API and no
shortcut the UI has that you do not.

## Connect

One protocol, JSON-RPC 2.0, three doors into it. Pick the one your client
already speaks.

| Door | Use it when |
|---|---|
| WebSocket at `/rpc` | you hold a connection and want events pushed to you |
| REST under `/api/v1/` | you are making one call from a script or a `curl` |
| MCP | your client speaks MCP and you want tools rather than HTTP |

Every call carries a token. A token has scopes (`read`, `operate`, `admin`) and
two policies that decide what you can do without a person: `confirm` (`none` or
`required`) and `rehearsal`. Ask for the narrowest token that does the job.
`godwinmix --info` prints the local socket path and the version.

MCP profiles decide how many tools you see. `standard` is 12 core tools inside
about 4,000 tokens; `minimal` is 5 tools inside about 1,200, for a local model
with a small context. Everything else, including every plugin's own tools, is
behind `search_tools`. The hot list never changes shape at runtime, so adding a
source or a plugin does not invalidate your prompt cache.

## Read the state before you act

`agent.state` returns one document sized for a model's context: the sources and
whether each is live, what is on programme, the outputs and their states, and
the current alerts. Ask for `response_format: "concise"` unless you need per
source audio peaks and the last five takes, which is what `"detailed"` adds.

Subscribe with `core.subscribe` and you get a snapshot then deltas, ending each
batch with `event/flush` so you never act on half a state. Turn on only what you
need: `ext: {telemetry: {hz: 2}}` costs almost nothing, `ext: {multiview: {...}}`
builds a whole pipeline. Nothing runs unless a client asks for it, so an `ext`
you leave off is work the machine never does.

If you fall behind, the core sends `event/resync {from_seq}`. Fetch a fresh
snapshot; do not try to patch from where you were.

## Take

```
program.take {scene: "wide-two-shot"}
program.take {source: "cam1"}          # shorthand for a one item scene
```

Rules that the core enforces whatever you send:

* A source must be `live` before it can be taken. A source that is not answers
  `-32001` and the message lists the sources that would have worked.
* `min_hold_ms` (8 seconds by default) refuses a take too soon after the last
  one, with `data.retry_after_ms`. So does `max_takes_per_minute`.
* The flash guard holds cuts that change luminance sharply to 360 ms apart, at
  most three in a second. It is on by default and it is a regulation, not a
  preference.

`program.revert` takes back to the previous source. `program.history` shows the
last 100 takes with who made them.

Every mutating call takes an `idempotency_key` you generate, honoured for 24
hours, and returns the full resulting object. A call that timed out can be sent
again without doubling a take. A call that times out is indeterminate, never
failed: read the state back before deciding anything.

## Numbers first, pictures when numbers cannot answer

`event/telemetry` carries, every tick, under 50 tokens: the shot change score,
the black ratio, a freeze flag, short term and integrated loudness, a silence
flag, and per source liveness. That is 75 times cheaper than a 1080p frame and
it carries things a still cannot.

Ask for a picture when the question is about content: is anyone in the shot, is
the right camera framed, is the graphic in the right place. Set thresholds with
`ext: {agent: {black: 0.98, freeze_ms: 200, silence_ms: 500}}` and the core
pushes `event/agent.state` with a snapshot URL when one crosses, so you are not
polling images. Default snapshots are 320x180. Presence questions survive that
size; reading a lower third does not, so ask for more resolution only for text.

## Safety and rehearsal

* Destructive calls (`source.remove`, `output.remove`, `plugin.remove`,
  `core.shutdown` and the rest) answer `-32020 confirmation required` when your
  token says so, with a `confirm_token` good for 30 seconds. Repeat the call
  with `confirm: <token>`. Do not work around this.
* Every destructive call takes `dry_run: true` and answers with the diff it
  would make against the live state. Use it first.
* A rehearsal token is accepted only by a core started with `--rehearsal`, which
  refuses `output.add`. A live core refuses the token outright. You do not have
  to work out which one you are on; the credential does it.
* The session log is append only and nothing you can call edits or truncates it.

## When it goes wrong

Read the error. Every one names the current state and the next step, and carries
a `data` object with the ids or the wait that would work. The codes:

| Code | What to do |
|---|---|
| -32001 | wait for the event the message names, then retry |
| -32002 | your token lacks the scope. Ask for a wider one; do not retry |
| -32003 | safety refused it. Wait `data.retry_after_ms` |
| -32004 | the id does not exist. The message lists the ones that do |
| -32010 | the plugin died mid call. Retry once the supervisor restarts it |
| -32020 | repeat the call with `confirm: <data.confirm_token>` |

`plugin.list` and `plugin.stats` carry per instance CPU, memory, latency,
dropped buffers and restarts, so a plugin that is costing the show is visible by
name. `event/alert` carries anything the core wants a person to know.

## What not to do

* Do not poll. Subscribe, and turn on only the `ext` keys you use.
* Do not take a source you have not seen go `live`.
* Do not paper over a refusal by retrying in a loop. Read what it said.
* Do not remove anything without `dry_run` first.
