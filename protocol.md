# GodwinMix control protocol

Generated from the method table in `crates/godwinmix-protocol/`. Do not edit by hand: `cargo test protocol_json_is_current` fails when this file and the code disagree, and `godwinmix --api-info` prints the JSON behind it.

* `api_level`: 1
* `api_compatible`: 1

The GodwinMix control protocol. One set of methods, events and types, whether the peer is a UI on /rpc, curl on /api/v1, the CLI, the MCP server or a plugin on stdio.

## Transports

| Peer | Where | Framing |
|---|---|---|
| UI, service, node | `WebSocket /rpc` | one JSON-RPC message per text frame; mosaic frames are binary |
| curl, <img> | `/api/v1` | the REST transform of the method names, one error shape |
| legacy UI | `/api and /ws` | deprecated aliases, kept for one release, answered with a Deprecation header |
| MCP client | `stdio` | godwinmix mcp, a thin adapter over these same methods |

## On every call

Keys accepted on every method, handled before a method runs.

| Key | Type | What it does |
|---|---|---|
| `confirm` | string | The confirm_token from a -32020 refusal, valid 30 seconds. Only a token whose policy is confirm = required needs it. |
| `dry_run` | boolean | On any destructive method. Answers the diff it would make and would_change, against the live state, and changes nothing. |
| `idempotency_key` | string | On any mutating method. The answer is kept for 24 hours; a replay returns it with replayed: true. The same key with different params is -32602 with data.idempotency = mismatch. |
| `trace_id` | string | Carried into the answer, the X-Trace-Id header and the log line. Taken from the W3C traceparent header over HTTP, or generated. |

## Methods

`scope` is the least a token needs. A destructive method accepts `dry_run` and, on a token whose policy is `confirm = required`, needs a confirm token first.

| Method | REST | Scope | Destructive | Since | What it does |
|---|---|---|---|---|---|
| `adbreak.end` | `POST /api/v1/adbreak/end` | operate |  | 1 | Cut a running ad short, or disarm one that is scheduled. |
| `adbreak.start` | `POST /api/v1/adbreak/start` | operate |  | 1 | Interrupt the programme with a clip, then rejoin live when it ends. |
| `agent.state` | `GET /api/v1/agent/state` | read |  | 1 | The compact document written for agents: the programme, each source's state and a motion score saying how much its picture is changing. |
| `codec.list` | `GET /api/v1/codecs` | read |  | 1 | Every codec and element in the catalogue, which of them this machine actually has, and what it would pick. |
| `core.api` | `GET /api/v1/core/api` | read |  | 1 | Every method, event and type as JSON Schema. The same document as protocol.json and `godwinmix --api-info`. |
| `core.doctor` | `GET /api/v1/core/doctor` | read |  | 1 | The environment checks: GStreamer, the elements, the config, the disk and the ports. The same list `gmx doctor` prints. |
| `core.info` | `GET /api/v1/core/info` | read |  | 1 | What this core is, what it can do, and where its edges are. |
| `core.session_log` | `GET /api/v1/core/session_log` | admin |  | 1 | The append only record of everything that happened, back as far as you ask. |
| `core.shutdown` | `POST /api/v1/core/shutdown` | admin | yes | 1 | Stop the mixer, and with it the programme. Nothing else takes the show off air, so this is deliberately its own call. |
| `core.startup_report` | `GET /api/v1/core/startup_report` | read |  | 1 | How long each stage of the start took, and what was over the 250 ms mark. |
| `core.status` | `GET /api/v1/core/status` | read |  | 1 | The full state: programme, every source, every output, the multiview grid, the encoder backend and any ad break. |
| `core.subscribe` | (none) | read |  | 1 | Subscribe to the event stream. WebSocket only: the core answers event/snapshot then deltas, ending every batch with event/flush. |
| `filter.add` | `POST /api/v1/filters` | operate |  | 1 | Hang a filter on one source or on the programme, live. |
| `filter.list` | `GET /api/v1/filters` | read |  | 1 | Every filter in place, with what it is and where it sits. |
| `filter.remove` | `DELETE /api/v1/filters/{id}` | operate | yes | 1 | Take a filter out of the pipeline. |
| `filter.set` | `POST /api/v1/filters/{id}/set` | operate |  | 1 | Change a filter's settings in place. A filter that cannot take the change while running says so rather than being restarted behind your back. |
| `log.gst` | `POST /api/v1/log/gst` | admin |  | 1 | Raise GStreamer's own debug categories for a while, then let them fall back on their own. |
| `log.levels` | `GET /api/v1/log/levels` | read |  | 1 | Every log level override in force, and the GStreamer categories still raised. |
| `log.set` | `POST /api/v1/log/set` | admin |  | 1 | Change one instance's or one module's log level while the mixer runs. |
| `media.convert` | `POST /api/v1/media/{id}/convert` | operate |  | 1 | Transcode a library file to a web safe copy, in the background. |
| `media.list` | `GET /api/v1/media` | read |  | 1 | The clips in the library, with durations and whether each has audio. |
| `media.remove` | `DELETE /api/v1/media/{id}` | operate | yes | 1 | Delete a library file and its converted copy. Refused while it is a live source. |
| `media.upload` | `POST /api/v1/media/upload` | operate |  | 1 | Stream a file into the library. HTTP only: the body is the file. |
| `output.add` | `POST /api/v1/outputs` | operate |  | 1 | Send the programme to another destination. The encoder is shared, so adding one costs nothing on air. |
| `output.get` | `GET /api/v1/outputs/{id}` | read |  | 1 | One destination. |
| `output.list` | `GET /api/v1/outputs` | read |  | 1 | Every destination, with its state, reconnect count and how much is buffered. |
| `output.reconnect` | `POST /api/v1/outputs/{id}/reconnect` | operate |  | 1 | Drop and re-establish one destination's connection now, without waiting for its reconnect policy. |
| `output.remove` | `DELETE /api/v1/outputs/{id}` | operate | yes | 1 | Stop sending to a destination and forget it. Other outputs are unaffected. |
| `pipeline.clock` | `GET /api/v1/pipeline/clock` | read |  | 1 | The clock every pipeline is running against, and how far each one has got. |
| `pipeline.dot` | `GET /api/v1/pipeline/dot` | read |  | 1 | One pipeline as a graphviz graph: every element, every pad and the caps negotiated between them. |
| `pipeline.latency` | `GET /api/v1/pipeline/latency` | read |  | 1 | How much delay one pipeline is carrying, and which stage put it there. |
| `pipeline.list` | `GET /api/v1/pipeline/list` | read |  | 1 | Every pipeline running right now, by the name the other pipeline methods accept. |
| `pipeline.queues` | `GET /api/v1/pipeline/queues` | read |  | 1 | Every queue in one pipeline with how full it is, fullest first. A queue that stays full is where the trouble is. |
| `program.get` | `GET /api/v1/program` | read |  | 1 | What is on air, the programme running time, and what revert would go back to. |
| `program.golive` | `POST /api/v1/program/golive` | operate |  | 1 | One call to put a web page on air: add the page, add the destination, and take the page as soon as it renders. |
| `program.history` | `GET /api/v1/program/history` | read |  | 1 | The last hundred takes, newest first, with the token that asked for each. |
| `program.revert` | `POST /api/v1/program/revert` | operate |  | 1 | Take back to the shot before this one. |
| `program.take` | `POST /api/v1/program/take` | operate |  | 1 | Put a source on programme. The cut is instant and the outgoing stream is not disturbed. |
| `snapshot.get` | `GET /api/v1/snapshot/{id}` | read |  | 1 | One JPEG: the whole contact sheet, the programme, or one source cut out of the mosaic. |
| `source.add` | `POST /api/v1/sources` | operate |  | 1 | Add a source while the mixer runs. Answers with the id it got and the whole source record. |
| `source.audio.set` | `POST /api/v1/sources/{id}/audio` | operate |  | 1 | Move a source's audio: the fader, the mute, and for a superimposed page the balance between its own sound and the videos under it. |
| `source.get` | `GET /api/v1/sources/{id}` | read |  | 1 | One source. Refused with the ids that exist when there is no such source. |
| `source.list` | `GET /api/v1/sources` | read |  | 1 | Every source, with its state, whether it has video and audio, and its fader. |
| `source.remove` | `DELETE /api/v1/sources/{id}` | operate | yes | 1 | Remove a source. If it is on programme the mixer cuts to the slate first. |
| `source.seek` | `POST /api/v1/sources/{id}/seek` | operate |  | 1 | Move a seekable source to a position. Answers with where it actually landed. |
| `task.cancel` | `POST /api/v1/task/cancel` | operate |  | 1 | Ask a piece of long running work to stop. Cooperative: the answer says the request landed, not that the work has stopped yet. |
| `task.get` | `GET /api/v1/task` | read |  | 1 | How a piece of long running work is getting on, and its answer once it has one. |
| `task.list` | `GET /api/v1/task/list` | read |  | 1 | Every background job this core knows about, newest first. |

### Params and results

#### `adbreak.end`

Cut a running ad short, or disarm one that is scheduled.

MCP tool `end_ad_break` in the `search` profile: readOnlyHint false, destructiveHint false, idempotentHint true.

```json
{
  "params": {
    "additionalProperties": false,
    "properties": {},
    "type": "object"
  },
  "result": {
    "type": "object"
  }
}
```

#### `adbreak.start`

Interrupt the programme with a clip, then rejoin live when it ends.

MCP tool `ad_break` in the `search` profile: readOnlyHint false, destructiveHint false, idempotentHint true.

```json
{
  "params": {
    "$ref": "#/$defs/AdBreakRequest"
  },
  "result": {
    "type": "object"
  }
}
```

#### `agent.state`

The compact document written for agents: the programme, each source's state and a motion score saying how much its picture is changing.

MCP tool `agent_state` in the `minimal` profile: readOnlyHint true, destructiveHint false, idempotentHint true.

```json
{
  "params": {
    "$ref": "#/$defs/AgentStateRequest"
  },
  "result": {
    "type": "object"
  }
}
```

#### `codec.list`

Every codec and element in the catalogue, which of them this machine actually has, and what it would pick.

MCP tool `list_codecs` in the `search` profile: readOnlyHint true, destructiveHint false, idempotentHint true.

```json
{
  "params": {
    "additionalProperties": false,
    "properties": {},
    "type": "object"
  },
  "result": {
    "type": "object"
  }
}
```

#### `core.api`

Every method, event and type as JSON Schema. The same document as protocol.json and `godwinmix --api-info`.

```json
{
  "params": {
    "additionalProperties": false,
    "properties": {},
    "type": "object"
  },
  "result": {
    "type": "object"
  }
}
```

#### `core.doctor`

The environment checks: GStreamer, the elements, the config, the disk and the ports. The same list `gmx doctor` prints.

MCP tool `doctor` in the `search` profile: readOnlyHint true, destructiveHint false, idempotentHint true.

```json
{
  "params": {
    "additionalProperties": false,
    "properties": {},
    "type": "object"
  },
  "result": {
    "type": "object"
  }
}
```

#### `core.info`

What this core is, what it can do, and where its edges are.

MCP tool `core_info` in the `search` profile: readOnlyHint true, destructiveHint false, idempotentHint true.

```json
{
  "params": {
    "additionalProperties": false,
    "properties": {},
    "type": "object"
  },
  "result": {
    "$ref": "#/$defs/CoreInfo"
  }
}
```

#### `core.session_log`

The append only record of everything that happened, back as far as you ask.

```json
{
  "params": {
    "$ref": "#/$defs/SessionLogRequest"
  },
  "result": {
    "type": "object"
  }
}
```

#### `core.shutdown`

Stop the mixer, and with it the programme. Nothing else takes the show off air, so this is deliberately its own call.

```json
{
  "params": {
    "additionalProperties": false,
    "properties": {},
    "type": "object"
  },
  "result": {
    "type": "object"
  }
}
```

#### `core.startup_report`

How long each stage of the start took, and what was over the 250 ms mark.

```json
{
  "params": {
    "additionalProperties": false,
    "properties": {},
    "type": "object"
  },
  "result": {
    "type": "object"
  }
}
```

#### `core.status`

The full state: programme, every source, every output, the multiview grid, the encoder backend and any ad break.

MCP tool `status` in the `standard` profile: readOnlyHint true, destructiveHint false, idempotentHint true.

```json
{
  "params": {
    "additionalProperties": false,
    "properties": {},
    "type": "object"
  },
  "result": {
    "$ref": "#/$defs/MixerStatus"
  }
}
```

#### `core.subscribe`

Subscribe to the event stream. WebSocket only: the core answers event/snapshot then deltas, ending every batch with event/flush.

```json
{
  "params": {
    "$ref": "#/$defs/SubscribeRequest"
  },
  "result": {
    "$ref": "#/$defs/SubscribeResult"
  }
}
```

#### `filter.add`

Hang a filter on one source or on the programme, live.

MCP tool `add_filter` in the `search` profile: readOnlyHint false, destructiveHint false, idempotentHint true.

```json
{
  "params": {
    "$ref": "#/$defs/AddFilterRequest"
  },
  "result": {
    "$ref": "#/$defs/FilterRecord"
  }
}
```

#### `filter.list`

Every filter in place, with what it is and where it sits.

```json
{
  "params": {
    "additionalProperties": false,
    "properties": {},
    "type": "object"
  },
  "result": {
    "$ref": "#/$defs/FilterListing"
  }
}
```

#### `filter.remove`

Take a filter out of the pipeline.

```json
{
  "params": {
    "$ref": "#/$defs/FilterIdRequest"
  },
  "result": {
    "$ref": "#/$defs/FilterRemoved"
  }
}
```

#### `filter.set`

Change a filter's settings in place. A filter that cannot take the change while running says so rather than being restarted behind your back.

```json
{
  "params": {
    "$ref": "#/$defs/SetFilterRequest"
  },
  "result": {
    "$ref": "#/$defs/FilterRecord"
  }
}
```

#### `log.gst`

Raise GStreamer's own debug categories for a while, then let them fall back on their own.

```json
{
  "params": {
    "$ref": "#/$defs/LogGstRequest"
  },
  "result": {
    "$ref": "#/$defs/LogGstResult"
  }
}
```

#### `log.levels`

Every log level override in force, and the GStreamer categories still raised.

```json
{
  "params": {
    "additionalProperties": false,
    "properties": {},
    "type": "object"
  },
  "result": {
    "type": "object"
  }
}
```

#### `log.set`

Change one instance's or one module's log level while the mixer runs.

```json
{
  "params": {
    "$ref": "#/$defs/LogSetRequest"
  },
  "result": {
    "type": "object"
  }
}
```

#### `media.convert`

Transcode a library file to a web safe copy, in the background.

MCP tool `convert_media` in the `search` profile: readOnlyHint false, destructiveHint false, idempotentHint true.

```json
{
  "params": {
    "$ref": "#/$defs/NameRequest"
  },
  "result": {
    "$ref": "#/$defs/ConversionState"
  }
}
```

#### `media.list`

The clips in the library, with durations and whether each has audio.

MCP tool `list_media` in the `search` profile: readOnlyHint true, destructiveHint false, idempotentHint true.

```json
{
  "params": {
    "additionalProperties": false,
    "properties": {},
    "type": "object"
  },
  "result": {
    "$ref": "#/$defs/MediaListing"
  }
}
```

#### `media.remove`

Delete a library file and its converted copy. Refused while it is a live source.

MCP tool `remove_media` in the `search` profile: readOnlyHint false, destructiveHint true, idempotentHint true.

```json
{
  "params": {
    "$ref": "#/$defs/NameRequest"
  },
  "result": {
    "type": "object"
  }
}
```

#### `media.upload`

Stream a file into the library. HTTP only: the body is the file.

```json
{
  "params": {
    "additionalProperties": false,
    "properties": {},
    "type": "object"
  },
  "result": {
    "type": "object"
  }
}
```

#### `output.add`

Send the programme to another destination. The encoder is shared, so adding one costs nothing on air.

MCP tool `add_output` in the `standard` profile: readOnlyHint false, destructiveHint false, idempotentHint true.

```json
{
  "params": {
    "$ref": "#/$defs/AddOutputRequest"
  },
  "result": {
    "$ref": "#/$defs/OutputStatus"
  }
}
```

#### `output.get`

One destination.

```json
{
  "params": {
    "$ref": "#/$defs/IdRequest"
  },
  "result": {
    "$ref": "#/$defs/OutputStatus"
  }
}
```

#### `output.list`

Every destination, with its state, reconnect count and how much is buffered.

MCP tool `list_outputs` in the `standard` profile: readOnlyHint true, destructiveHint false, idempotentHint true.

```json
{
  "params": {
    "additionalProperties": false,
    "properties": {},
    "type": "object"
  },
  "result": {
    "items": {
      "$ref": "#/$defs/OutputStatus"
    },
    "type": "array"
  }
}
```

#### `output.reconnect`

Drop and re-establish one destination's connection now, without waiting for its reconnect policy.

MCP tool `reconnect_output` in the `search` profile: readOnlyHint false, destructiveHint false, idempotentHint true.

```json
{
  "params": {
    "$ref": "#/$defs/IdRequest"
  },
  "result": {
    "$ref": "#/$defs/OutputStatus"
  }
}
```

#### `output.remove`

Stop sending to a destination and forget it. Other outputs are unaffected.

MCP tool `remove_output` in the `search` profile: readOnlyHint false, destructiveHint true, idempotentHint true.

```json
{
  "params": {
    "$ref": "#/$defs/IdRequest"
  },
  "result": {
    "type": "object"
  }
}
```

#### `pipeline.clock`

The clock every pipeline is running against, and how far each one has got.

```json
{
  "params": {
    "additionalProperties": false,
    "properties": {},
    "type": "object"
  },
  "result": {
    "type": "object"
  }
}
```

#### `pipeline.dot`

One pipeline as a graphviz graph: every element, every pad and the caps negotiated between them.

```json
{
  "params": {
    "$ref": "#/$defs/PipelineRequest"
  },
  "result": {
    "$ref": "#/$defs/PipelineDot"
  }
}
```

#### `pipeline.latency`

How much delay one pipeline is carrying, and which stage put it there.

MCP tool `pipeline_latency` in the `search` profile: readOnlyHint true, destructiveHint false, idempotentHint true.

```json
{
  "params": {
    "$ref": "#/$defs/PipelineRequest"
  },
  "result": {
    "type": "object"
  }
}
```

#### `pipeline.list`

Every pipeline running right now, by the name the other pipeline methods accept.

```json
{
  "params": {
    "additionalProperties": false,
    "properties": {},
    "type": "object"
  },
  "result": {
    "type": "object"
  }
}
```

#### `pipeline.queues`

Every queue in one pipeline with how full it is, fullest first. A queue that stays full is where the trouble is.

```json
{
  "params": {
    "$ref": "#/$defs/PipelineRequest"
  },
  "result": {
    "type": "object"
  }
}
```

#### `program.get`

What is on air, the programme running time, and what revert would go back to.

```json
{
  "params": {
    "additionalProperties": false,
    "properties": {},
    "type": "object"
  },
  "result": {
    "$ref": "#/$defs/ProgramState"
  }
}
```

#### `program.golive`

One call to put a web page on air: add the page, add the destination, and take the page as soon as it renders.

MCP tool `go_live` in the `standard` profile: readOnlyHint false, destructiveHint false, idempotentHint true.

```json
{
  "params": {
    "$ref": "#/$defs/GoLiveRequest"
  },
  "result": {
    "$ref": "#/$defs/GoLiveResult"
  }
}
```

#### `program.history`

The last hundred takes, newest first, with the token that asked for each.

MCP tool `program_history` in the `search` profile: readOnlyHint true, destructiveHint false, idempotentHint true.

```json
{
  "params": {
    "$ref": "#/$defs/HistoryRequest"
  },
  "result": {
    "items": {
      "$ref": "#/$defs/TakeRecord"
    },
    "type": "array"
  }
}
```

#### `program.revert`

Take back to the shot before this one.

MCP tool `revert` in the `standard` profile: readOnlyHint false, destructiveHint false, idempotentHint true.

```json
{
  "params": {
    "additionalProperties": false,
    "properties": {},
    "type": "object"
  },
  "result": {
    "$ref": "#/$defs/ProgramState"
  }
}
```

#### `program.take`

Put a source on programme. The cut is instant and the outgoing stream is not disturbed.

MCP tool `take` in the `minimal` profile: readOnlyHint false, destructiveHint false, idempotentHint true.

```json
{
  "params": {
    "$ref": "#/$defs/TakeRequest"
  },
  "result": {
    "$ref": "#/$defs/ProgramState"
  }
}
```

#### `snapshot.get`

One JPEG: the whole contact sheet, the programme, or one source cut out of the mosaic.

MCP tool `snapshot` in the `standard` profile: readOnlyHint true, destructiveHint false, idempotentHint true.

```json
{
  "params": {
    "$ref": "#/$defs/SnapshotRequest"
  },
  "result": {
    "type": "object"
  }
}
```

#### `source.add`

Add a source while the mixer runs. Answers with the id it got and the whole source record.

MCP tool `add_source` in the `minimal` profile: readOnlyHint false, destructiveHint false, idempotentHint true.

```json
{
  "params": {
    "$ref": "#/$defs/AddSourceRequest"
  },
  "result": {
    "$ref": "#/$defs/SourceStatus"
  }
}
```

#### `source.audio.set`

Move a source's audio: the fader, the mute, and for a superimposed page the balance between its own sound and the videos under it.

MCP tool `set_source_audio` in the `search` profile: readOnlyHint false, destructiveHint false, idempotentHint true.

```json
{
  "params": {
    "$ref": "#/$defs/AudioSetParams"
  },
  "result": {
    "$ref": "#/$defs/SourceAudioState"
  }
}
```

#### `source.get`

One source. Refused with the ids that exist when there is no such source.

```json
{
  "params": {
    "$ref": "#/$defs/IdRequest"
  },
  "result": {
    "$ref": "#/$defs/SourceStatus"
  }
}
```

#### `source.list`

Every source, with its state, whether it has video and audio, and its fader.

MCP tool `list_sources` in the `minimal` profile: readOnlyHint true, destructiveHint false, idempotentHint true.

```json
{
  "params": {
    "additionalProperties": false,
    "properties": {},
    "type": "object"
  },
  "result": {
    "items": {
      "$ref": "#/$defs/SourceStatus"
    },
    "type": "array"
  }
}
```

#### `source.remove`

Remove a source. If it is on programme the mixer cuts to the slate first.

MCP tool `remove_source` in the `standard` profile: readOnlyHint false, destructiveHint true, idempotentHint true.

```json
{
  "params": {
    "$ref": "#/$defs/IdRequest"
  },
  "result": {
    "type": "object"
  }
}
```

#### `source.seek`

Move a seekable source to a position. Answers with where it actually landed.

MCP tool `seek_source` in the `search` profile: readOnlyHint false, destructiveHint false, idempotentHint true.

```json
{
  "params": {
    "$ref": "#/$defs/SeekParams"
  },
  "result": {
    "$ref": "#/$defs/SourcePositionState"
  }
}
```

#### `task.cancel`

Ask a piece of long running work to stop. Cooperative: the answer says the request landed, not that the work has stopped yet.

MCP tool `task_cancel` in the `search` profile: readOnlyHint false, destructiveHint false, idempotentHint true.

```json
{
  "params": {
    "$ref": "#/$defs/TaskRequest"
  },
  "result": {
    "type": "object"
  }
}
```

#### `task.get`

How a piece of long running work is getting on, and its answer once it has one.

MCP tool `task_get` in the `search` profile: readOnlyHint true, destructiveHint false, idempotentHint true.

```json
{
  "params": {
    "$ref": "#/$defs/TaskRequest"
  },
  "result": {
    "$ref": "#/$defs/TaskView"
  }
}
```

#### `task.list`

Every background job this core knows about, newest first.

```json
{
  "params": {
    "additionalProperties": false,
    "properties": {},
    "type": "object"
  },
  "result": {
    "items": {
      "$ref": "#/$defs/TaskView"
    },
    "type": "array"
  }
}
```

## Events

Subscribe with `core.subscribe`. Patterns match the part after `event/`, so `program.*` matches `event/program.took`. Every event carries `seq`; every batch ends with `event/flush`; a client that falls behind gets `event/resync`.

| Event | ext | Replaces on /ws | What it carries |
|---|---|---|---|
| `event/snapshot` |  | `status` | The full state, and the sequence number it is current as of. Sent on subscribe and after any change the deltas cannot describe. |
| `event/program.took` |  | `took` | The programme changed. Carries the running time the cut landed on, so a client can see how close a scheduled take was to its mark. |
| `event/source.state` |  | `source_state_changed` | A source moved between connecting, live, stalled and failed. |
| `event/source.position` | `positions` | `source_position` | How far through a seekable source has got, a few times a second. Never sent for a camera, which has no position to report. |
| `event/output.state` |  | `output_state_changed` | A destination connected, dropped or is retrying. |
| `event/adbreak.changed` |  | `ad_break_changed` | An ad break was armed, went on air, or ended. |
| `event/media.changed` |  | `media_changed` | A file in the library was uploaded, deleted, or its conversion moved on. |
| `event/meters` | `meters` | `audio_level, source_audio_level` | Peak dBFS for the programme bus and every source, in one message at 10 per second. Replaces the two separate meter events on /ws. |
| `event/tally` | `tally` |  | Which sources are on programme, on preview, or off. Derived by the core so a Stream Deck does not have to. |
| `event/alert` |  | `alert` | Something an operator should see. Also written to the log and to the alert webhook. |
| `event/multiview.layout` | `multiview` |  | How to read the binary frames that follow: the cells, and the layout id carried in every frame header. |
| `event/multiview.frame` | `multiview` | `raw JPEG binary frame` | A mosaic frame, as a binary WebSocket frame rather than JSON: a 16 byte little endian header (seq u32, layout id u32, programme running time in milliseconds u64) then the JPEG. |
| `event/resync` |  |  | This client fell behind and events were dropped. Re-subscribe for a fresh snapshot; nothing between from_seq and the new snapshot arrives. |
| `event/flush` |  |  | The end of a batch. Render here and not before, so a client never paints half an update. |

## The routes this replaces

The paths below still answer, for one release, with a `Deprecation: true` header. Move to the method named beside each one.

| Was | Now |
|---|---|
| `GET /api/status` | `core.status` |
| `GET /api/agent/state` | `agent.state` |
| `GET /api/snapshot/{name}` | `snapshot.get` |
| `POST /api/take` | `program.take` |
| `POST /api/golive` | `program.golive` |
| `POST /api/shutdown` | `core.shutdown` |
| `GET /api/media` | `media.list` |
| `POST /api/media/upload` | `media.upload` |
| `POST /api/media/{name}/convert` | `media.convert` |
| `DELETE /api/media/{name}` | `media.remove` |
| `POST /api/adbreak` | `adbreak.start` |
| `POST /api/adbreak/end` | `adbreak.end` |
| `POST /api/sources` | `source.add` |
| `DELETE /api/sources/{id}` | `source.remove` |
| `POST /api/sources/{id}/audio` | `source.audio.set` |
| `POST /api/sources/{id}/seek` | `source.seek` |
| `GET /api/outputs` | `output.list` |
| `POST /api/outputs` | `output.add` |
| `DELETE /api/outputs/{id}` | `output.remove` |
| `POST /api/outputs/{id}/reconnect` | `output.reconnect` |
| `GET /ws` | `core.subscribe` |

| Path | What it is |
|---|---|
| `GET /` | the reference web UI, served from the binary |
| `GET /rpc` | the JSON-RPC WebSocket. Everything in `methods` is reachable here. |
| `GET /api/v1/status` | an alias for GET /api/v1/core/status, because it is what people type |
| `ANY /api/v1/{*rest}` | every method's REST route, generated by the transform rule |

## The ext table

A client declares which expensive streams it wants. The core does no work for a stream nobody asked for. An `ext` key is the subscription for its own events: ask for `meters` and you get `event/meters`, whether or not `meters` is among your event patterns. Keys marked not implemented are accepted and reported back in `ignored_ext`, so a client written against the whole table still connects.

| Key | Value | Turns on | In this build |
|---|---|---|---|
| `multiview` | `{fps: 1..30, width: 320..1920} or false` | the mosaic pipeline, built on the first subscriber and stopped on the last, plus event/multiview.layout and the binary frames | yes |
| `meters` | `true` | event/meters | yes |
| `tally` | `true` | event/tally | yes |
| `positions` | `true` | event/source.position | yes |
| `thumb` | `{fps}` | per source thumbnails from a node | not yet |
| `preview` | `{fps, width} or "full"` | the preview scene | not yet |
| `telemetry` | `{hz: 1..10}` | event/telemetry | not yet |
| `agent` | `true or thresholds` | event/agent.state with a snapshot URL | not yet |

## Errors

One shape everywhere: `{"error": {"code", "message", "data"}}`, with `trace_id` beside it. The message names the current state and the next step, and an unknown id lists the ids that would have worked.

| Code | Meaning | HTTP | Retryable |
|---|---|---|---|
| -32700 | the body was not JSON | 400 | no |
| -32600 | the envelope was not a JSON-RPC request | 400 | no |
| -32601 | no such method | 404 | no |
| -32602 | the params were wrong for this method | 400 | no |
| -32603 | the core failed while handling the call | 500 | yes |
| -32001 | not in a state that allows this | 409 | yes |
| -32002 | refused by the token's scopes | 403 | no |
| -32003 | refused by a safety rule | 429 | yes |
| -32004 | no such id | 404 | no |
| -32005 | the plugin did not declare that placement | 400 | no |
| -32010 | the plugin died during the call | 500 | yes |
| -32011 | a protocol line was over 4 MiB | 400 | no |
| -32012 | the change needs the instance restarted | 400 | no |
| -32020 | a confirm token is needed first | 428 | yes |

