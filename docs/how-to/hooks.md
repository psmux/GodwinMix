# Run your own code when something happens

A hook is your code, called by the mixer when a thing happens. Tally lights on
a take. A message to the duty engineer when a destination drops. A rule that
refuses to put a camera on air if it has no sound on it.

Hooks never touch the picture. The compositor keeps producing frames whatever
your hook is doing, and the one hook that can hold anything up holds up a
decision for at most 20 milliseconds. The rest of this page is the detail.

## The shortest thing that works

Save this as `receiver.py` and run it. Ten lines, standard library, no
dependencies.

```python
import http.server, json

class Hook(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers["content-length"])))
        print(body["hook"], json.dumps(body["payload"]))
        self.send_response(200); self.send_header("content-length", "0"); self.end_headers()

http.server.HTTPServer(("127.0.0.1", 8787), Hook).serve_forever()
```

Then add three lines to `godwinmix.toml` and restart:

```toml
[[hooks]]
event = "take.after"
http = "http://127.0.0.1:8787/on-take"
```

Take something. The receiver prints:

```
take.after {"source":"cam-wide","at_running_time_ms":58402,"by":"desk","revert":false}
```

That is the whole of it. Everything below is other events, other ways to be
called, and the one hook that can say no.

## What arrives

Every hook gets the same envelope, whichever way it is called:

```json
{
  "hook": "take.after",
  "ts": "2026-09-14T20:13:58.402Z",
  "payload": { "source": "cam-wide", "at_running_time_ms": 58402, "by": "desk", "revert": false }
}
```

`hook` is in the body as well as in the URL, because one receiver usually
serves several events.

## The events

| Event | When | Payload |
|---|---|---|
| `take.before` | before a take is applied | `source`, `at_running_time_ms`, `by`, `revert` |
| `take.after` | once it has landed | the same |
| `source.added` | a source was added | `source`, `uri`, `state` |
| `source.removed` | a source was removed | `source`, `uri` |
| `source.state` | a source connected, went live, stalled or failed | `source`, `state` |
| `output.state` | a destination connected, dropped or is retrying | `output`, `state`, `reconnects` |
| `alert.raised` | anything the operator should see | `severity`, `message` |
| `session.start` | the daemon came up | `version`, `bind`, `log` |
| `session.end` | it is going down | `version` |
| `plugin.loaded` | a plugin was installed and read | `plugin`, `version`, `provides` |
| `plugin.failed` | one would not load | `plugin`, `reason` |
| `plugin.state` | either of those, as one event | `plugin`, `state` |

Only `take.before` can delay anything. The other eleven are told after the
fact and nothing waits for them.

## The three ways to be called

### `http`: a URL

For a hook with no plugin behind it. Written in the operator's config file:

```toml
[[hooks]]
event = "output.state"
http = "https://ops.example/gmx/output"
```

The mixer POSTs the envelope. For `take.before` it reads the answer; for
everything else it does not read the body at all, so a receiver can return
whatever it likes.

### `command`: a program

The event arrives as one line of JSON on standard input. `GMX_HOOK` in the
environment is the hook name.

```toml
[[hooks]]
event = "take.after"
command = "/usr/local/bin/tally.sh"
```

```sh
#!/bin/sh
# tally.sh
source=$(python3 -c 'import json,sys; print(json.load(sys.stdin)["payload"]["source"] or "")')
logger "gmx: $source is on air"
```

A `take.before` command refuses a take by exiting 2, or by printing
`{"allow": false, "reason": "..."}` on stdout. Anything else lets it through.

### `rpc`: a plugin

A plugin asks for its own hooks in `gmx-plugin.toml` and gets a JSON-RPC call
on the channel it already speaks:

```toml
[hooks]
"take.before" = { mode = "rpc", timeout_ms = 19 }
"take.after"  = { mode = "rpc" }
```

The method is `hook` and its params are the envelope. Answer `{"allow": false,
"reason": "..."}` to refuse a take, or `{}` to let it through.

The plugin is started once, the first time one of its hooks fires, and kept.
Read `docs/reference/hooks.md` before you write one: the hook process is a
second instance of your plugin, not the one already running as a source, and
that matters if the hook needs to see the source's state.

## Refusing a take

`take.before` is the one hook that can say no. This is how you make a rule the
core does not have: no take to a camera whose audio has been silent for a
minute, no take to the stage camera during the sermon, no take at all from an
agent token between 11:00 and 11:20.

```python
def on_take_before(payload):
    if payload["source"] == "cam-stage" and during_the_sermon():
        return {"allow": False, "reason": "cam-stage is off limits during the sermon"}
    return {"allow": True}
```

The refusal reaches the caller as a normal error, with the reason in it:

```
-32003  a take.before hook refused this take: http://127.0.0.1:8787/on-take:
        cam-stage is off limits during the sermon. The programme is unchanged.
        Take a different source, or turn the hook off in the config.
```

Two things are deliberate here.

**Silence is consent.** A hook that answers `{}`, or nothing, or something that
is not JSON, has not refused. Only an explicit `{"allow": false}` stops a take.
A hook that refuses by accident takes a programme off air, and the failure mode
has to be the other way round.

**Late is the same as silent.** See the next section.

## The timeout rule

`take.before` gets 20 milliseconds by default. You can ask for more with
`timeout_ms`, up to 100. Nothing else has a timeout, because nothing else is
waited for.

A hook that has not answered by then does not delay the take. Its answer is
abandoned, the take goes ahead, and `event/hook.blocked` is published:

```json
{"hook": "take.before", "plugin": "http://127.0.0.1:8787/on-take",
 "reason": "no answer within 20 ms, so the take went ahead without it. Raise timeout_ms (up to 100 ms) or move the work off the hook."}
```

Watch for it. A hook that has been timing out all afternoon is a rule that has
not been applied all afternoon, and the event is the only thing that says so.

Why 20 ms: at 30 frames a second a frame is 33 ms, so a hook that answers
inside 20 ms costs less than one frame and a take armed for a frame still lands
on it. 100 ms is three frames, which an operator can feel, which is why the
manifest validator refuses more.

Why any limit at all: a hook is somebody else's code, and somebody else's code
stops responding. The rule that it never gets to delay a take by more than
`timeout_ms` is what lets you add one to a live mixer without thinking about
it.

## Several hooks on one event

They all run, at the same time, and the wait is the longest timeout any of them
asked for rather than the sum. The first refusal wins; the take is refused once
and the reason names which hook refused it.

## When a hook goes wrong

Anything that is not an answer is a `hook.blocked` event and a warning in the
log: a URL that will not resolve, a 500, a command that is not installed, a
plugin that will not start. The thing the hook was attached to goes ahead
anyway. There is no configuration to make a broken hook stop the show, and
there will not be.

```sh
gmx logs --since 20:00 | grep hook          # what has been failing
gmx session show session.jsonl --from 20:00 # hook.blocked in the timeline
```

## What it costs

Nothing, until you configure one. A core with no hooks does one atomic read at
each call site and builds no payload. A core with hooks spawns one task per
hook per event, and the task is on the control plane, never on the mixer thread
and never on a GStreamer streaming thread.

## See also

* [`docs/reference/hooks.md`](../reference/hooks.md) for every field, the
  refusal shapes, and what `rpc` mode does not do yet.
* [`docs/reference/session-log.md`](../reference/session-log.md) for
  `hook.blocked` in the log.
* [`docs/reference/plugin-manifest.md`](../reference/plugin-manifest.md) for
  `[hooks]` in a plugin.
