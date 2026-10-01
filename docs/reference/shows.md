# Reference: shows and the station

A show is one independent programme: its own scenes, sources, programme
encode and outputs. The process a person starts is the station. It runs each
show as a process of its own and stands in front of all of them on the one
control port. For how to use this, see
[run several shows](../how-to/run-several-shows.md); for why it is built this
way, see [the crate map](../explanation/architecture.md#the-station-and-its-shows).

## Processes

| Process | Started by | Listens on | Owns |
|---|---|---|---|
| The station | a person, a service unit, the desktop app: `godwinmix --config godwinmix.toml` | the control port from `[control] bind` | the page, `/rpc`, `/api/v1`, WHIP ingest, the channels and the ingest plugin, the governor, the list of shows |
| A show | the station: `godwinmix --config <its config> --show <id> --station <link> --supervised --bind 127.0.0.1:0` | a loopback port it picks and tells the station | its mixer, its scenes, sources and outputs, and every plugin but the ingest one |
| A single process core | a person: `godwinmix --config godwinmix.toml --show main` | the control port | everything, as every release before shows had |

Without `--show`, the binary is a station. `--station` is passed by the
station only and is hidden from `--help`.

## Where each show's files are

| Show | Config | Beside it |
|---|---|---|
| `main` | the file the station was started with, in place | its runtime store and scenes, where they always were |
| any other | `<data dir>/shows/<id>/godwinmix.toml` | its own runtime store, scenes and `.godwinmix/` runtime folder |

The data dir is the folder the station's config is in. The list of shows is
`<data dir>/shows.json`, written only once it says something a bare config
cannot: a second show, or `main` renamed or stopped. The channels file and the
governor's calibration stay the station's. When `GODWINMIX_RUNTIME_DIR` is
set, a show other than `main` gets `<that dir>/shows/<id>`.

## Addressing a show

Every method and route that is not the station's addresses one show:

| Door | How the show is named |
|---|---|
| `/rpc` | `?show=<id>` on the URL, or `show` in `core.subscribe`, which moves the connection to that show from then on |
| `/api/v1/...` and the legacy `/api/...` | `?show=<id>` |
| the streams (`/mjpeg/...`, `/pcm/...`, `/whep/...`, snapshots, HLS) and their WebSockets | `?show=<id>` |

With none, the request goes to the first show, which is `main`. The station
takes `show` off the query before the show sees it. A call to a show that is
starting waits up to 15 seconds for it; a call to one that is stopped or
failed is refused with `not_in_state`, `data.show`, `data.state` and an `open`
action for the shows panel.

The station answers these itself, whatever `show` says: `show.*`,
`channel.*`, `governor.status`, `governor.calibrate`, and `rendition.plan`
with a `channel:<id>` scope.

## Methods

### `show.list {}`

```json
{
  "current": "main",
  "shows": [
    { "id": "main", "name": "Main", "state": "running", "on_air": "live-main",
      "programme_kbps": 0, "cpu_millicores": 12, "memory_mib": 132, "restarts": 0 },
    { "id": "second-room", "name": "Second room", "state": "running", "on_air": "church",
      "programme_kbps": 0, "cpu_millicores": 119, "memory_mib": 201, "restarts": 1 }
  ]
}
```

| Field | Meaning |
|---|---|
| `state` | `starting`, `running`, `stopped` (a person stopped it; its config is kept) or `failed` (it kept dying and the station stopped trying) |
| `on_air` | the scene, or the source, on its programme. `null` on black and while it is not running |
| `programme_kbps` | what its live outputs report sending, summed. Zero from an output that reports no rate |
| `cpu_millicores` | its process's CPU in thousandths of one core, averaged since the previous `show.list`. Zero on the first read |
| `memory_mib` | its process's resident memory |
| `restarts` | how often the station started it again after it died |
| `error` | why it is not running, when a person did not ask for that |
| `compositing` | `true` for a show with scenes and a programme in a process of its own; `false` for a show without compositing (below) |
| `input` | what the show takes in, `{uri, program?, params?, backup?}`. Absent for a show that has none. An SRT passphrase reads `__secret__` here (see [Input passphrases](#input-passphrases)) |
| `outputs` | a show without compositing's outputs, each as a channel destination is shown: `id`, `platform`, `label`, `uri_host`, `has_key`, `enabled`, `state`, `kbps`, `reconnects`, `error`, and `rendition`, `plan` or `refused` when it converts. Absent for a show that composites |
| `health` | `{state, alarms}`, see [Health](#health) |

For a show without compositing, `state` is `running` unless it was stopped,
`on_air` is `"input"` while its input arrives, and `programme_kbps` is what
its outputs send.

Read scope. A read asks no show anything. What only a show's process can say
(`on_air`, `programme_kbps`) and what its process costs are measured once a
second by a sampler in the station while someone reads `show.list`, and for
ten seconds after the last read; every read is served from what it last
measured. The first read waits for one round. With 201 shows a read took
the times in [Two hundred shows](#two-hundred-shows).

### `show.add {name, compositing?, input?, outputs?, from?}`

Admin. Makes a show, starts it and answers with it in `starting`;
`event/show.changed` says when it is `running`. With `compositing: false` it
makes a show without compositing instead, which needs `input` and takes
`outputs` (see [Shows without compositing](#shows-without-compositing)); it
answers with it `running` at once, since there is no process to start. A
show that composites refuses `outputs`: add them inside it with `output.add`
and `?show=<id>`. A show that composites and has an `input` gets it as its one
source, `input`, put on its programme, once the ingest plugin has it.

| `from` | The new show starts with |
|---|---|
| absent, or `"empty"` | the config a first run writes |
| `"<show id>"` | a copy of that show's config, runtime store and scenes, without its outputs, so nothing goes out twice |
| `{"project": <file>}` | an empty show, into which the file is opened with `project.import` once it is running. A build without `project.import` refuses this before it makes anything |

The id is made from the name: `Second room` is `second-room`, then
`second-room-2`.

### `show.rename {id, name}`

Admin. The id stays.

### `show.remove {id}`

Admin, destructive. Stops the show and removes it with its folder. Refused
for the last show, and for `main`, whose config is the station's own: stop it
instead.

### `show.start {id}` and `show.stop {id}`

Admin. A stopped show keeps its config and stays stopped across a restart of
the station until `show.start`. A show that failed starts again with
`show.start`. A show without compositing that is stopped is taken out of the
direct host's table, so its input is closed and its outputs stop.

### `show.set {id, name?, input?, compositing?}`

Admin. Names only what moves. The name and the input change first, then
compositing, so one call can give a show an input and turn compositing off.
The answer is the show with a `switch` beside its fields when compositing
moved:

| `switch` field | Meaning |
|---|---|
| `compositing` | what the show does now |
| `outputs` | the ids of the outputs that moved |
| `gap_ms` | from the moment the outputs stopped where they were to the moment every one was live again where they went. Absent when they were not all live within 30 seconds, or there were none |
| `note` | an output that could not be moved, or why the station did not wait |

`compositing: true` on a show without compositing: the direct host stops its
outputs and keeps its input open in its hub, the station writes the show a
folder and starts its process, gives it the input as its one source (read
from the hub, so the input is opened once) and adds the outputs to it with
`output.add`. Outputs move break then make, because a platform takes one
publisher per key.

`compositing: false` on a show that composites, refused with `not_in_state`
and the reason in `data` when:

| Refused when | `data` |
|---|---|
| it is `main` | `show` |
| it has no input and the call gives none | `field: "input"` |
| it is not running, so its sources and outputs cannot be read | `show` |
| it has sources besides `input` | `sources` |
| a scene is on its programme | `scene` |
| it has an output that was added inside it, whose address is write only | `outputs` |

Otherwise its outputs are removed from the show, handed back to the direct
host and its process is stopped.

Measured on macOS, Apple silicon, release build, a 720p30 H.264 feed over UDP
copied to an RTMP server (ffmpeg listening), with the direct host running:
`gap_ms` was 608, 617, 704 and 692 turning compositing on, and 563, 461 and
1174 turning it off. It is timed by the station, from the moment the
outputs were taken away to the moment the side that took them over reported
every one live. Its folder stays, so turning compositing on
again finds its config.

### `show.add_many {shows, dry_run?}`

Admin. `shows` is a list of what `show.add` takes, at most 1000. The batch
is checked whole first: every show's name, input and outputs by the same
rules as `show.add`, and every rendition priced by the planner against an
input shaped like a broadcast HD feed (H.264 1920x1080 30 fps with stereo
AAC), since an input's real shape is known only once it arrives. The prices
are added up in order against what the governor says is free now, taking no
ticket. With `dry_run` (the default) nothing is made; without it every show
that passed and fits is made, whole, and the host is handed its table once.

```json
{
  "dry_run": true,
  "added": ["bbc-one", "itv"],
  "refused": [
    {"index": 2, "name": "Broken", "why": "\"nonsense\" is not an input this machine can open. ...",
     "data": {"field": "input.uri", "schemes": ["udp", "rtp", "srt", "..."]}}
  ],
  "plan": {"cost": {"cpu_millicores": 290, "...": 0}, "have": {"cpu_millicores": 5200, "...": 0},
           "fits": false, "assumed_input": "H.264 1920x1080 30 fps with stereo AAC"}
}
```

`added` is what was made, or on a dry run what would be. A show with one
output that breaks a rule is refused whole, with `data.output_index` naming
the output. A show that would take the machine past what it has free is
refused with `data.alarm: "governor-refused"`, `data.need` and `data.have`.
`fits` is true only when nothing was refused.

### `show.remove_many {ids}`

Admin, destructive. `show.remove` for each id; an id that cannot go (`main`,
the last show, one not there) is in `refused` with why, and the rest go.

### `show.output.add {id, output?, platform?, label?, uri?, key?, enabled?, rendition?}`

Admin, on a show without compositing. `uri` is the whole address: `srt://`,
`rtmp://`, `udp://` (unicast or multicast), `rtp://` or `rist://`. A platform
(`youtube`, `facebook`, `twitch`) takes `key`, which is write only. `output`
is the new output's id, made from the label or the platform when left out.
No `rendition` copies the input's own bytes into the output's container; a
rendition request or `{"preset": "youtube-720p30"}` is planned with the
channels' planner and refused at once when it cannot be served. Answers the
show. `show` is taken as another name for `id`.

### `show.output.set {id, output, label?, uri?, key?, enabled?, rendition?}`

Admin. Names only what moves; a key left out is kept, and `rendition: null`
goes back to a copy. A UDP, RTP or RIST output keeps its scheme.

### `show.output.remove {id, output}`

Admin, destructive. Stops the output and forgets its address and key.

On a show that composites, the three are refused with `data.compositing:
true`; its outputs are `output.*` with `?show=<id>`.

### `show.stats {ids?, fields?}`

Read. Health and numbers for many shows in one read, every show when `ids`
is left out, narrowed by `fields` to any of `health`, `input`, `outputs`:

```json
{"shows": [{"id": "bbc-one",
  "health": {"state": "ok", "alarms": []},
  "input": {"kbps": 6100, "fps": 25.0, "width": 1920, "height": 1080, "video_codec": "h264",
            "audio_codec": "aac", "audio_channels": 2, "cc_errors": 0, "packets_lost": 0,
            "keyframe_ms": 1000, "last_frame_ms": 12},
  "outputs": [{"id": "srt", "state": "live", "kbps": 6050, "reconnects": 0,
               "rendition_text": "copy", "cpu_millicores": 0}]}]}
```

It reads what the station already holds (the direct host's last
`direct.stats`, about once a second, and the plan), so it asks nothing of a
show or of the host and is cheap to call every second for two hundred shows.
A field the host has not counted yet is left out. A show that composites has
its health and, when it has an input, the input's numbers; its outputs are
read with `output.list` and `?show=<id>`.

### Alarm settings: `show.set {id, alarms}`

`alarms` is `{enabled?, black_ms?, freeze_ms?, silence_ms?, silence_dbfs?}`.
The fields named move and the rest stay; the show carries the result as
`alarms`. `enabled` left out is on for a show without compositing and off for
one that composites. A duration of 0 switches that check off. The direct host
gets them in its table as thresholds; a field never set keeps the host's
default (black 4 s, freeze 10 s, silence 10 s under -60 dBFS).

A show that composites gets them too, for the checks it makes of its own
programme: the station calls the show's `vitals.set` when its process links
and again when `show.set` changes them while it runs, before it answers. The
milliseconds become seconds, `silence_dbfs` becomes `silence_db` and
`enabled` becomes `alarms`, which there means keeping a mosaic up for the
black and freeze checks while nobody is looking (see
[show health](show-health.md#thresholds)). `vitals.get` with `?show=<id>`
reads back what the show holds.

### `GET /api/v1/shows/{id}/thumbnail.jpg?width=160`

Read, with the token as a header or `?token=`. The show's picture as a JPEG,
32 to 1280 pixels wide. For a show without compositing it comes from the
direct host, which decodes keyframes only, about one a second, and only for a
show someone asked a picture of lately; for a show that composites it is its
programme snapshot. `409` with `data.retry_after_ms` while there is no picture
yet (the host has decoded no keyframe, the ingest plugin is not running, the
show is not running); `404` for a show that is not there.

### `governor.status`

As in the [renditions reference](renditions.md), with `ingress_kbps` beside
`egress_kbps`: every channel stream and every direct show's input, as last
counted.

## Shows without compositing

A show without compositing is one input sent straight to its outputs, with
no compositor and no process of its own. Two hundred channels from a headend
are two hundred of these. The station keeps each one in the list of shows,
its outputs' addresses and keys sealed in the secret store under
`show.<id>.output`, and hands the whole table of them to the direct host in
the ingest plugin, the way it hands the channel table: laid over the plugin's
settings as `direct` and pushed with `configure`, the whole table every time,
one table for a run of changes. The host demuxes the input and copies or
converts it to each output; the shapes between the two are
`dev/plans/wave4-direct-table.md`.

A show is in the table while it is not stopped and has an input. A show that
composites and has an input is in it too, with no outputs, so its input is
opened once and read by the show from the hub.

Every other method with `?show=<id>` of a show without compositing (a scene,
a source, the programme) is refused at once with `not_in_state` and
`data.compositing: false`.

### Input passphrases

An SRT passphrase in a show's input, in the address
(`srt://feed:9000?passphrase=...`) or in `params.passphrase`, of the input or
of its backup, is sealed in the secret store under `show.<id>.input` when the
input is given to `show.add`, `show.add_many` or `show.set`. The list of
shows on disk, `show.list` and `event/show.changed` carry the secret store's
sentinel in its place:

```json
{"uri": "srt://feed:9000?mode=caller&passphrase=__secret__&latency=200"}
```

Sending the input back with the sentinel still in it keeps the sealed
passphrase, so a form that edits the input without knowing the passphrase
cannot lose it. Sending one without the field forgets it. Only the table the
direct host opens the input from has the passphrase itself. A list written
before passphrases were sealed is sealed when the station starts, and
removing a show forgets its passphrases with its output keys.

## Health

```json
{"state": "alarm", "alarms": [{"kind": "no-input", "since_ms": 1759312800000, "detail": "nothing has arrived on the input yet"}]}
```

| `state` | When |
|---|---|
| `ok` | running and nothing wrong |
| `warning` | only `cc-errors` or `loss`: the feed is damaged but still arriving |
| `alarm` | any other alarm, including a show process that died, lost its link or failed |
| `off` | stopped, or a show that composites and has not linked to the station since it was started |

The direct host says `stall`, `black`, `freeze`, `silence`, `cc-errors` and
`loss`. The station adds `no-input` while the input has not arrived or the
ingest plugin is not running, `governor-refused` for an output whose
rendition the governor would not admit, and `output-failed` for an output the
host reports failed. `since_ms` is unix milliseconds.

A show that composites judges its own programme (black, freeze, silence, a
failed or shed output) and sends its health to the station over the link
whenever its state or set of alarm kinds changes. The station adds the
alarms of the show's input, when it has one, and serves the result the same
way as a direct show's. When the show's process dies, is killed or loses its
link, what it last said is dropped: it reads `alarm` with one `stall` alarm
from the moment the link closed until a new process says hello, and the same
once the station has given up on it (`failed`). The thresholds and what each
check costs are in [show health](show-health.md).

## Events

Sent by the station to every client, whichever show it is looking at, when
its subscription asks for them (`show.*` or `*`).

| Event | Payload | When |
|---|---|---|
| `event/show.changed` | `{show}`, a row of `show.list` without the measured fields | a show was added, renamed, started, stopped, died, came back or failed |
| `event/show.removed` | `{id}` | a show was removed |
| `event/show.health` | `{id, health}` | a show's health changed state, or an alarm began or ended. Never for a number alone, nor for an alarm whose detail changed |

A show without compositing is announced with `event/show.changed` when one
of its outputs changes state, error or reconnect count, and when its input
arrives, leaves or changes shape.

The station's alerts (`event/alert`) say when a show died and when it will be
started again, and when it was left failed.

## Restarts

| How a show's process ended | What the station does |
|---|---|
| status 0 (`core.shutdown`) | leaves it stopped. When it was the only show, the station stops too, as the single process did |
| status 75 (`core.restart`) | starts it again at once, not counted |
| anything else (a crash, `SIGKILL`) | starts it again: three times at once, then after 30 seconds doubling, the plugin host's backoff. The seventh death in a row leaves it `failed`, with an alert. A show that ran a minute before dying starts its count again |

A show whose link to the station closes stops: nothing could reach it. It
stops in order if it can, and four seconds later ends itself and its process
group, so a station killed outright leaves no show behind. The station stops
every show on `SIGINT`, `SIGTERM` and `SIGHUP` alike, and a show that has not
stopped ten seconds after it was asked is killed with its process group. In
the test, with three shows, every show was gone 160 ms after the station was
sent `SIGKILL`, and 30 ms after `SIGTERM` or `SIGHUP`.

## The link

Each show opens one TCP connection on loopback to the station: JSON-RPC 2.0,
one object per line. It carries what a show asks of the station and never a
client's call or any media.

| Method | Direction | Params | Answer |
|---|---|---|---|
| `show.hello` | show to station, first line | `{show, addr, secret, pid}` | none; a wrong secret closes the link |
| `governor.admit` | show to station | a claim: `{what, cost, kind, device?, encode?: {slot, shape}}` | `{answer: "granted", ticket, cost, preset}` or `{answer: "refused", need, have, text, short, fits}` |
| `governor.release` | show to station | `{ticket}` | none |
| `show.on_air` | show to station | `{on}` | none |
| `show.load` | show to station, once a second while it holds a ticket | `{millicores}`, its own CPU | none |
| `governor.shed` | station to show, once a second while the machine is over its line and the show holds something to give up | `{steps}`, the station's shed plan cut down to this show's tickets | none |

The secret is new for every start, handed to the show in
`GODWINMIX_STATION_SECRET` and removed from its environment before it starts
anything. A show with tokens accepts it as an admin token (id `station`), so
the station can ask it for its status and add a channel's stream to it; a
show with no tokens stays open, as a single process core does.

The station decides for the whole machine: what is free, and when it is
over its line, what to give up first, in the order the governor always used,
whichever show holds it. A show acts on the steps it is sent as a single
process acted on its own. What a show reports of its own CPU counts as the
station's own work, so a ticket is not counted twice, once as promised and
again as another program's load.

When the link closes the station drops every ticket it held for that show, so
a show that died gives its share of the machine back at once. A show that
cannot get an answer from the station within half a second admits against its own book.

## What a single show setup costs

Measured on macOS, Apple silicon, release build, the first run config, nobody
watching:

| | CPU | Memory |
|---|---|---|
| The station | under 0.1% of one core | 18 to 25 MiB |
| The ingest plugin, once, in the station | 0% | 9 MiB |
| One show (`main`) | 0.7% to 0.9% of one core | 131 to 153 MiB |
| A single process core, for comparison | 0.7% of one core | 135 MiB |
| A second, idle show | 0.7% to 0.9% of one core | 124 to 135 MiB, and 52 MiB more for the camera, screen and audio discovery plugins it starts |

Through the relay a call takes about 25 microseconds longer (`core.status`,
median of 60: 113 direct, 138 through the station) and a call with its event
about 30 (`scene.add` to `event/scene.patch`, median of 20: 392 and 422).
Relaying the mosaic and the preview, ten frames a second each, costs the
station 0.1% of one core. `cargo test -p godwinmix --test station -- --nocapture`
prints the latency on the machine it runs on.

## Two hundred shows

Measured on macOS, Apple silicon, release build, by the station's own test
(`cargo test --release -p godwinmix --test station many -- --nocapture`),
with 200 shows without compositing, one SRT output each, and `main`, no
ingest plugin running:

| | Time |
|---|---|
| `show.add_many` of the 200, applied | 79 ms |
| `show.list` of 201 shows, through the WebSocket, median of 20 | 2.4 ms |
| `show.stats` of 201 shows, the same | 1.2 ms |
