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

Read scope. Measured when asked, never in the background: the running shows
are asked for their status at once (a second at most) and the processes are
read by the plugin host's sampler.

### `show.add {name, from?}`

Admin. Makes a show, starts it and answers with it in `starting`;
`event/show.changed` says when it is `running`.

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
`show.start`.

## Events

Sent by the station to every client, whichever show it is looking at, when
its subscription asks for them (`show.*` or `*`).

| Event | Payload | When |
|---|---|---|
| `event/show.changed` | `{show}`, a row of `show.list` without the measured fields | a show was added, renamed, started, stopped, died, came back or failed |
| `event/show.removed` | `{id}` | a show was removed |

The station's alerts (`event/alert`) say when a show died and when it will be
started again, and when it was left failed.

## Restarts

| How a show's process ended | What the station does |
|---|---|
| status 0 (`core.shutdown`) | leaves it stopped. When it was the only show, the station stops too, as the single process did |
| status 75 (`core.restart`) | starts it again at once, not counted |
| anything else (a crash, `SIGKILL`) | starts it again: three times at once, then after 30 seconds doubling, the plugin host's backoff. The seventh death in a row leaves it `failed`, with an alert. A show that ran a minute before dying starts its count again |

A show whose link to the station closes stops: nothing could reach it.

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
