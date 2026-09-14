---
name: min-hold
description: Refuse a take that comes too soon after the last one, with the time left in the reason. Use when the operator asks for a minimum shot length, a hold, a rule against cutting too fast, or a per channel version of the core's min_hold_ms; and read it when a take is refused with "this channel holds a shot for".
---

# min-hold/hold

A `take.before` hook that answers `{allow: false, reason}` when a take arrives
inside the window. It is a plugin rather than a config key because a station
whose rule is not the core's rule (a different window per channel, a window
that lifts during an ad break) can edit this one and ship it, and the core
never changes.

It runs as a WebAssembly component, inside the core process, sandboxed. It
touches no media and cannot.

## Installing

```
gmx plugin add ./plugins/min-hold
```

The manifest declares `placements = ["wasm"]` and `[run] wasm = "plugin.wasm"`,
so the supervisor loads it into the WebAssembly host rather than spawning a
process. The core has to have been built with `--features wasm`; `gmx doctor`
says whether it was, on the `wasm host` line.

## Settings

| Key | Meaning |
|---|---|
| `min_hold_ms` | how long a shot stays on air before another take is allowed. Default 8000, maximum 60000 |

```toml
[plugins.min-hold]
min_hold_ms = 8000
```

## What a refused take looks like

```json
{"code": -32003,
 "message": "a take.before hook refused this take: min-hold: the last take was 1200 ms ago and this channel holds a shot for 8000 ms. Wait 6800 ms, or raise min_hold_ms under [plugins.min-hold]."}
```

`-32003` is the safety code, and it is retryable: wait the time the message
names and call `program.take` again. Do not retry in a loop; the reason names
the wait.

## Asking before taking

```
tool.call {name: "min-hold/hold_state"}
```

answers `{window_ms, remaining_ms, refused}`. `remaining_ms` of 0 means the
next take goes through. An agent running a show unattended should read this
before a take that follows another closely, rather than taking and handling
the refusal.

## The clock

The wall clock, not the pipeline's running time. A minimum hold is about what
a viewer sees and a viewer counts seconds. The pipeline clock restarts when the
pipeline does, and a restart lifting the rule would be a fault.

## What it does not do

It is not the core's `[safety] min_hold_ms`, which is still there and still on
by default. Both apply: the stricter one refuses first. Turn the core's off
with `[safety] min_hold_ms = 0` if this is meant to be the only rule.
