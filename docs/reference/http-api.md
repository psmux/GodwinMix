# The HTTP API

Every route the mixer serves. The UI, `gmx ctl` and the MCP server all use
these and nothing else. A generated `openapi.json` and a `protocol.md` from
`godwinmix --api-info` are planned; this page is written by hand until then.


| | |
|---|---|
| `GET /api/status` | full snapshot |
| `GET /api/media` | ad clips found in the configured library |
| `POST /api/take` | `{"source": "cam1"}`, or `{"source": null}` for black |
| `POST /api/sources` | add a source at runtime: `{"id","uri","name","kind","superimpose"}` |
| `DELETE /api/sources/{id}` | remove one |
| `POST /api/sources/{id}/audio` | `{"gain","muted"}` on any source; `{"page","media"}` only on a superimposed one, 409 otherwise. Every field optional, and only what is named moves. Answers with every value read back off the elements |
| `POST /api/adbreak` | `{"uri": "/path/to/ad.mp4"}`, optional `at_running_time_ms` and `return_to` |
| `POST /api/adbreak/end` | cut the ad short and return early |
| `GET /api/outputs` | destinations and their state |
| `POST /api/outputs` | add a destination: `{"id","uri","policy"}` |
| `DELETE /api/outputs/{id}` | stop sending to one |
| `POST /api/outputs/{id}/reconnect` | force a reconnect |
| `POST /api/shutdown` | stop the mixer; what the desktop app's "Quit and stop the mixer" sends |
| `POST /api/v1/core/restart` | `core.restart`, admin scope. On a mixer started with `--supervised` it answers `{"restarting": true, "how": "supervised", "message"}` and exits with status 75 for its supervisor to start it again. On one started by hand it answers `{"restarting": false, "how": "none", "message"}`, the message saying how to restart it, and keeps running. `core.info` carries `supervised` and `restart: {possible, how}` so a page can decide before it asks. See [restart the mixer](../how-to/restart-the-mixer.md) |
| `GET /api/agent/state` | the state a language model needs, compact: programme, sources with their state and motion, outputs, a snapshot URL pattern. `?response_format=detailed` adds the audio peak per source, the last five takes and the safety limits in force. See [agent-state.md](agent-state.md) |
| `POST /api/v1/program/revert` | take back to the shot before this one |
| `GET /api/v1/program/history` | the last hundred takes, newest first, each with the token that asked |
| `GET /api/v1/tasks` | every background job, newest first |
| `GET /api/v1/tasks/{id}` | one job: `state`, `progress`, `result`, `error`. See [tasks.md](tasks.md) |
| `POST /api/v1/task/cancel` | ask one to stop, cooperatively |
| `GET /api/snapshot/sheet.jpg` | every source and the programme in one mosaic JPEG; `?width=N` scales it |
| `GET /api/snapshot/program.jpg` | the programme alone |
| `GET /api/snapshot/{source_id}.jpg` | one source alone |
| `POST /api/golive` | `{"url", "rtmp", "superimpose", "id"}`: add a page as a source, add the destination, take it once live; answers 202 |
| `GET /ws` | JSON events and state, plus mosaic JPEGs as binary frames |

The `/api/v1` routes are generated from the method table rather than written
out, so `GET /api/v1/core/api` is the list and `godwinmix --api-info` prints the
same thing without a mixer. The ones the how to pages reach for:

| | |
|---|---|
| `POST /api/v1/sources` | the whole of `source.add`: `{"id","uri","name","type","place","latency_ms","params"}`. `uri` is required, and the type id goes in it when a source has no address of its own |
| `POST /api/v1/sources/{id}/duplicate` | `source.duplicate`: `{"name","new_id"}`, both optional. Another source with the same address and settings, which `source.add` cannot do from the shortened address a client is shown |
| `POST /api/v1/sources/{id}/restore` | `source.restore`: puts back one of the last sixteen sources `source.remove` took away, with its id, fader and mute |
| `POST /api/v1/outputs` | `output.add`: `{"id","uri","policy"}` |
| `POST /api/v1/outputs/{id}/set` | `output.set`: `{"uri","policy","queue_secs"}`, naming only what moves. `uri` is write only and an absent one keeps the address in force, which is how a stream key is replaced without anything ever reading it back. See [stream to a platform](../how-to/stream-to-a-platform.md) |
| `POST /api/v1/tool/call` | `tool.call`: `{"name","arguments"}`, the name being `<plugin>/<tool>` |
| `GET /api/v1/plugins/{id}/settings` | a plugin's settings, and `POST` with `{"settings":{...}}` changes the keys it names |

A scheduled take takes `at_running_time_ms`, armed on the pipeline clock so it
lands on the intended frame rather than whenever the request happened to arrive.

A take is also subject to the rules in [safety.md](safety.md): a minimum hold,
a rate limit and the ITU-R BT.1702-3 flash guard. A refusal is HTTP 429 with
`-32003`, `data.rule` and `data.retry_after_ms`.

Every route that asks the mixer something waits five seconds for it and no
longer. A mixer whose command loop is held answers rather than hanging:
`/api/status` and `/metrics` give 503 with the name of the command holding the
loop and how long it has been there, and `/api/v1` gives `-32001` with
`data.command`, `data.held_ms` and `data.retryable`. The same wedge writes one
error line into the log naming that command, so a CI run or an operator can
see which one it was. A page that loads while this is happening shows the
warning instead of drawing nothing.

With `[control] token` set (or `GODWINMIX_TOKEN` in the environment) every
request carries `Authorization: Bearer <token>`. `GET` requests and the
WebSocket also accept `?token=`, so an `<img>` tag can fetch a snapshot.

