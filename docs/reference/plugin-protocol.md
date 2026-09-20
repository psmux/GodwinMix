# The plugin protocol

One protocol, JSON-RPC 2.0, with the same methods and types whether the peer is
a plugin on stdio, a UI on a WebSocket, a remote node, the CLI or the MCP
server. This page covers the plugin side: the framing, the handshake, every
method the core calls on a plugin, and the errors.

This page is written by hand today. It will be generated from the protocol crate
when that crate carries the plugin types, and this note will go.

The implementation it describes is
[`crates/godwinmix-sdk/src/wire.rs`](../../crates/godwinmix-sdk/src/wire.rs),
[`framing.rs`](../../crates/godwinmix-sdk/src/framing.rs) and
[`handshake.rs`](../../crates/godwinmix-sdk/src/handshake.rs). Where the two
disagree, the crate has a bug.

## Transports, per peer

| Peer | Transport |
|---|---|
| sidecar or node plugin | stdin for core to plugin, stderr for plugin to core. stdout is media in container mode; in raw mode stdout is inherited by the core and logged at debug |
| UI, service, node | a WebSocket at `/rpc`, text frames, one JSON-RPC message per frame |
| local tools | a Unix socket or named pipe, at the path `godwinmix --info` prints |
| MCP client | stdio or Streamable HTTP; the MCP server is a thin adapter over the same methods |
| curl, `<img>` | the REST layer, generated from the same methods |

An in process plugin makes the same exchange as Rust function calls. The core
never knows which it is talking to.

## Framing on stdio

* UTF-8, one JSON object per line, terminated by `\n`. A message never contains
  a raw newline, because JSON escapes them. A `\r` before the newline is
  stripped, so a plugin driven from a Windows shell still parses.
* At most 4 MiB (4,194,304 bytes) per line. A line at exactly the limit is
  accepted; one byte more is error -32011, the channel closes and the instance
  goes to `failed`. The SDK refuses to send an over long line as well, rather
  than writing one it would reject.
* A line that is not a JSON object is a log line, not a protocol error. The core
  forwards it to its log at `info`, tagged with the instance, so a Python
  traceback or a library warning lands in the log rather than breaking the
  channel. The SDK does the same with a non JSON line arriving on stdin:

```
{"jsonrpc":"2.0","method":"log","params":{"level":"debug","message":"ignored a non JSON line on stdin: this line is not JSON at all"}}
```

* Both directions may have several requests in flight, and answers may come back
  in any order.
* Request ids are per direction. The core's id 1 and the plugin's id 1 are
  different things, and a response is matched by id within its own direction.
  The SDK's writer counts its own ids from 0.
* A plugin must keep answering `health` while a slow `start` or `configure` is
  pending. The SDK's reader thread answers `health` from a cached value without
  touching the plugin, and the worker thread refreshes that value after every
  call, so the rule holds without the plugin author doing anything.

Structured logging is a notification, not stderr text:

```
{"jsonrpc":"2.0","method":"log","params":{"level":"info","message":"colour bars at 160x90@30 for instance 'bars'"}}
```

## The handshake

The plugin speaks first. Three messages, then the channel is open.

```
plugin -> core   {"jsonrpc":"2.0","id":0,"method":"initialize","params":{"plugin":"my-cam","version":"0.1.0","api":1,"transports":["container"],"provides":[{"kind":"source","id":"source","transports":["container"],"media":{"video":"raw","audio":"none","alpha":false,"thumb":true},"capabilities":["restart-in-place","health"],"latency_ms":0,"settings":"schemas/source.json","skill":"skills/source/SKILL.md"}]}}

core -> plugin   {"jsonrpc":"2.0","id":0,"result":{"core":"godwinmix","version":"0.0.0","api_level":1,"api_compatible":1,"canvas":{"width":160,"height":90,"fps":30},"transport":"container","media":"","instance":"cam","provide":"source","params":{"bars":8}}}

plugin -> core   {"jsonrpc":"2.0","method":"initialized","params":{}}
```

Those three lines are from a real run of the Python template, driven by hand.

The core kills a plugin that has not sent `initialize` within 5 seconds, or that
names an `api` outside the range it serves, and says why in
`event/plugin.state`.

The plugin checks the answer in the other direction before trusting it. The SDK
refuses, with a message carrying the numbers, when:

* `api` is outside `api_compatible` to `api_level`.
* the canvas has a zero width, height or fps.
* the transport is `unixfd` or `shm` and `media` is empty, because neither can
  be opened without an address.

## The state machine

```
              initialize ok            start
  starting  ------------->  ready  ------------>  running  <----+
     |                       ^                     |  |         | health says
     |                       |                     |  |         | degraded, or ok
  timeout, bad api,          |       stop          |  |         v
  exit                       +---------------------+  |     degraded
     |                                                |
     v                                                | no buffers for stall_timeout
  failed  <---- exit, crash                           v
                                                   stalled
```

`stopped` follows the same rules as `ready`, and `start` may follow it.
`configure` never changes state. `shutdown` is legal from every state but
`failed`, and leads to process exit.

### Legal call order

| State | The core may call | The plugin may send |
|---|---|---|
| `starting` | nothing; it is waiting for `initialize` | `initialize` |
| `ready` | `configure`, `start`, `health`, `discover`, `tool.call`, `shutdown` | `log`, `event/<name>`, core methods over `GMX_RPC` |
| `running` | `configure`, `stop`, `health`, `seek`, `position`, `keyframe`, `audio.set`, `render`, `tool.call`, `discover`, `shutdown` | `log`, `event/<name>`, `media.report`, `health.changed` |
| `stalled` | the same as `running` | the same as `running` |
| `degraded` | the same as `running` | the same as `running` |
| `stopped` | the same as `ready` | `log` |
| `failed` | nothing; the supervisor decides | nothing is read |

A plugin in trouble still answers everything, which is how the supervisor gets
it back.

Anything outside the table is refused with -32001, and the refusal names the
state, what is legal now, and the next step:

```
{"jsonrpc":"2.0","id":1,"error":{"code":-32001,"message":"'stop' is not legal in state 'ready'. Legal now: configure, start, health, discover, tool.call, shutdown. Call start first.","data":{"legal":["configure","start","health","discover","tool.call","shutdown"],"method":"stop","retryable":true,"state":"ready"}}}
```

`configure` before `start` is legal, and is how the first settings arrive when
they change between the handshake and the source going live.

## Methods the core calls on a plugin

Lines marked **recorded** are copied from real runs of the Python template and
the Rust `colour-bars` example, driven by hand. The rest are illustrations of
the shape: no core implements those calls yet.

### `initialize`

The plugin sends this one, before anything else. Its params are the plugin's
side of the handshake; the result is the core's.

| Param | Type | Meaning |
|---|---|---|
| `plugin` | string | The manifest's `plugin.name` |
| `version` | string | The manifest's `plugin.version` |
| `api` | integer | The protocol level this plugin was written against |
| `transports` | array of strings | What this plugin can speak, in preference order |
| `provides` | array of objects | The `[[provides]]` blocks, verbatim, so the core need not read the file twice |

| Result | Type | Meaning |
|---|---|---|
| `core` | string | The core's name |
| `version` | string | The core's version |
| `api_level` | integer | The highest api this core serves |
| `api_compatible` | integer | The lowest api this core serves |
| `canvas` | object | `{width, height, fps}`. Every frame is at these caps |
| `transport` | string | The one the core chose from your list |
| `media` | string | The `unixfd` or `shm` address. Empty in container mode |
| `instance` | string | The instance id, a slug such as `cam1` |
| `provide` | string | Which of your provides this process serves |
| `params` | object | The settings, already validated against your schema |

**recorded**

```
{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"api":1,"plugin":"colour-bars","provides":[{"capabilities":["restart-in-place","health"],"id":"source","kind":"source","latency_ms":0,"media":{"alpha":false,"audio":"none","thumb":true,"video":"raw"},"settings":"settings.json","transports":["container"]}],"transports":["container"],"version":"0.1.0"}}
```

### `initialized`

A notification from the plugin, with no params and no answer. The handshake is
complete once it is out.

**recorded**

```
{"jsonrpc":"2.0","method":"initialized","params":{}}
```

### `shutdown`

| Param | Type | Meaning |
|---|---|---|
| `reason` | string | Why, for the log |

Result: `{}`. The process exits within 8 seconds or is killed. Legal from every
state but `failed`.

**recorded**

```
{"jsonrpc":"2.0","id":7,"result":{}}
```

### `configure`

| Param | Type | Meaning |
|---|---|---|
| `params` | object | The full validated settings object, never a diff |

| Result | Type | Meaning |
|---|---|---|
| `applied` | boolean | True when the new settings are live |
| `restart_required` | boolean | Present and true when they are not |
| `reason` | string | Why a restart is needed, in words an operator can act on |

The core turns `{applied: false, restart_required: true, reason}` into error
-32012 for the caller, with the reason in it. A plugin never crashes here.

**recorded**

```
{"jsonrpc":"2.0","id":3,"method":"configure","params":{"params":{"bars":4}}}
{"jsonrpc":"2.0","id":3,"result":{"applied":true}}
```

### `health`

Params: `{}`.

| Result | Type | Meaning |
|---|---|---|
| `state` | string | `ok`, `degraded` or `failing` |
| `detail` | string | Optional. What is wrong, or what is going well |
| `latency_ms` | integer | Optional. What the plugin is adding now |

Answered while other calls are in flight. `degraded` raises an alert before a
stall, for a plugin that declared the `health` capability.

**recorded**

```
{"jsonrpc":"2.0","id":2,"result":{"state":"ok","detail":"1 frames sent","latency_ms":0}}
```

### `start`

Kinds: source, output, filter.

| Param | Type | Meaning |
|---|---|---|
| `canvas` | object | `{width, height, fps}` |
| `transport` | string | `unixfd`, `shm` or `container` |
| `media` | string | The address for the first two, empty for the third |

| Result | Type | Meaning |
|---|---|---|
| `latency_ms` | integer | Optional. What this plugin adds |

Return as soon as the producer is running. Do not block until the first frame.

**recorded**

```
{"jsonrpc":"2.0","id":1,"method":"start","params":{"canvas":{"width":160,"height":90,"fps":30},"transport":"container","media":""}}
{"jsonrpc":"2.0","id":1,"result":{"latency_ms":0}}
```

### `stop`

Kinds: source, output, filter. Params: `{}`. Result: `{}`.

Stop producing and release the transport. `start` may follow.

**recorded**

```
{"jsonrpc":"2.0","id":6,"result":{}}
```

### `seek`

Kinds: a source that declared the `seek` capability.

| Param | Type | Meaning |
|---|---|---|
| `position_ms` | integer | Where to go |

| Result | Type | Meaning |
|---|---|---|
| `position_ms` | integer | Where it landed |
| `duration_ms` | integer | Optional. The whole length, when it is known |

A plugin that has not declared the capability answers -32601 naming it:

**recorded**

```
{"jsonrpc":"2.0","id":2,"error":{"code":-32601,"message":"this plugin does not implement 'seek'. Implement it and add 'seek' to capabilities in gmx-plugin.toml; the core only calls it when that is declared.","data":{"capability":"seek","method":"seek","retryable":false}}}
```

### `position`

Kinds: a source that declared `seek`. Params: `{}`. The result is the same shape
as `seek`.

Illustration:

```
{"jsonrpc":"2.0","id":9,"result":{"position_ms":4000,"duration_ms":183000}}
```

### `keyframe`

Kinds: a source or output that declared `keyframe-request`. Params: `{}`.
Result: `{}`.

A downstream client asked for a keyframe. Without the capability the core
answers the request from its own encoder GOP instead.

**recorded**

```
{"jsonrpc":"2.0","id":4,"result":{}}
```

### `audio.set`

Kinds: source. Gain is in decibels and 0 is unity.

| Param | Type | Meaning |
|---|---|---|
| `gain_db` | number | Optional |
| `muted` | boolean | Optional |
| `layers` | object | Optional, and only with the `audio-layers` capability: `{page, media}` where `media` is an array of numbers or nulls |

| Result | Type | Meaning |
|---|---|---|
| `gain_db` | number | The full state read back |
| `muted` | boolean | The full state read back |
| `layers` | object | Present when the plugin has layers |

Illustration:

```
{"jsonrpc":"2.0","id":11,"method":"audio.set","params":{"gain_db":-6,"muted":false}}
{"jsonrpc":"2.0","id":11,"result":{"gain_db":-6,"muted":false}}
```

### `render`

Kinds: transition. Called at the compositor's frame rate, so it must answer
fast.

| Param | Type | Meaning |
|---|---|---|
| `from` | array of strings | The pad ids being left |
| `to` | array of strings | The pad ids being arrived at |
| `progress` | number | 0 to 1 |
| `running_time_ns` | integer | The compositor's running time |

The result is `{pads: {...}}`, where each pad takes `alpha` and optionally
`xpos`, `ypos`, `width`, `height` and `volume`, and the core applies them. A
plugin may instead answer once with `{curve: ...}` for the core to bind as a
control source, which costs nothing per frame.

Illustration:

```
{"jsonrpc":"2.0","id":12,"method":"render","params":{"from":["cam1"],"to":["cam2"],"progress":0.5,"running_time_ns":4000000000}}
{"jsonrpc":"2.0","id":12,"result":{"pads":{"cam1":{"alpha":0.5},"cam2":{"alpha":0.5}}}}
```

### `discover`

Kinds: device. Answer within the timeout; the core will not wait longer.

| Param | Type | Meaning |
|---|---|---|
| `timeout_ms` | integer | How long to look |

| Result | Type | Meaning |
|---|---|---|
| `candidates` | array | Each `{type, name, params, confidence}` |

`type` is a provide id such as `ndi/source`, `params` is ready to pass to
`source.add`, and `confidence` is 0 to 1.

Illustration:

```
{"jsonrpc":"2.0","id":13,"result":{"candidates":[{"type":"ndi/source","name":"CAM 1 (Studio)","params":{"url":"ndi://CAM 1 (Studio)"},"confidence":0.95}]}}
```

### `tool.call`

Kinds: any plugin with a `[[tools]]` block.

| Param | Type | Meaning |
|---|---|---|
| `name` | string | The tool name, unprefixed |
| `arguments` | object | Validated against the tool's input schema |

| Result | Type | Meaning |
|---|---|---|
| `content` | array | MCP content blocks |
| `structuredContent` | object | Optional, matching the tool's output schema |
| `isError` | boolean | Optional |

Illustration:

```
{"jsonrpc":"2.0","id":14,"method":"tool.call","params":{"name":"list_senders","arguments":{}}}
{"jsonrpc":"2.0","id":14,"result":{"content":[{"type":"text","text":"1 sender"}],"structuredContent":{"senders":[{"name":"CAM 1 (Studio)"}]}}}
```

## Notifications a plugin may send

None of these want an answer, and none carries an id.

| Method | Params | When |
|---|---|---|
| `log` | `{level, message}` where level is `trace`, `debug`, `info`, `warn` or `error` | any time. It lands in the core's log tagged with the instance, and in the last fifty lines of a crash report |
| `event/<name>` | whatever the event carries | any time. The name after `event/` is what subscribers match on |
| `media.report` | what the plugin is actually producing, for example `{"latency_ms":0}` | after `initialize`, and whenever the answer changes |
| `health.changed` | the same shape as the `health` result | when health moves and the core should act rather than wait to be asked |

**recorded**

```
{"jsonrpc":"2.0","method":"media.report","params":{"latency_ms":0}}
{"jsonrpc":"2.0","method":"log","params":{"level":"debug","message":"media loop finished after 48 frames, 0 late"}}
```

## Error codes

One shape everywhere: `{code, message, data}`. The message names the current
state and the next step, because that is what an unattended caller needs.

| Code | Meaning | Retryable | What `data` carries |
|---|---|---|---|
| -32700 | parse error | no | |
| -32600 | invalid request | no | |
| -32601 | method not found | no | `method`, `retryable`, and `capability` when the method exists but the plugin has not declared it |
| -32602 | invalid params | no | the parse error in the message |
| -32603 | internal error | no | |
| -32001 | not in a state that allows this | yes, after the named event | `state`, `method`, `legal`, `retryable` |
| -32002 | refused by scope | no | |
| -32003 | refused by safety, such as `min_hold_ms` or the rate limit | yes, after the wait | `retry_after_ms` |
| -32004 | not found: an id, a plugin, a node | no | |
| -32005 | placement not declared by the plugin | no | `placements` |
| -32010 | the plugin died during the call | yes, once the supervisor restarts it | |
| -32011 | line too long | no | `bytes`, `limit`, `retryable` |
| -32012 | restart required, from `configure` | no; call `plugin.reload` | the `reason` from `configure` |
| -32020 | confirmation required | yes, with the token | `confirm_token`, valid for 30 seconds |

Two real refusals, one for a method the plugin does not have at all and one for
a method it has not declared:

```
{"jsonrpc":"2.0","id":5,"error":{"code":-32601,"message":"this plugin has no method 'teleport'. Check the spelling against docs/reference/plugin-protocol.md, or implement `call` to handle it.","data":{"method":"teleport","retryable":false}}}
{"jsonrpc":"2.0","id":3,"error":{"code":-32601,"message":"this plugin does not implement 'audio.set'. Implement it and add 'audio-layers' to capabilities in gmx-plugin.toml; the core only calls it when that is declared.","data":{"capability":"audio-layers","method":"audio.set","retryable":false}}}
```

## Recorded transcripts

A transcript is a JSONL file that records one conversation, so it can be
replayed against the plugin with no core, no sockets and no clock. It is what
the Python template's `./check` runs, and what `gmx plugin test --offline` will
run when it exists.

Rules:

* One JSON object per line, with exactly one key.
* `core` is a line written to the plugin's stdin.
* `plugin` is a line that must appear on the plugin's stderr, in this order.
* A `plugin` line matches as a subset: the real line must carry the keys named
  with the values named, and may carry anything else. Lines the transcript does
  not mention are skipped, so adding a log line does not break a test.
* `"*"` as a value matches whatever is there, which is how a generated id or a
  timestamp is allowed to vary.
* Arrays must be the same length and match element by element.
* A line starting `#` or `//`, and a blank line, is a comment. Line numbers are
  kept so an error can name one.

Every step is `{"core": ...}` or `{"plugin": ...}`; a line with both keys, or
neither, is a parse error naming its line number.

This is `tests/transcript.jsonl` from the Python template, with the name filled
in:

```jsonl
# One recorded conversation with the core. `core` lines are written to the
# plugin's stdin; `plugin` lines must appear on its stderr, in this order, as a
# subset of the real line. "*" matches any value.
{"plugin": {"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {"plugin": "my-cam", "api": 1, "transports": ["container"]}}}
{"core": {"jsonrpc": "2.0", "id": 0, "result": {"core": "godwinmix", "version": "0.2.0", "api_level": 1, "api_compatible": 1, "canvas": {"width": 160, "height": 90, "fps": 30}, "transport": "container", "media": "", "instance": "test", "provide": "source", "params": {"bars": 8}}}}
{"plugin": {"jsonrpc": "2.0", "method": "initialized"}}
{"core": {"jsonrpc": "2.0", "id": 1, "method": "start", "params": {"canvas": {"width": 160, "height": 90, "fps": 30}, "transport": "container", "media": ""}}}
{"plugin": {"jsonrpc": "2.0", "id": 1, "result": {"latency_ms": 0}}}
{"core": {"jsonrpc": "2.0", "id": 2, "method": "health", "params": {}}}
{"plugin": {"jsonrpc": "2.0", "id": 2, "result": {"state": "ok"}}}
{"core": {"jsonrpc": "2.0", "id": 3, "method": "configure", "params": {"params": {"bars": 4}}}}
{"plugin": {"jsonrpc": "2.0", "id": 3, "result": {"applied": true}}}
{"core": {"jsonrpc": "2.0", "id": 4, "method": "teleport", "params": {}}}
{"plugin": {"jsonrpc": "2.0", "id": 4, "error": {"code": -32601, "message": "*"}}}
{"core": {"jsonrpc": "2.0", "id": 5, "method": "stop", "params": {}}}
{"plugin": {"jsonrpc": "2.0", "id": 5, "result": {}}}
{"core": {"jsonrpc": "2.0", "id": 6, "method": "shutdown", "params": {"reason": "offline test"}}}
{"plugin": {"jsonrpc": "2.0", "id": 6, "result": {}}}
```

Notice the `"*"` on the `-32601` message. The test asserts that an unknown
method is refused with the right code, and leaves the wording free to improve.

The Rust side of this is `godwinmix_sdk::transcript`: `steps` parses the file,
`matches` does the subset comparison and `explain` formats a failure.
[Write a source plugin in Rust](../how-to/write-a-source-plugin.md) has a
`cargo test` that replays one in process.

## See also

* [The plugin manifest](plugin-manifest.md), every key of `gmx-plugin.toml`.
* [Write a source plugin in Rust](../how-to/write-a-source-plugin.md).
* [Your first plugin](../tutorials/your-first-plugin.md).

### Rust callback panics

The Rust SDK answers a request whose callback unwinds with `PLUGIN_DIED` and
error data containing `method`, `retryable: false` and `restart_required: true`.
It marks cached health `failing`. Later requests receive an error without
calling the damaged handler; shutdown is acknowledged. The instance must be
restarted before another operation can run. The crash hook still records the
original panic. This does not intercept process aborts or native crashes.
