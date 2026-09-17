# Safety

The rules that stand in front of every take. They live in the core, not in a
plugin and not in an agent's prompt, because a prompt is a suggestion and a
plugin can be uninstalled. They apply to every caller through every door:
`/rpc`, `/api/v1`, the CLI, the MCP server and the web UI.

## The table

```toml
[safety]
min_hold_ms = 0
max_takes_per_minute = 120
flash_guard = true
on_operator_silence = { after_secs = 120, action = "alert" }
```

Every key has a default and none of it has to be written out.

| Key | Default | What it does |
|---|---|---|
| `min_hold_ms` | `0` | A take within this window of the last one is refused |
| `max_takes_per_minute` | `120` | Takes allowed in any rolling minute, counted per core |
| `flash_guard` | `true` | The ITU-R BT.1702-3 hold, below |
| `on_operator_silence` | `{ after_secs = 120, action = "alert" }` | What happens when whoever made the last take stops calling |

### Who these rules are for

An unattended caller, which in practice means an agent. A person at a desk is
watching the output and is the safety mechanism rather than the thing being
guarded against; a mixer that refuses a cut because the last one was recent is
not a mixer.

So the table's defaults hold nobody, and a token marked `agent = true` is held
to its own floor whatever the table says: eight seconds between shots and
twelve takes a minute, which is what every caller used to get. An agent's own
`safety` override may still only tighten. An operator who wants an agent to cut
freely gives it a token that is not marked `agent`.

### `min_hold_ms`

A house style, not a standard. No standards body publishes a minimum shot
length, and the three second rule people quote is folklore. Zero by default,
because a vision mixer cuts on a word or on a beat. A locked off lecture might
put it at 30000, and a service that wants shots to breathe at 3000. An agent
token never sees less than eight seconds.

### `max_takes_per_minute`

Counted across the whole core rather than per token, because the programme
only has one picture and two clients each taking sixty times a minute is one
hundred and twenty cuts on air. The default is high enough that a person
cutting a song will not meet it, and low enough to stop a loop that takes on
every pass. An agent token is held to twelve.

### `flash_guard`

This one is regulated. ITU-R BT.1702-3, which Ofcom rule 2.12 gives the force
of a rule in the UK: a cut that changes the picture's luminance by 20 candela
per square metre or more over more than a quarter of the frame is held to 360
milliseconds from the one before it, 334 above 50 Hz, and no more than three
of those are allowed in any one second.

Measuring it needs a look at the picture, and the telemetry probes are the only
thing that takes one. While a client is subscribed to `ext.telemetry` or
`ext.agent`, the probe reads a 16 by 9 luma grid off every programme frame,
converts each sample to luminance on the reference display the standard is
written for (BT.1886, 100 cd/m2 white, studio swing), and tells the guard when
a step crosses the threshold.

While nobody is subscribed there is no measurement, and a cut nothing measured
is not refused. The guard used to assume the opposite and record every cut as a
flash, which put a 360 millisecond floor under every cut on any core with no
probes running. That is not what the standard says, and a guard that refuses
cuts it has no evidence about gives false assurance rather than compliance. If
the flash guard has to be in force, run something subscribed to `ext.telemetry`
so there is something to measure.

### `on_operator_silence`

The watchdog for an unattended show. It watches the token that made the last
take. If that token makes no RPC call at all for `after_secs`, the core raises
a `critical` alert on the event stream, in the log, to `[alerts] webhook` and
to the `alert.raised` hook, and then does one of:

| `action` | What happens |
|---|---|
| `"alert"` | Nothing else. The default, because a programme that keeps running is the safe state |
| `"hold"` | Further takes are refused until somebody calls |
| `"slate"` | The programme cuts to the slate |
| `"fallback:cam1"` | The programme cuts to that source |

Any call from any token clears it. A misspelt action is a startup error rather
than a silent `alert`: a `"slat"` that quietly did nothing would be found
during the show it was meant to save.

## Per token numbers

```toml
[[tokens]]
id = "vision-desk"
secret = "..."
scopes = ["read", "operate"]
safety = { min_hold_ms = 1500, max_takes_per_minute = 60 }

[[tokens]]
id = "studio-agent"
secret = "..."
scopes = ["read", "operate"]
agent = true
safety = { min_hold_ms = 1500 }     # ignored: an agent may only tighten
```

A token without `agent = true` belongs to a person at a desk and may move any
of the three numbers in either direction. A token with `agent = true` may only
make them harder: a higher `min_hold_ms`, a lower `max_takes_per_minute`, and
`flash_guard` never turned off. An agent that has been told to raise its own
limits therefore finds that it cannot, which is the point.

The numbers in force for a token are in `agent.state` with
`response_format: "detailed"`, under `safety`.

## What a refusal looks like

```json
{
  "code": -32003,
  "message": "the shot on air has been up for 1200 ms and this core holds a shot for 8000 ms, so there are 6800 ms left. Wait 6800 ms and take again, or set [safety] min_hold_ms lower in the config and restart.",
  "data": {
    "rule": "min_hold",
    "retry_after_ms": 6800,
    "retryable": true,
    "method": "program.take"
  }
}
```

`data.rule` is one of `min_hold`, `rate_limit`, `flash_guard` or
`operator_silence`, so a client branches without reading English.
`data.retry_after_ms` is how long to wait. On `/api/v1` the status is 429.

## What is and is not held

| Method | Minimum hold | Rate limit | Flash guard |
|---|---|---|---|
| `program.take` | yes | yes | yes |
| `program.revert` | no | yes | yes |

`program.revert` exists to undo a take that turned out wrong. A revert that
has to wait out an eight second hold is not a revert, so the hold does not
apply to it. The rate limit and the flash guard do, because they are about the
picture rather than about the decision.

## The history

`program.history` returns the last hundred takes, newest first, each with the
source, the programme running time it landed on, the event sequence number and
the token id that asked for it. It is held in memory and bounded; the durable
record is the session log, which is append only and which no method exposed to
any client can edit or truncate.

`program.revert` reads the same list: it takes back to the last shot that is
not the one on air now, so holding a camera through three takes still reverts
to the shot before it, and reverting from the slate goes back to the picture.

## Related

* [`docs/reference/agent-state.md`](agent-state.md), which reports the limits
  in force and says when the programme is held.
* [`docs/how-to/use-with-an-ai-agent.md`](../how-to/use-with-an-ai-agent.md).
* `protocol.md`, generated, for the error code table.
