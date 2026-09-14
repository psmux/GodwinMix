# `agent.state`

The document an agent reads before it decides anything. Every read is charged
for in somebody's context window, so it is measured in bytes and a test fails
the build when it grows.

```
agent.state {"response_format": "concise" | "detailed"}
GET /api/v1/agent/state?response_format=detailed
```

`concise` is the default.

## Concise

```json
{
  "program": "cam1",
  "program_motion": 0.12,
  "uptime_secs": 942,
  "sources": [
    {"id": "cam1", "state": "live", "motion": 0.12},
    {"id": "cam2", "state": "live", "no_audio": true},
    {"id": "clip", "state": "connecting"}
  ],
  "outputs": [{"id": "yt", "state": "live"}],
  "snapshot": "/api/v1/snapshot/{id}"
}
```

Everything at its usual value is left out. That is what keeps sixteen sources
inside the budget without shortening the field names into something nobody can
read.

| Field | Present when |
|---|---|
| `program` | always; `null` is the slate |
| `program_motion` | a snapshot tracker is running and has compared two frames |
| `uptime_secs` | always |
| `sources[].id`, `.state` | always |
| `sources[].name` | it is not the same as the id |
| `sources[].motion` | a score has been computed for that source |
| `sources[].no_video` | the source has no video right now |
| `sources[].no_audio` | the source has no audio right now |
| `sources[].superimposed` | the source is layered over another |
| `sources[].video_idle_ms` | the picture has not moved for at least a second |
| `outputs[].reconnects` | it is not zero |
| `snapshot` | this build has stills. Substitute a source id, `program` or `sheet` for `{id}` |
| `held` | a safety rule is refusing takes, with the reason |

A source that says nothing about its video or its sound has both.

## Detailed

Everything concise has, plus:

| Field | What it is |
|---|---|
| `sources[].audio_peak_db` | the loudest channel of that source's last meter reading |
| `backend` | the encoder and decoder elements actually in use |
| `recent_takes` | the last five, each with the source, the running time and the token that asked |
| `safety` | `min_hold_ms`, `max_takes_per_minute` and `flash_guard` as they apply to this token |
| `telemetry` | the current `shot`, `black`, `freeze`, `lufs_s`, `lufs_i` and `silence`, and whether the probes are measuring |

Ask for it when something is wrong. It is several times the size.

## The budget

Measured by a test in `crates/godwinmix/src/control/methods/agent.rs`, and
printed by `gmx agent cost`:

| Sources | Concise, bytes | About, tokens | Budget |
|---|---|---|---|
| 2 | 228 | 57 | 250 |
| 6 | 396 | 99 | 500 |
| 16 | 823 | 206 | 1,200 |

Four bytes to a token, which is the proxy used throughout. For comparison, a
snapshot at 320 by 180 is 84 tokens, at 640 by 360 about 300, and at 1280 by
720 about 1,200: one look at the largest size costs about as much as twelve
reads of this document.

```sh
gmx agent cost                    # against a core on the default address
gmx agent cost --json             # the same numbers as JSON
```

## Pushed instead of polled

An agent that subscribes on `/rpc` with `ext: {agent: true}` gets the whole
concise document as `event/agent.state` when something crosses a threshold or
a take lands, rather than asking for it.

```json
{"method": "core.subscribe",
 "params": {"events": ["agent.state"],
            "ext": {"agent": {"shot": 0.3, "black": 0.98, "freeze_ms": 200, "silence_ms": 500}}}}
```

`"agent": true` takes those defaults. The push is edge triggered, so a picture
that stays black is one message rather than one a tick, and rate limited to at
most one a second. The payload carries `why`, one of `program`, `black`,
`freeze`, `silence` or `shot`, and the `snapshot` URL pattern.

Over MCP the same push arrives as a server notification,
`notifications/gmx/agent.state`, on stdio and on the Streamable HTTP transport.
Nothing has to be subscribed to: the MCP server holds the `/rpc` socket for you.

## `event/telemetry`

The numbers without the document, up to ten times a second, on
`ext: {telemetry: {hz: 2}}`:

```json
{"ts": 1789402257401, "shot": 0.031, "black": 0.0, "freeze": false,
 "lufs_s": -21.3, "lufs_i": -22.1, "silence": false,
 "sources": {"cam1": 1, "cam2": 0}}
```

Under 200 bytes at eight sources, which is about 50 tokens. `sources` is 1 for
live and 0 for anything else, spelled as numbers because `true` and `false`
cost three more bytes each and this goes out ten times a second.

| Field | What it is |
|---|---|
| `ts` | milliseconds since the Unix epoch |
| `shot` | how much the picture changed since the last frame, 0 to 1. A cut is near 1 |
| `black` | fraction of the picture at or below black, 0 to 1 |
| `freeze` | the picture has been identical for `freeze_ms` |
| `lufs_s` | short term loudness over three seconds |
| `lufs_i` | the running integrated figure |
| `silence` | the programme has been below the floor for `silence_ms` |

The loudness figures are approximations: BS.1770's formula without the K
weighting filter, read off the `level` element that is already on the
programme. They are within a decibel or two on speech and music and they are
not a compliance meter.

Nothing measures until a client asks. The probe on the programme's raw video
tee reads one atomic per frame and returns while nobody is subscribed, and
what it measured is forgotten when the last subscriber goes, so a later one
does not read a freeze that happened while nobody was looking.

## Related

* [`docs/reference/safety.md`](safety.md) for what `held` means.
* [`docs/reference/tasks.md`](tasks.md) for a call that answers with a handle.
* [`docs/how-to/use-with-an-ai-agent.md`](../how-to/use-with-an-ai-agent.md).
