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
| `core.restart` | `POST /api/v1/core/restart` | admin | yes | 1 | Stop the mixer and have it started again, when something will start it again. On a supervised core (core.info restart.possible) it answers restarting: true and exits; the programme is off air until it is back. On a core started by hand it answers restarting: false, says how to restart it, and keeps running. |
| `core.session_log` | `GET /api/v1/core/session_log` | admin |  | 1 | The append only record of everything that happened, back as far as you ask. |
| `core.shutdown` | `POST /api/v1/core/shutdown` | admin | yes | 1 | Stop the mixer, and with it the programme. Nothing else takes the show off air, so this is deliberately its own call. |
| `core.startup_report` | `GET /api/v1/core/startup_report` | read |  | 1 | How long each stage of the start took, and what was over the 250 ms mark. |
| `core.status` | `GET /api/v1/core/status` | read |  | 1 | The full state: programme, every source, every output, the multiview grid, the encoder backend and any ad break. |
| `core.subscribe` | (none) | read |  | 1 | Subscribe to the event stream. WebSocket only: the core answers event/snapshot then deltas, ending every batch with event/flush. |
| `device.discover` | `POST /api/v1/device/discover` | operate |  | 1 | Ask every device plugin what it can see: cameras, NDI senders, publishers. Each candidate's params are ready for source.add. |
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
| `node.discover` | `POST /api/v1/nodes/{id}/discover` | read |  | 1 | Look for nodes on the local network over mDNS. A network without multicast finds nothing and the [nodes] table in the config is the way there. |
| `node.enrol` | `POST /api/v1/nodes/{id}/enrol` | admin |  | 1 | Mint a one time enrolment token for a node. The answer carries the command to run on the other machine. The token is good for one enrolment and expires. |
| `node.get` | `GET /api/v1/nodes/{id}` | read |  | 1 | One node: its clock offset, how long since its last heartbeat, the plugins it has, and the instances it is hosting. |
| `node.list` | `GET /api/v1/nodes` | read |  | 1 | Every node this core knows about: the ones connected now, the ones that have gone quiet, and the ones the config expects that have never dialled in. |
| `node.remove` | `DELETE /api/v1/nodes/{id}` | admin | yes | 1 | Forget a node. Its bridge is closed, every token minted for a plugin on it is revoked, and its certificate stops working. Sources placed on it go to the slate until they are moved or the node enrols again. |
| `output.add` | `POST /api/v1/outputs` | operate |  | 1 | Send the programme to another destination. The encoder is shared, so adding one costs nothing on air. |
| `output.get` | `GET /api/v1/outputs/{id}` | read |  | 1 | One destination. |
| `output.list` | `GET /api/v1/outputs` | read |  | 1 | Every destination, with its state, reconnect count and how much is buffered. |
| `output.reconnect` | `POST /api/v1/outputs/{id}/reconnect` | operate |  | 1 | Drop and re-establish one destination's connection now, without waiting for its reconnect policy. |
| `output.remove` | `DELETE /api/v1/outputs/{id}` | operate | yes | 1 | Stop sending to a destination and forget it. Other outputs are unaffected. |
| `output.set` | `POST /api/v1/outputs/{id}/set` | operate |  | 1 | Change a destination in place: a new address with a new stream key, a new reconnect policy, a deeper outage buffer. The address is write only, so a client that only wants the buffer never has to hold the key. |
| `pipeline.clock` | `GET /api/v1/pipeline/clock` | read |  | 1 | The clock every pipeline is running against, and how far each one has got. |
| `pipeline.dot` | `GET /api/v1/pipeline/dot` | read |  | 1 | One pipeline as a graphviz graph: every element, every pad and the caps negotiated between them. |
| `pipeline.latency` | `GET /api/v1/pipeline/latency` | read |  | 1 | How much delay one pipeline is carrying, and which stage put it there. |
| `pipeline.list` | `GET /api/v1/pipeline/list` | read |  | 1 | Every pipeline running right now, by the name the other pipeline methods accept. |
| `pipeline.queues` | `GET /api/v1/pipeline/queues` | read |  | 1 | Every queue in one pipeline with how full it is, fullest first. A queue that stays full is where the trouble is. |
| `plugin.add` | `POST /api/v1/plugins` | admin | yes | 1 | Install a plugin, while live, from any source form: a GitHub release (owner/repo), a git URL, cargo:, npm:, pypi:, a local directory, or a bare name looked up in the marketplaces this mixer knows. The signature and the api level are checked before anything is copied. |
| `plugin.describe` | `POST /api/v1/plugins/{id}/describe` | read |  | 1 | One plugin in full: its manifest, the settings schema of every provide, and the description from each SKILL.md. |
| `plugin.disable` | `POST /api/v1/plugins/{id}/disable` | admin |  | 1 | Turn a plugin off without uninstalling it. It registers nothing and runs no process until it is enabled again. |
| `plugin.enable` | `POST /api/v1/plugins/{id}/enable` | admin |  | 1 | Turn a plugin back on. It registers what it declares and its instances start. |
| `plugin.list` | `GET /api/v1/plugins` | read |  | 1 | Every plugin installed, with what it provides and what each running instance is costing in cpu, memory, latency, dropped buffers and restarts. |
| `plugin.reload` | `POST /api/v1/plugins/{id}/reload` | admin |  | 1 | Read a plugin's directory again and swap its running instances one at a time, with the freeze frame covering each. |
| `plugin.remove` | `DELETE /api/v1/plugins/{id}` | admin | yes | 1 | Uninstall a plugin and unwind everything it registered: its provides, its tools, its panels, its hooks and its discovery matchers. |
| `plugin.search` | `POST /api/v1/plugins/{id}/search` | read |  | 1 | Search every marketplace this mixer knows for a plugin, by name, description or kind. Answers what `gmx plugin add <name>` would install. |
| `plugin.settings.get` | `GET /api/v1/plugins/{id}/settings` | read |  | 1 | A plugin's settings as they stand, with its schema beside them. |
| `plugin.settings.set` | `POST /api/v1/plugins/{id}/settings` | admin |  | 1 | Change a plugin's settings. A plugin that cannot take a change while running says so rather than being restarted behind your back. |
| `plugin.stats` | `POST /api/v1/plugins/{id}/stats` | read |  | 1 | Per instance cpu, memory, media latency, dropped buffers and restarts, refreshed once a second. |
| `plugin.update` | `POST /api/v1/plugins/{id}/update` | admin | yes | 1 | Fetch a newer build of a plugin, install it beside the one that is running, and prove it starts. A build that does not answer `initialize` within ten seconds is rolled back and the plugin that was working stays working. |
| `preset.apply` | `POST /api/v1/preset/apply` | admin | yes | 1 | Put a preset on this core: its config, its scenes, its layout, its theme and its gallery mode. Pass dry_run to get the plan and write nothing. |
| `preset.list` | `GET /api/v1/preset/list` | read |  | 1 | Every preset this core can apply: the six built in, plus anything installed beside the binary or under ~/.godwinmix/presets. |
| `preset.save` | `POST /api/v1/preset/save` | admin |  | 1 | Turn this core's working setup into a preset directory somebody else can apply. Stream keys and the control token are replaced with placeholders. |
| `preview.close` | `POST /api/v1/preview/close` | read |  | 1 | Give up a raw frame socket. The socket goes when the last holder closes it. |
| `preview.open` | `POST /api/v1/preview/open` | read |  | 1 | Open a raw frame socket on this machine for a source or the programme, and answer with its path. No encode anywhere: a client on the same host reads the frames the mixer already has. Close it with preview.close. |
| `program.get` | `GET /api/v1/program` | read |  | 1 | What is on air, the programme running time, and what revert would go back to. |
| `program.golive` | `POST /api/v1/program/golive` | operate |  | 1 | One call to put a web page on air: add the page, add the destination, and take the page as soon as it renders. |
| `program.history` | `GET /api/v1/program/history` | read |  | 1 | The last hundred takes, newest first, with the token that asked for each. |
| `program.revert` | `POST /api/v1/program/revert` | operate |  | 1 | Take back to the shot before this one. |
| `program.take` | `POST /api/v1/program/take` | operate |  | 1 | Put a scene or a source on programme. The cut is instant and the outgoing stream is not disturbed. |
| `scene.add` | `POST /api/v1/scenes` | operate |  | 1 | Make an empty scene, or one built from a set of sources. |
| `scene.apply_graphic` | `POST /api/v1/scenes/apply_graphic` | operate |  | 1 | Fill a graphic that is on a scene, by field name, and optionally play it on or take it off. Answers with the records and, if asked, a still. |
| `scene.apply_layout` | `POST /api/v1/scenes/apply_layout` | operate |  | 1 | Apply a layout, making a scene or reshaping one that exists. Applying onto an existing scene keeps the item ids, so the change is a ramp and not a cut. |
| `scene.create_from` | `POST /api/v1/scenes/create_from` | operate |  | 1 | A scene from a set of sources, laid out by the built in layout for that count (full, two-box, three-box, quad, then a grid) or by a named one. |
| `scene.duplicate` | `POST /api/v1/scenes/{id}/duplicate` | operate |  | 1 | A copy of a scene with new ids throughout, so editing the copy cannot touch the original. |
| `scene.edit.apply` | `POST /api/v1/scenes/edit/apply` | operate |  | 1 | Write a draft back into the live document. |
| `scene.edit.begin` | `POST /api/v1/scenes/edit/begin` | operate |  | 1 | Take a working copy of a scene. Editing is off air by default: the draft is written back on the next take of that scene, or when you apply it. |
| `scene.edit.discard` | `POST /api/v1/scenes/edit/discard` | operate |  | 1 | Throw a draft away. The live document is untouched. |
| `scene.export` | `GET /api/v1/scenes/export` | read |  | 1 | The whole collection: as JSON, or as a zip bundle carrying its assets with a hash each, which is what you send somebody. |
| `scene.get` | `GET /api/v1/scenes/{id}` | read |  | 1 | One scene: its records and where every item actually lands on the canvas. |
| `scene.graphic.list` | `GET /api/v1/scenes/graphic/list` | read |  | 1 | Every graphic template this core can place, with what each one takes. |
| `scene.history.mark` | `POST /api/v1/scenes/history/mark` | operate |  | 1 | Group the changes that follow into one undo step, until the next mark. This is what makes a drag of forty moves one Ctrl+Z. |
| `scene.import` | `POST /api/v1/scenes/import` | operate |  | 1 | Read a collection bundle, a zip or the directory it unpacks to, and add its scenes to this one. Answers with a relink report for any asset that did not come across. |
| `scene.import.obs` | `POST /api/v1/scenes/import/obs` | operate |  | 1 | Read an OBS Studio scene collection and add its scenes to this one. |
| `scene.item.add` | `POST /api/v1/scenes/item/add` | operate |  | 1 | Put something on a scene's canvas. With no transform it lands in the next free cell, so a drop never needs a dialog. |
| `scene.item.align` | `POST /api/v1/scenes/item/align` | operate |  | 1 | Line items up on an edge: left, right, top, bottom, center-x or center-y. |
| `scene.item.arrange_grid` | `POST /api/v1/scenes/item/arrange_grid` | operate |  | 1 | Lay items out in a grid of `cols` columns. |
| `scene.item.bind` | `POST /api/v1/scenes/item/bind` | operate |  | 1 | Bind a geometry property to an expression over the collection's parameters, so changing a number moves everything that follows it. |
| `scene.item.copy` | `GET /api/v1/scenes/item/copy` | operate |  | 1 | Copy an item into another scene. The copy keeps the transform and the filters and gets a new id. |
| `scene.item.cover_canvas` | `POST /api/v1/scenes/item/cover_canvas` | operate |  | 1 | Put items over the whole canvas, filling it and letting the overflow go. |
| `scene.item.distribute` | `POST /api/v1/scenes/item/distribute` | operate |  | 1 | Space items evenly between the two on the ends, horizontally or vertically. |
| `scene.item.filter.add` | `POST /api/v1/scenes/item/filter/add` | operate |  | 1 | Hang a filter on one item, so a camera keyed in one scene is not keyed in all of them. |
| `scene.item.filter.remove` | `POST /api/v1/scenes/item/filter/remove` | operate | yes | 1 | Take a filter off an item. |
| `scene.item.filter.set` | `POST /api/v1/scenes/item/filter/set` | operate |  | 1 | Change one of an item's filters, or turn it off without taking it out. |
| `scene.item.fit_to_canvas` | `POST /api/v1/scenes/item/fit_to_canvas` | operate |  | 1 | Put items over the whole canvas, keeping their aspect ratio inside it. |
| `scene.item.group` | `POST /api/v1/scenes/item/group` | operate |  | 1 | Put items into a group. The picture does not change. |
| `scene.item.match_size` | `POST /api/v1/scenes/item/match_size` | operate |  | 1 | Make items the same size as another one. |
| `scene.item.move` | `POST /api/v1/scenes/item/move` | operate |  | 1 | Move an item to another scene, keeping its transform and filters. |
| `scene.item.remove` | `POST /api/v1/scenes/item/remove` | operate | yes | 1 | Take an item off a scene. |
| `scene.item.reorder` | `POST /api/v1/scenes/item/reorder` | operate |  | 1 | Move an item up or down the stack, between two named neighbours. |
| `scene.item.schema` | `GET /api/v1/scenes/item/schema` | read |  | 1 | What one item type takes: a graphic's OGraf schema, or a source or filter plugin's settings schema. The same JSON Schema every client renders an inspector from. |
| `scene.item.set` | `POST /api/v1/scenes/item/set` | operate |  | 1 | Assign an item's properties. Only the keys named move; the rest are left alone, so calling it twice with the same body changes nothing the second time. |
| `scene.item.ungroup` | `POST /api/v1/scenes/item/ungroup` | operate |  | 1 | Take a group apart, leaving every child exactly where it looked. |
| `scene.layout.copy` | `GET /api/v1/scenes/layout/copy` | read |  | 1 | Read one scene's geometry, to paste onto another. |
| `scene.layout.list` | `GET /api/v1/scenes/layout/list` | read |  | 1 | The layouts that ship with the core, with the parameters each one takes. |
| `scene.layout.paste` | `POST /api/v1/scenes/layout/paste` | operate |  | 1 | Put one scene's geometry onto another's items, matched by name first and slot order second. Items that match nothing are left alone. |
| `scene.list` | `GET /api/v1/scenes` | read |  | 1 | Every scene in the collection, with how many items it has, the sources it draws and whether it is armed. |
| `scene.params.get` | `GET /api/v1/scenes/params/get` | read |  | 1 | The collection's typed parameters, readable without their values, so a client discovers what is fillable before filling it. |
| `scene.params.set` | `POST /api/v1/scenes/params/set` | operate |  | 1 | Set the collection's parameter values, declaring any that are new. A `{{name}}` in any string property of any item follows them, so one call changes every lower third that uses it. |
| `scene.preview.frame` | `GET /api/v1/scenes/preview/frame` | read |  | 1 | A still of the armed scene as base64 JPEG, the floor every client has. |
| `scene.preview.set` | `POST /api/v1/scenes/preview/set` | operate |  | 1 | Arm a scene. The armed scene is the preview, and program.take with no argument takes it. |
| `scene.redo` | `POST /api/v1/scenes/redo` | operate |  | 1 | Put back what undo took away. |
| `scene.remove` | `DELETE /api/v1/scenes/{id}` | operate | yes | 1 | Delete a scene. What is on air is not touched. |
| `scene.rename` | `POST /api/v1/scenes/{id}/rename` | operate |  | 1 | Change a scene's name, its colour, or both. Names and colours live on the document, so every client, the tally and an agent see the same ones. |
| `scene.transaction.abort` | `POST /api/v1/scenes/transaction/abort` | operate |  | 1 | Throw the batch away. The document goes back to where it was when the batch opened. |
| `scene.transaction.begin` | `POST /api/v1/scenes/transaction/begin` | operate |  | 1 | Start a batch. Everything until the commit applies on one frame or not at all, and undoes in one step. |
| `scene.transaction.commit` | `POST /api/v1/scenes/transaction/commit` | operate |  | 1 | Apply the batch. |
| `scene.undo` | `POST /api/v1/scenes/undo` | operate |  | 1 | Undo the last change. A drag marked with scene.history.mark undoes as one step. |
| `scene.validate` | `GET /api/v1/scenes/validate` | read |  | 1 | Overlaps, items off the canvas, safe area breaches and missing sources: what to fix before saying a scene is done. |
| `snapshot.get` | `GET /api/v1/snapshot/{id}` | read |  | 1 | One JPEG: the whole contact sheet, the programme, or one source cut out of the mosaic. |
| `source.add` | `POST /api/v1/sources` | operate |  | 1 | Add a source while the mixer runs. Answers with the id it got and the whole source record. |
| `source.audio.set` | `POST /api/v1/sources/{id}/audio` | operate |  | 1 | Move a source's audio: the fader, the mute, and for a superimposed page the balance between its own sound and the videos under it. |
| `source.duplicate` | `POST /api/v1/sources/{id}/duplicate` | operate |  | 1 | Add another source like one the mixer has: the same address and settings under a new id. A client cannot do this with source.add, because the address it is shown has everything after the host cut off. |
| `source.get` | `GET /api/v1/sources/{id}` | read |  | 1 | One source. Refused with the ids that exist when there is no such source. |
| `source.group` | `POST /api/v1/sources/{id}/group` | operate |  | 1 | Put sources in a tray folder. A tag for finding things, not a group on the canvas. |
| `source.list` | `GET /api/v1/sources` | read |  | 1 | Every source, with its state, whether it has video and audio, and its fader. |
| `source.remove` | `DELETE /api/v1/sources/{id}` | operate | yes | 1 | Remove a source. If it is on programme the mixer cuts to the slate first. |
| `source.restore` | `POST /api/v1/sources/{id}/restore` | operate |  | 1 | Put back a source that source.remove took away, as it was: same id, address, settings, fader and mute. The mixer remembers the last sixteen it removed, until it restarts. |
| `source.seek` | `POST /api/v1/sources/{id}/seek` | operate |  | 1 | Move a seekable source to a position. Answers with where it actually landed. |
| `source.set` | `POST /api/v1/sources/{id}/set` | operate |  | 1 | Change a running source: its name and colour, its params, or where it runs. The name and colour live on the scene document. Moving a source between the core, a sidecar and a node is `place`; the programme keeps its frame rate across the move and the compositor covers the swap. |
| `task.cancel` | `POST /api/v1/task/cancel` | operate |  | 1 | Ask a piece of long running work to stop. Cooperative: the answer says the request landed, not that the work has stopped yet. |
| `task.get` | `GET /api/v1/task` | read |  | 1 | How a piece of long running work is getting on, and its answer once it has one. |
| `task.list` | `GET /api/v1/task/list` | read |  | 1 | Every background job this core knows about, newest first. |
| `tool.call` | `POST /api/v1/tool/call` | operate |  | 1 | Call one of a plugin's tools, in MCP's shape. The name is `<plugin>/<tool>`, or the bare tool name when only one plugin has it. |

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

#### `core.restart`

Stop the mixer and have it started again, when something will start it again. On a supervised core (core.info restart.possible) it answers restarting: true and exits; the programme is off air until it is back. On a core started by hand it answers restarting: false, says how to restart it, and keeps running.

```json
{
  "params": {
    "additionalProperties": false,
    "properties": {},
    "type": "object"
  },
  "result": {
    "$ref": "#/$defs/RestartAnswer"
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

#### `device.discover`

Ask every device plugin what it can see: cameras, NDI senders, publishers. Each candidate's params are ready for source.add.

MCP tool `discover_sources` in the `search` profile: readOnlyHint false, destructiveHint false, idempotentHint true.

```json
{
  "params": {
    "$ref": "#/$defs/DiscoverRequest"
  },
  "result": {
    "type": "object"
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

#### `node.discover`

Look for nodes on the local network over mDNS. A network without multicast finds nothing and the [nodes] table in the config is the way there.

```json
{
  "params": {
    "$ref": "#/$defs/DiscoverRequest2"
  },
  "result": {
    "$ref": "#/$defs/DiscoverAnswer"
  }
}
```

#### `node.enrol`

Mint a one time enrolment token for a node. The answer carries the command to run on the other machine. The token is good for one enrolment and expires.

```json
{
  "params": {
    "$ref": "#/$defs/EnrolRequest"
  },
  "result": {
    "type": "object"
  }
}
```

#### `node.get`

One node: its clock offset, how long since its last heartbeat, the plugins it has, and the instances it is hosting.

```json
{
  "params": {
    "$ref": "#/$defs/NodeName"
  },
  "result": {
    "$ref": "#/$defs/NodeView"
  }
}
```

#### `node.list`

Every node this core knows about: the ones connected now, the ones that have gone quiet, and the ones the config expects that have never dialled in.

MCP tool `list_nodes` in the `search` profile: readOnlyHint true, destructiveHint false, idempotentHint true.

```json
{
  "params": {
    "additionalProperties": false,
    "properties": {},
    "type": "object"
  },
  "result": {
    "$ref": "#/$defs/NodeListing"
  }
}
```

#### `node.remove`

Forget a node. Its bridge is closed, every token minted for a plugin on it is revoked, and its certificate stops working. Sources placed on it go to the slate until they are moved or the node enrols again.

```json
{
  "params": {
    "$ref": "#/$defs/NodeName"
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

#### `output.set`

Change a destination in place: a new address with a new stream key, a new reconnect policy, a deeper outage buffer. The address is write only, so a client that only wants the buffer never has to hold the key.

MCP tool `set_output` in the `search` profile: readOnlyHint false, destructiveHint false, idempotentHint true.

```json
{
  "params": {
    "$ref": "#/$defs/SetOutputRequest"
  },
  "result": {
    "$ref": "#/$defs/OutputStatus"
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

#### `plugin.add`

Install a plugin, while live, from any source form: a GitHub release (owner/repo), a git URL, cargo:, npm:, pypi:, a local directory, or a bare name looked up in the marketplaces this mixer knows. The signature and the api level are checked before anything is copied.

```json
{
  "params": {
    "$ref": "#/$defs/AddPluginRequest"
  },
  "result": {
    "$ref": "#/$defs/PluginRecord"
  }
}
```

#### `plugin.describe`

One plugin in full: its manifest, the settings schema of every provide, and the description from each SKILL.md.

MCP tool `describe_plugin` in the `search` profile: readOnlyHint true, destructiveHint false, idempotentHint true.

```json
{
  "params": {
    "$ref": "#/$defs/PluginName"
  },
  "result": {
    "$ref": "#/$defs/PluginDescription"
  }
}
```

#### `plugin.disable`

Turn a plugin off without uninstalling it. It registers nothing and runs no process until it is enabled again.

```json
{
  "params": {
    "$ref": "#/$defs/PluginName"
  },
  "result": {
    "$ref": "#/$defs/PluginRecord"
  }
}
```

#### `plugin.enable`

Turn a plugin back on. It registers what it declares and its instances start.

```json
{
  "params": {
    "$ref": "#/$defs/PluginName"
  },
  "result": {
    "$ref": "#/$defs/PluginRecord"
  }
}
```

#### `plugin.list`

Every plugin installed, with what it provides and what each running instance is costing in cpu, memory, latency, dropped buffers and restarts.

MCP tool `list_plugins` in the `search` profile: readOnlyHint true, destructiveHint false, idempotentHint true.

```json
{
  "params": {
    "additionalProperties": false,
    "properties": {},
    "type": "object"
  },
  "result": {
    "$ref": "#/$defs/PluginListing"
  }
}
```

#### `plugin.reload`

Read a plugin's directory again and swap its running instances one at a time, with the freeze frame covering each.

```json
{
  "params": {
    "$ref": "#/$defs/PluginName"
  },
  "result": {
    "$ref": "#/$defs/PluginRecord"
  }
}
```

#### `plugin.remove`

Uninstall a plugin and unwind everything it registered: its provides, its tools, its panels, its hooks and its discovery matchers.

```json
{
  "params": {
    "$ref": "#/$defs/PluginName"
  },
  "result": {
    "$ref": "#/$defs/PluginRemoved"
  }
}
```

#### `plugin.search`

Search every marketplace this mixer knows for a plugin, by name, description or kind. Answers what `gmx plugin add <name>` would install.

MCP tool `search_plugins` in the `search` profile: readOnlyHint true, destructiveHint false, idempotentHint true.

```json
{
  "params": {
    "$ref": "#/$defs/SearchRequest"
  },
  "result": {
    "$ref": "#/$defs/SearchResults"
  }
}
```

#### `plugin.settings.get`

A plugin's settings as they stand, with its schema beside them.

```json
{
  "params": {
    "$ref": "#/$defs/PluginName"
  },
  "result": {
    "$ref": "#/$defs/PluginSettings"
  }
}
```

#### `plugin.settings.set`

Change a plugin's settings. A plugin that cannot take a change while running says so rather than being restarted behind your back.

```json
{
  "params": {
    "$ref": "#/$defs/SetSettingsRequest"
  },
  "result": {
    "$ref": "#/$defs/PluginSettings"
  }
}
```

#### `plugin.stats`

Per instance cpu, memory, media latency, dropped buffers and restarts, refreshed once a second.

```json
{
  "params": {
    "additionalProperties": false,
    "properties": {},
    "type": "object"
  },
  "result": {
    "$ref": "#/$defs/StatsListing"
  }
}
```

#### `plugin.update`

Fetch a newer build of a plugin, install it beside the one that is running, and prove it starts. A build that does not answer `initialize` within ten seconds is rolled back and the plugin that was working stays working.

```json
{
  "params": {
    "$ref": "#/$defs/UpdatePluginRequest"
  },
  "result": {
    "$ref": "#/$defs/PluginUpdated"
  }
}
```

#### `preset.apply`

Put a preset on this core: its config, its scenes, its layout, its theme and its gallery mode. Pass dry_run to get the plan and write nothing.

MCP tool `apply_preset` in the `search` profile: readOnlyHint false, destructiveHint true, idempotentHint true.

```json
{
  "params": {
    "$ref": "#/$defs/ApplyRequest"
  },
  "result": {
    "$ref": "#/$defs/ApplyResult"
  }
}
```

#### `preset.list`

Every preset this core can apply: the six built in, plus anything installed beside the binary or under ~/.godwinmix/presets.

MCP tool `list_presets` in the `search` profile: readOnlyHint true, destructiveHint false, idempotentHint true.

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

#### `preset.save`

Turn this core's working setup into a preset directory somebody else can apply. Stream keys and the control token are replaced with placeholders.

```json
{
  "params": {
    "$ref": "#/$defs/SaveRequest"
  },
  "result": {
    "type": "object"
  }
}
```

#### `preview.close`

Give up a raw frame socket. The socket goes when the last holder closes it.

```json
{
  "params": {
    "$ref": "#/$defs/PreviewOpenRequest"
  },
  "result": {
    "$ref": "#/$defs/PreviewClosed"
  }
}
```

#### `preview.open`

Open a raw frame socket on this machine for a source or the programme, and answer with its path. No encode anywhere: a client on the same host reads the frames the mixer already has. Close it with preview.close.

```json
{
  "params": {
    "$ref": "#/$defs/PreviewOpenRequest"
  },
  "result": {
    "$ref": "#/$defs/PreviewSocket"
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

Put a scene or a source on programme. The cut is instant and the outgoing stream is not disturbed.

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

#### `scene.add`

Make an empty scene, or one built from a set of sources.

```json
{
  "params": {
    "$ref": "#/$defs/AddSceneRequest"
  },
  "result": {
    "$ref": "#/$defs/SceneView"
  }
}
```

#### `scene.apply_graphic`

Fill a graphic that is on a scene, by field name, and optionally play it on or take it off. Answers with the records and, if asked, a still.

MCP tool `apply_graphic` in the `search` profile: readOnlyHint false, destructiveHint false, idempotentHint true.

```json
{
  "params": {
    "$ref": "#/$defs/ApplyGraphicRequest"
  },
  "result": {
    "type": "object"
  }
}
```

#### `scene.apply_layout`

Apply a layout, making a scene or reshaping one that exists. Applying onto an existing scene keeps the item ids, so the change is a ramp and not a cut.

MCP tool `apply_layout` in the `search` profile: readOnlyHint false, destructiveHint false, idempotentHint true.

```json
{
  "params": {
    "$ref": "#/$defs/ApplyLayoutRequest"
  },
  "result": {
    "type": "object"
  }
}
```

#### `scene.create_from`

A scene from a set of sources, laid out by the built in layout for that count (full, two-box, three-box, quad, then a grid) or by a named one.

MCP tool `create_scene_from` in the `search` profile: readOnlyHint false, destructiveHint false, idempotentHint true.

```json
{
  "params": {
    "$ref": "#/$defs/CreateFromRequest"
  },
  "result": {
    "$ref": "#/$defs/SceneView"
  }
}
```

#### `scene.duplicate`

A copy of a scene with new ids throughout, so editing the copy cannot touch the original.

```json
{
  "params": {
    "$ref": "#/$defs/DuplicateSceneRequest"
  },
  "result": {
    "$ref": "#/$defs/SceneView"
  }
}
```

#### `scene.edit.apply`

Write a draft back into the live document.

```json
{
  "params": {
    "$ref": "#/$defs/DraftRequest"
  },
  "result": {
    "type": "object"
  }
}
```

#### `scene.edit.begin`

Take a working copy of a scene. Editing is off air by default: the draft is written back on the next take of that scene, or when you apply it.

```json
{
  "params": {
    "$ref": "#/$defs/EditBeginRequest"
  },
  "result": {
    "$ref": "#/$defs/DraftRecord"
  }
}
```

#### `scene.edit.discard`

Throw a draft away. The live document is untouched.

```json
{
  "params": {
    "$ref": "#/$defs/DraftRequest"
  },
  "result": {
    "type": "object"
  }
}
```

#### `scene.export`

The whole collection: as JSON, or as a zip bundle carrying its assets with a hash each, which is what you send somebody.

MCP tool `export_collection` in the `search` profile: readOnlyHint true, destructiveHint false, idempotentHint true.

```json
{
  "params": {
    "$ref": "#/$defs/ExportRequest"
  },
  "result": {
    "type": "object"
  }
}
```

#### `scene.get`

One scene: its records and where every item actually lands on the canvas.

MCP tool `get_scene` in the `search` profile: readOnlyHint true, destructiveHint false, idempotentHint true.

```json
{
  "params": {
    "$ref": "#/$defs/SceneRequest"
  },
  "result": {
    "$ref": "#/$defs/SceneView"
  }
}
```

#### `scene.graphic.list`

Every graphic template this core can place, with what each one takes.

MCP tool `list_graphics` in the `search` profile: readOnlyHint true, destructiveHint false, idempotentHint true.

```json
{
  "params": {
    "additionalProperties": false,
    "properties": {},
    "type": "object"
  },
  "result": {
    "$ref": "#/$defs/GraphicListing"
  }
}
```

#### `scene.history.mark`

Group the changes that follow into one undo step, until the next mark. This is what makes a drag of forty moves one Ctrl+Z.

```json
{
  "params": {
    "$ref": "#/$defs/MarkRequest"
  },
  "result": {
    "type": "object"
  }
}
```

#### `scene.import`

Read a collection bundle, a zip or the directory it unpacks to, and add its scenes to this one. Answers with a relink report for any asset that did not come across.

MCP tool `import_collection` in the `search` profile: readOnlyHint false, destructiveHint false, idempotentHint true.

```json
{
  "params": {
    "$ref": "#/$defs/ImportRequest"
  },
  "result": {
    "$ref": "#/$defs/ImportedReport"
  }
}
```

#### `scene.import.obs`

Read an OBS Studio scene collection and add its scenes to this one.

```json
{
  "params": {
    "$ref": "#/$defs/ImportObsRequest"
  },
  "result": {
    "$ref": "#/$defs/ImportReport"
  }
}
```

#### `scene.item.add`

Put something on a scene's canvas. With no transform it lands in the next free cell, so a drop never needs a dialog.

MCP tool `add_scene_item` in the `search` profile: readOnlyHint false, destructiveHint false, idempotentHint true.

```json
{
  "params": {
    "$ref": "#/$defs/AddItemRequest"
  },
  "result": {
    "type": "object"
  }
}
```

#### `scene.item.align`

Line items up on an edge: left, right, top, bottom, center-x or center-y.

```json
{
  "params": {
    "$ref": "#/$defs/ItemsRequest"
  },
  "result": {
    "type": "object"
  }
}
```

#### `scene.item.arrange_grid`

Lay items out in a grid of `cols` columns.

```json
{
  "params": {
    "$ref": "#/$defs/ItemsRequest"
  },
  "result": {
    "type": "object"
  }
}
```

#### `scene.item.bind`

Bind a geometry property to an expression over the collection's parameters, so changing a number moves everything that follows it.

```json
{
  "params": {
    "$ref": "#/$defs/BindRequest"
  },
  "result": {
    "type": "object"
  }
}
```

#### `scene.item.copy`

Copy an item into another scene. The copy keeps the transform and the filters and gets a new id.

```json
{
  "params": {
    "$ref": "#/$defs/MoveItemRequest"
  },
  "result": {
    "type": "object"
  }
}
```

#### `scene.item.cover_canvas`

Put items over the whole canvas, filling it and letting the overflow go.

```json
{
  "params": {
    "$ref": "#/$defs/ItemsRequest"
  },
  "result": {
    "type": "object"
  }
}
```

#### `scene.item.distribute`

Space items evenly between the two on the ends, horizontally or vertically.

```json
{
  "params": {
    "$ref": "#/$defs/ItemsRequest"
  },
  "result": {
    "type": "object"
  }
}
```

#### `scene.item.filter.add`

Hang a filter on one item, so a camera keyed in one scene is not keyed in all of them.

```json
{
  "params": {
    "$ref": "#/$defs/AddItemFilterRequest"
  },
  "result": {
    "type": "object"
  }
}
```

#### `scene.item.filter.remove`

Take a filter off an item.

```json
{
  "params": {
    "$ref": "#/$defs/ItemFilterRequest"
  },
  "result": {
    "type": "object"
  }
}
```

#### `scene.item.filter.set`

Change one of an item's filters, or turn it off without taking it out.

```json
{
  "params": {
    "$ref": "#/$defs/ItemFilterRequest"
  },
  "result": {
    "type": "object"
  }
}
```

#### `scene.item.fit_to_canvas`

Put items over the whole canvas, keeping their aspect ratio inside it.

```json
{
  "params": {
    "$ref": "#/$defs/ItemsRequest"
  },
  "result": {
    "type": "object"
  }
}
```

#### `scene.item.group`

Put items into a group. The picture does not change.

```json
{
  "params": {
    "$ref": "#/$defs/ItemsRequest"
  },
  "result": {
    "type": "object"
  }
}
```

#### `scene.item.match_size`

Make items the same size as another one.

```json
{
  "params": {
    "$ref": "#/$defs/ItemsRequest"
  },
  "result": {
    "type": "object"
  }
}
```

#### `scene.item.move`

Move an item to another scene, keeping its transform and filters.

```json
{
  "params": {
    "$ref": "#/$defs/MoveItemRequest"
  },
  "result": {
    "type": "object"
  }
}
```

#### `scene.item.remove`

Take an item off a scene.

```json
{
  "params": {
    "$ref": "#/$defs/ItemRequest"
  },
  "result": {
    "type": "object"
  }
}
```

#### `scene.item.reorder`

Move an item up or down the stack, between two named neighbours.

```json
{
  "params": {
    "$ref": "#/$defs/ReorderRequest"
  },
  "result": {
    "type": "object"
  }
}
```

#### `scene.item.schema`

What one item type takes: a graphic's OGraf schema, or a source or filter plugin's settings schema. The same JSON Schema every client renders an inspector from.

MCP tool `scene_item_schema` in the `search` profile: readOnlyHint true, destructiveHint false, idempotentHint true.

```json
{
  "params": {
    "$ref": "#/$defs/ItemSchemaRequest"
  },
  "result": {
    "type": "object"
  }
}
```

#### `scene.item.set`

Assign an item's properties. Only the keys named move; the rest are left alone, so calling it twice with the same body changes nothing the second time.

MCP tool `set_scene_item` in the `search` profile: readOnlyHint false, destructiveHint false, idempotentHint true.

```json
{
  "params": {
    "$ref": "#/$defs/SetItemRequest"
  },
  "result": {
    "type": "object"
  }
}
```

#### `scene.item.ungroup`

Take a group apart, leaving every child exactly where it looked.

```json
{
  "params": {
    "$ref": "#/$defs/ItemRequest"
  },
  "result": {
    "type": "object"
  }
}
```

#### `scene.layout.copy`

Read one scene's geometry, to paste onto another.

```json
{
  "params": {
    "$ref": "#/$defs/SceneRequest"
  },
  "result": {
    "$ref": "#/$defs/Layout"
  }
}
```

#### `scene.layout.list`

The layouts that ship with the core, with the parameters each one takes.

MCP tool `list_layouts` in the `search` profile: readOnlyHint true, destructiveHint false, idempotentHint true.

```json
{
  "params": {
    "additionalProperties": false,
    "properties": {},
    "type": "object"
  },
  "result": {
    "$ref": "#/$defs/LayoutListing"
  }
}
```

#### `scene.layout.paste`

Put one scene's geometry onto another's items, matched by name first and slot order second. Items that match nothing are left alone.

```json
{
  "params": {
    "$ref": "#/$defs/LayoutClipboardRequest"
  },
  "result": {
    "type": "object"
  }
}
```

#### `scene.list`

Every scene in the collection, with how many items it has, the sources it draws and whether it is armed.

MCP tool `list_scenes` in the `search` profile: readOnlyHint true, destructiveHint false, idempotentHint true.

```json
{
  "params": {
    "additionalProperties": false,
    "properties": {},
    "type": "object"
  },
  "result": {
    "$ref": "#/$defs/SceneListing"
  }
}
```

#### `scene.params.get`

The collection's typed parameters, readable without their values, so a client discovers what is fillable before filling it.

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

#### `scene.params.set`

Set the collection's parameter values, declaring any that are new. A `{{name}}` in any string property of any item follows them, so one call changes every lower third that uses it.

MCP tool `set_scene_params` in the `search` profile: readOnlyHint false, destructiveHint false, idempotentHint true.

```json
{
  "params": {
    "$ref": "#/$defs/ParamsRequest"
  },
  "result": {
    "type": "object"
  }
}
```

#### `scene.preview.frame`

A still of the armed scene as base64 JPEG, the floor every client has.

```json
{
  "params": {
    "$ref": "#/$defs/PreviewFrameRequest"
  },
  "result": {
    "type": "object"
  }
}
```

#### `scene.preview.set`

Arm a scene. The armed scene is the preview, and program.take with no argument takes it.

MCP tool `arm_preview` in the `search` profile: readOnlyHint false, destructiveHint false, idempotentHint true.

```json
{
  "params": {
    "$ref": "#/$defs/PreviewRequest"
  },
  "result": {
    "type": "object"
  }
}
```

#### `scene.redo`

Put back what undo took away.

```json
{
  "params": {
    "additionalProperties": false,
    "properties": {},
    "type": "object"
  },
  "result": {
    "$ref": "#/$defs/HistoryStep"
  }
}
```

#### `scene.remove`

Delete a scene. What is on air is not touched.

```json
{
  "params": {
    "$ref": "#/$defs/SceneRequest"
  },
  "result": {
    "$ref": "#/$defs/SceneRemoved"
  }
}
```

#### `scene.rename`

Change a scene's name, its colour, or both. Names and colours live on the document, so every client, the tally and an agent see the same ones.

```json
{
  "params": {
    "$ref": "#/$defs/RenameSceneRequest"
  },
  "result": {
    "$ref": "#/$defs/SceneView"
  }
}
```

#### `scene.transaction.abort`

Throw the batch away. The document goes back to where it was when the batch opened.

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

#### `scene.transaction.begin`

Start a batch. Everything until the commit applies on one frame or not at all, and undoes in one step.

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

#### `scene.transaction.commit`

Apply the batch.

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

#### `scene.undo`

Undo the last change. A drag marked with scene.history.mark undoes as one step.

MCP tool `undo_scene_edit` in the `search` profile: readOnlyHint false, destructiveHint false, idempotentHint true.

```json
{
  "params": {
    "additionalProperties": false,
    "properties": {},
    "type": "object"
  },
  "result": {
    "$ref": "#/$defs/HistoryStep"
  }
}
```

#### `scene.validate`

Overlaps, items off the canvas, safe area breaches and missing sources: what to fix before saying a scene is done.

MCP tool `validate_scene` in the `search` profile: readOnlyHint true, destructiveHint false, idempotentHint true.

```json
{
  "params": {
    "$ref": "#/$defs/ValidateRequest"
  },
  "result": {
    "$ref": "#/$defs/Validation"
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

#### `source.duplicate`

Add another source like one the mixer has: the same address and settings under a new id. A client cannot do this with source.add, because the address it is shown has everything after the host cut off.

```json
{
  "params": {
    "$ref": "#/$defs/DuplicateSourceRequest"
  },
  "result": {
    "$ref": "#/$defs/SourceStatus"
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

#### `source.group`

Put sources in a tray folder. A tag for finding things, not a group on the canvas.

```json
{
  "params": {
    "$ref": "#/$defs/GroupSourcesRequest"
  },
  "result": {
    "type": "object"
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

#### `source.restore`

Put back a source that source.remove took away, as it was: same id, address, settings, fader and mute. The mixer remembers the last sixteen it removed, until it restarts.

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

#### `source.set`

Change a running source: its name and colour, its params, or where it runs. The name and colour live on the scene document. Moving a source between the core, a sidecar and a node is `place`; the programme keeps its frame rate across the move and the compositor covers the swap.

MCP tool `set_source` in the `search` profile: readOnlyHint false, destructiveHint false, idempotentHint true.

```json
{
  "params": {
    "$ref": "#/$defs/SetSourceRequest"
  },
  "result": {
    "$ref": "#/$defs/SourceStatus"
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

#### `tool.call`

Call one of a plugin's tools, in MCP's shape. The name is `<plugin>/<tool>`, or the bare tool name when only one plugin has it.

```json
{
  "params": {
    "$ref": "#/$defs/ToolCallRequest"
  },
  "result": {
    "type": "object"
  }
}
```

## Events

Subscribe with `core.subscribe`. Patterns match the part after `event/`, so `program.*` matches `event/program.took`. Every event carries `seq`; every batch ends with `event/flush`; a client that falls behind gets `event/resync`.

| Event | ext | Replaces on /ws | What it carries |
|---|---|---|---|
| `event/snapshot` |  | `status` | The full state, and the sequence number it is current as of. Sent on subscribe and after any change the deltas cannot describe. |
| `event/program.took` |  | `took` | The programme changed. Carries the running time the cut landed on, so a client can see how close a scheduled take was to its mark. |
| `event/scene.patch` |  |  | One change to the scene document, as records rather than a snapshot: what was added, what changed with its before and after, and what was removed. One per transaction, batched and ended by event/flush. |
| `event/preview.changed` |  |  | A scene was armed, or the arming was cleared. The armed scene is the preview, and program.take with no argument takes it. |
| `event/source.state` |  | `source_state_changed` | A source moved between connecting, live, stalled and failed. |
| `event/source.position` | `positions` | `source_position` | How far through a seekable source has got, a few times a second. Never sent for a camera, which has no position to report. |
| `event/output.state` |  | `output_state_changed` | A destination connected, dropped or is retrying. |
| `event/adbreak.changed` |  | `ad_break_changed` | An ad break was armed, went on air, or ended. |
| `event/ui.changed` |  |  | The surface defaults changed: a preset was applied, or an operator set the layout, theme or gallery mode by hand. Nothing on air moves. |
| `event/hook.blocked` |  |  | A hook did not get its say: it did not answer inside its timeout, or the thing behind it could not be reached. Whatever the hook was attached to went ahead anyway, which is the rule that keeps a slow hook off the frame path. See 03 section 8. |
| `event/media.changed` |  | `media_changed` | A file in the library was uploaded, deleted, or its conversion moved on. |
| `event/meters` | `meters` | `audio_level, source_audio_level` | Peak dBFS for the programme bus and every source, in one message at 10 per second. Replaces the two separate meter events on /ws. |
| `event/tally` | `tally` |  | Which sources are on programme, on preview, or off. Derived by the core so a Stream Deck does not have to. |
| `event/alert` |  | `alert` | Something an operator should see. Also written to the log and to the alert webhook. |
| `event/telemetry` | `telemetry` |  | Numbers instead of a picture, up to ten times a second and under 200 bytes: the shot change score, the black ratio, a freeze flag, short term and integrated loudness, a silence flag and which sources are live. From cheap probes on the raw programme frames, which run only while a client is subscribed. |
| `event/agent.state` | `agent` |  | The agent.state document, pushed when a telemetry threshold crosses or a take lands, with `why` naming which and a snapshot URL beside it. Edge triggered and at most one a second, so a picture that stays black is one message rather than one a tick. |
| `event/multiview.layout` | `multiview` |  | How to read the binary frames that follow: the cells, and the layout id carried in every frame header. |
| `event/multiview.frame` | `multiview` | `raw JPEG binary frame` | A mosaic frame, as a binary WebSocket frame rather than JSON: a 16 byte little endian header (seq u32, layout id u32, programme running time in milliseconds u64) then the JPEG. The top bit of seq is the stream and is clear on a mosaic frame; the other 31 bits count. |
| `event/preview.frame` | `preview` |  | The armed scene as a picture, on the same socket and in the same 16 byte header as a mosaic frame, with the top bit of seq set to say so and the layout id zero because there is no grid to cut up. One picture per frame: draw it whole. |
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
| `preview` | `{fps, width} or "full"` | event/preview.frame: the armed scene, composited in the multiview pipeline from the per source thumbnails and published on this socket, to /mjpeg/preview, to scene.preview.frame and to its own cell on the mosaic. "full" composites it at the canvas's own size while a client is subscribed, so a designer's handles land on real coordinates | yes |
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

