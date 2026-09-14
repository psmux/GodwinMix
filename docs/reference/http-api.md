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

A scheduled take takes `at_running_time_ms`, armed on the pipeline clock so it
lands on the intended frame rather than whenever the request happened to arrive.

A take is also subject to the rules in [safety.md](safety.md): a minimum hold,
a rate limit and the ITU-R BT.1702-3 flash guard. A refusal is HTTP 429 with
`-32003`, `data.rule` and `data.retry_after_ms`.

With `[control] token` set (or `GODWINMIX_TOKEN` in the environment) every
request carries `Authorization: Bearer <token>`. `GET` requests and the
WebSocket also accept `?token=`, so an `<img>` tag can fetch a snapshot.

