# Control the mixer

Everything the mixer can do is one set of methods. This page gets a source on
air from a terminal, then shows the same thing over the WebSocket a UI uses.

## Two minutes to a source on air

Start a mixer with the example config and leave it running:

```sh
godwinmix --example-config > godwinmix.toml
godwinmix
```

In another terminal, add a source and take it:

```sh
curl -s -X POST localhost:8080/api/v1/sources \
  -H 'content-type: application/json' \
  -d '{"uri":"rtmp://127.0.0.1:1935/live/cam1","name":"Camera 1"}'
```

```json
{"id":"camera-1","name":"Camera 1","state":"connecting","has_video":false, ...}
```

The id comes back with the whole record, so there is nothing to look up. Wait
for `"state":"live"`, then put it on air:

```sh
curl -s -X POST localhost:8080/api/v1/program/take \
  -H 'content-type: application/json' -d '{"source":"camera-1"}'
```

```json
{"program":"camera-1","running_time_ms":18320,"previous":null,"should_retry":true}
```

Every mutating call answers with the resulting object, so you never have to
read the state back to find out what happened.

The CLI does the same thing with less typing:

```sh
gmx ctl source add - rtmp://127.0.0.1:1935/live/cam1 --name "Camera 1"
gmx ctl take camera-1
gmx ctl status
```

## The paths

Method names are `noun.verb` and the path follows from the name. `list`,
`get`, `add` and `remove` map onto the collection; everything else hangs off
it.

| Method | Route |
|---|---|
| `source.list` | `GET /api/v1/sources` |
| `source.get` | `GET /api/v1/sources/{id}` |
| `source.add` | `POST /api/v1/sources` |
| `source.remove` | `DELETE /api/v1/sources/{id}` |
| `source.audio.set` | `POST /api/v1/sources/{id}/audio` |
| `program.take` | `POST /api/v1/program/take` |
| `core.info` | `GET /api/v1/core/info` |

`GET /api/v1/core/api` returns the whole table as JSON Schema, and so does
`godwinmix --api-info` without a running mixer. `openapi.json` in the
repository is the same thing for Swagger UI and client generators.

The old `/api/...` paths still answer and carry a `Deprecation: true` header.
They go one release after this one.

## The token

No token in the config leaves the control port open, which is fine on a LAN
nobody else is on. Otherwise:

```toml
[control]
token = "a-long-random-string"
```

```sh
curl -H 'Authorization: Bearer a-long-random-string' localhost:8080/api/v1/core/info
gmx ctl --token a-long-random-string status     # or GODWINMIX_TOKEN=...
```

A GET may carry it as `?token=` instead, because a browser opening a WebSocket
and an `<img src>` cannot set a header. A POST may not: a token in a URL ends
up in more logs than it should.

Several tokens, each with its own scopes, go in a `[[tokens]]` table:

```toml
[[tokens]]
id = "studio-agent"
secret = "..."
scopes = ["read", "operate"]   # read < operate < admin
confirm = "required"           # destructive calls need a confirm round trip
rehearsal = false              # true is accepted only by `godwinmix --rehearsal`
profile = "minimal"            # which MCP tool surface this token is meant for
```

`read` can look, `operate` can run a show, `admin` can change the machine. A
call above the token's scope is refused with `-32002` before it runs.

## When something is refused

One shape, everywhere:

```json
{
  "error": {
    "code": -32004,
    "message": "there is no source 'cam9'. The sources: cam1, cam2. Use one of those.",
    "data": { "id": "cam9", "valid": ["cam1", "cam2"], "retryable": false }
  },
  "trace_id": "0af7651916cd43dd8448eb211c80319c"
}
```

The message names the current state and the next step, and an unknown id lists
the ids that would have worked, so a script can recover without a second call.
`data.retryable` says whether trying again can ever help.

| Code | HTTP | What to do |
|---|---|---|
| -32001 | 409 | Not in a state that allows this. Wait for the event the message names. |
| -32002 | 403 | The token does not carry the scope. Ask for one that does. |
| -32003 | 429 | A safety rule refused it. `data.retry_after_ms` says how long. |
| -32004 | 404 | No such id. `data.valid` lists the ones that exist. |
| -32020 | 428 | Destructive, and this token confirms first. See below. |
| -32602 | 400 | The params were wrong. `core.api` has the schema. |

`trace_id` comes back in the body and in the `X-Trace-Id` header, and is in the
mixer's log line for that call. Send your own as `traceparent` or as
`trace_id` in the body and it is used instead.

## Retries, dry runs and confirmation

A timed out call is not a failed call. Put an `idempotency_key` on anything
that changes state and a retry is free:

```sh
curl -s -X POST localhost:8080/api/v1/sources -H 'content-type: application/json' \
  -d '{"uri":"rtmp://host/live/cam1","idempotency_key":"add-cam1-2026-09-14"}'
```

The answer is kept for 24 hours. The same call under the same key returns the
first answer with `"replayed": true`. The same key with different arguments is
`-32602` with `data.idempotency: "mismatch"`, rather than quietly answering a
question nobody asked.

Anything destructive (`source.remove`, `output.remove`, `media.remove`,
`core.shutdown`) accepts `dry_run`:

```sh
curl -s -X DELETE localhost:8080/api/v1/sources/camera-1 \
  -H 'content-type: application/json' -d '{"dry_run":true}'
```

```json
{"would_change":true,
 "diff":["remove source camera-1 (rtmp://127.0.0.1:1935/…)",
         "cut the programme to the slate, because it is on air"],
 "method":"source.remove","dry_run":true}
```

That is read off the live state, not simulated. A dry run never needs a
confirm token, because it changes nothing.

On a token whose policy is `confirm = "required"`, a real destructive call is
refused once:

```json
{"error":{"code":-32020,
  "message":"source.remove is destructive and token 'desk' is set to confirm = required. Send the same call again with confirm = \"cfm-73d39bbc-0\" within 30 seconds.",
  "data":{"confirm_token":"cfm-73d39bbc-0","expires_in_ms":30000}}}
```

Send it again with `{"confirm": "cfm-73d39bbc-0"}` and it proceeds. The token
works once, for that method, for that credential.

## Live state over `/rpc`

`/api/v1` is for scripts. Anything that wants to watch opens one WebSocket at
`/rpc` and speaks JSON-RPC 2.0, one message per text frame. Every method above
is reachable there by its dotted name.

```python
# pip install websockets
import asyncio, json, websockets

async def main():
    async with websockets.connect("ws://localhost:8080/rpc") as ws:
        await ws.send(json.dumps({"jsonrpc": "2.0", "id": 1, "method": "core.subscribe",
            "params": {"events": ["program.*", "source.*", "alert"],
                       "ext": {"meters": True, "tally": True}}}))
        async for frame in ws:
            if isinstance(frame, bytes):
                continue                      # a mosaic frame; see below
            msg = json.loads(frame)
            if msg.get("method") == "event/snapshot":
                state = msg["params"]["state"]
                print("on air:", state["program"], "sources:", len(state["sources"]))
            elif msg.get("method") == "event/program.took":
                print("took", msg["params"]["source"])
            elif msg.get("method") == "event/flush":
                pass                          # render here, and not before

asyncio.run(main())
```

Add `?token=...` to the URL when the mixer has one.

The core answers `core.subscribe` with the sequence number the snapshot is
current as of, then sends `event/snapshot` and after that only deltas. Every
event carries `seq`. Every batch ends with `event/flush`, so a client paints
whole updates and never half of one. A client that falls behind gets
`event/resync {from_seq, dropped}` followed by a fresh snapshot, rather than a
silent hole.

Nothing expensive runs unless a client asks for it. `ext` is that request:

| Key | Turns on |
|---|---|
| `"meters": true` | `event/meters`, the programme bus and every source, ten a second |
| `"tally": true` | `event/tally`, which source is on air |
| `"positions": true` | `event/source.position` for file sources |
| `"multiview": {"fps": 8, "width": 1280}` | the mosaic: `event/multiview.layout` and binary frames |

Asking for the ext key is asking for the stream; you do not also have to name
it among your event patterns. Keys this build does not implement come back in
`ignored_ext` rather than being refused, so a client written against the whole
table still connects.

Mosaic frames arrive as binary WebSocket frames: sixteen little endian bytes
(`seq` u32, layout id u32, programme running time in milliseconds u64) then
the JPEG. The layout id matches the `id` in the last `event/multiview.layout`,
so a client that fell behind can tell which grid a late frame belongs to.

```python
import struct
seq, layout, running_time_ms = struct.unpack("<IIQ", frame[:16])
jpeg = frame[16:]
```

The old `/ws` still broadcasts everything to everyone with no sequence
numbers, for one release.

## Where to read more

* `protocol.md` at the repository root: every method, event and type, generated.
* `openapi.json`: the REST layer, for Swagger UI and client generators.
* `docs/how-to/use-with-an-ai-agent.md`: the MCP server and the agent surface.
