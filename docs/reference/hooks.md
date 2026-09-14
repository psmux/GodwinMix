# Hooks

Lifecycle interception. Your code, called by the mixer when something happens,
without an in process API and without a build of the core.

The how-to is [`docs/how-to/hooks.md`](../how-to/hooks.md). This page is the
contract.

## Where a hook is declared

Two places, and they do not overlap.

**The operator's config**, for a hook with no plugin behind it. One
`[[hooks]]` block each, in `godwinmix.toml`:

```toml
[[hooks]]
event = "take.after"          # required, one of the events below
http = "https://..."          # exactly one of http, command, plugin
command = "/usr/bin/tally"
plugin = "compliance"
timeout_ms = 20               # take.before only, 1 to 100
name = "tally"                # what event/hook.blocked calls it
```

**A plugin's manifest**, for hooks that belong to the plugin. One table, keyed
by event, in `gmx-plugin.toml`:

```toml
[hooks]
"take.before" = { mode = "rpc", timeout_ms = 19 }
"take.after"  = { mode = "command", command = "./on-take.sh" }
"alert.raised" = { mode = "http", url = "https://..." }
```

`mode` is required and is `rpc`, `command` or `http`. `command` needs
`command`, `http` needs `url`, `rpc` needs neither. `gmx plugin test` checks
all of it and names the key that is wrong.

A block that does not parse is reported at startup and skipped. The mixer comes
up. A typo in a webhook URL is not a reason a church cannot stream.

## Events

| Event | Fired | Can delay the decision | Payload |
|---|---|---|---|
| `take.before` | before `program.take` or `program.revert` is applied | yes, within `timeout_ms` | `source` (string or null), `at_running_time_ms` (integer or null), `by` (token id), `revert` (bool) |
| `take.after` | once the take has landed | no | the same |
| `source.added` | after `source.add` | no | `source`, `uri`, `state` |
| `source.removed` | after `source.remove` | no | `source`, `uri` |
| `source.state` | a source moved between connecting, live, stalled and failed | no | `source`, `state` |
| `output.state` | a destination connected, dropped or is retrying | no | `output`, `state`, `reconnects` |
| `alert.raised` | any alert | no | `severity`, `message` |
| `session.start` | the control plane is up, before it serves | no | `version`, `bind`, `log` |
| `session.end` | shutdown, before the mixer stops | no | `version` |
| `plugin.loaded` | a plugin was installed and read | no | `plugin`, `version`, `provides` |
| `plugin.failed` | a plugin would not load | no | `plugin`, `reason` |
| `plugin.state` | either of those | no | `plugin`, `state` |

`source` is null on a take to the slate. `revert` is true when the take came
from `program.revert`, which is what a policy hook usually wants to let through
even when it would have refused the take.

`session.start` and `session.end` are the spellings in the manifest vocabulary
and in 03 section 8. There is no `session.started`.

## The envelope

The same body in all three modes:

```json
{
  "hook": "take.before",
  "ts": "2026-09-14T20:13:58.402Z",
  "payload": { "source": "cam-wide", "at_running_time_ms": null, "by": "desk", "revert": false }
}
```

`ts` is RFC 3339 in UTC, to the millisecond, the same spelling as the log and
the session log.

## The answer

Only read for `take.before`. Everything else's answer is discarded, and for
`http` the body is not even read off the socket.

| Answer | Meaning |
|---|---|
| `{"allow": true}` | let the take through |
| `{}`, `null`, no body, not JSON | let the take through |
| `{"allow": false, "reason": "..."}` | refuse it |
| `{"allow": false}` | refuse it, with a reason saying none was given |
| exit code 2 (`command` mode) | refuse it; the first line of stderr is the reason |
| any other exit code | let the take through |

Silence is consent, deliberately. A hook that refuses by accident takes a
programme off air.

A refusal reaches the caller as `-32003`:

```json
{"code": -32003,
 "message": "a take.before hook refused this take: compliance: cam-stage has no audio. The programme is unchanged. Take a different source, or turn the hook off in the config.",
 "data": {"hook": "take.before", "source": "cam-stage"}}
```

With several hooks on one event, the first refusal wins and the message names
which hook it was.

## Timeouts

| | `take.before` | everything else |
|---|---|---|
| Default | 20 ms | not waited for |
| Maximum | 100 ms | |
| Abandoned after | `timeout_ms` | 5 s |
| On expiry | the take goes ahead, `event/hook.blocked` | `event/hook.blocked` |

`timeout_ms` on a hook that is not `take.before` is ignored, and the manifest
validator says so rather than accepting it quietly.

Several hooks on one event run at the same time. The wait is the longest
timeout any of them asked for, not the sum.

## `event/hook.blocked`

```json
{"hook": "take.before", "plugin": "compliance", "reason": "no answer within 20 ms, so the take went ahead without it. Raise timeout_ms (up to 100 ms) or move the work off the hook."}
```

`plugin` is the plugin that owns the hook, or the URL or command for a
`[[hooks]]` block that has no plugin, or the `name` if one was given. `reason`
names what happened and what to do about it, like every other error in this
protocol.

Raised on a timeout, a connection that failed, a non 2xx answer, a command that
is not installed or exits non zero, and a plugin whose hook process will not
start. The thing the hook was attached to always goes ahead.

The event is on the ordinary stream (`core.subscribe` pattern `hook.*`), in the
session log, and in `gmx session show`.

## `command` mode

* The envelope arrives as one line of JSON on stdin, followed by end of file.
* `GMX_HOOK` is the hook name.
* stdout is read for `take.before` and ignored otherwise.
* The first line of stderr is the refusal reason on exit 2, and is logged on
  any other non zero exit.
* The command line is split the way a shell splits one, with no shell in the
  middle: `python3 on_take.py --loud 'a b'` is four arguments, and `foo && bar`
  is not two commands.
* The child is killed when its timeout fires.

## `rpc` mode

The plugin gets `hook` as a JSON-RPC call on the stdio channel it already
speaks, with the envelope as params.

Two limits, and they are real:

**The hook process is a second instance.** The core has no registry of running
plugin instances that anything can call: the loader interns source provides and
remembers process ids, only sources are ever instantiated, and the
`SidecarService` type that would have held a service instance is constructed
nowhere. So the first time an `rpc` hook fires for a plugin, that plugin is
started as a sidecar of its own and kept for the life of the core. It is
started for its `service` provide where it has one and for its first provide
otherwise. If your hook needs to see the state of your plugin's *source*
instance, it cannot reach it: keep that state somewhere both can read, or use
`http` and let your own process answer.

**`process = "shared"` is not honoured**, for the same reason: there is no
shared instance to join.

When the host grows a real service instantiation, the hook dispatcher becomes a
lookup in it and nothing else about this page changes.

A plugin that declares an `rpc` hook and provides nothing runnable gets a
`hook.blocked` saying so and naming the fix.

## Where a hook runs

Never on the mixer thread. Never on a GStreamer streaming thread. Never inside
the pipeline.

`take.before` is awaited in the control layer, before the take command has been
sent to the mixer at all. Every other hook is a task that nobody waits for. The
compositor produces frames on schedule whether or not a decision is pending,
which is the whole reason a hook can only ever delay a decision.

The acceptance test in `crates/godwinmix/tests/hooks.rs` measures it: a hook
answering at 19 ms delays the take decision by under 20 ms, a hook sleeping 200
ms does not delay it at all, and across both the programme's frame interval
never exceeds 34 ms.

## Cost

A core with no hooks configured does one atomic read per call site and builds
no payload. Adding a hook costs one Tokio task per hook per event.

## Ordering

Hooks on one event run concurrently, not in sequence. Two hooks that both want
to write to the same file need to sort that out between themselves.

Registrations are reversible: `plugin.remove` unwinds everything a plugin
registered, including its hooks, and stops its hook process.

## A hook in a sandbox

A plugin that only exists to answer hooks can run as a WebAssembly component
inside the core rather than as a process. The hook reaches the singleton that is
already running, not a second copy of the plugin, and it is held to the same
`timeout_ms` plus a fuel allowance of its own. `plugins/min-hold` is the worked
example: it answers `take.before` with `{allow: false, reason}` inside an eight
second window. See [plugins as WebAssembly components](wasm.md).

