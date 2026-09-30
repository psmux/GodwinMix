# Wave 3: shows, sharing, routing, and every format

The design is dev/plans/shows-and-renditions.md (read "Architecture",
"Decisions" and "Phases" 5). This file is the shapes the agents of wave 3
build against at once. Change it first, in its own commit, if a shape moves.

## Who owns what

| Agent | Owns | Delivers |
|---|---|---|
| station | crates/godwinmix (lib.rs startup, a new `station/` module, control routing), crates/godwinmix-core only where a show needs a seam | several shows on one machine, each its own process, one control port, one page |
| busfeed | crates/godwinmix-framebus, the plugin host (crates/godwinmix-host, crates/godwinmix-core/src/plugin), plugins/camera, plugins/screen, plugins/audio-device | a device or stream opened and decoded once, read by every show that uses it |
| showui | ui/ (a show switcher, a routing view) | shows as tabs, new, rename, remove, on air state; a routing view of every input to every output with its format |
| coverage | new or existing plugins, crates/godwinmix-core output kinds | the formats and transports still missing, so almost anything comes in and goes out |

## Shows over the protocol

The process a person starts is the station. It owns the control port, the
page, the ingest hub (the ingest plugin), the governor and the supervisor.
Each show is a child process with its own config file under
`<data dir>/shows/<id>/godwinmix.toml`, started with `--supervised`, bound to
a loopback address or a unix socket the station chooses, never to a port a
person sees.

    show.list {}   -> {shows: [{id, name, state: "starting"|"running"|"stopped"|"failed", on_air: string|null, programme_kbps, cpu_millicores, error?}], current: id}
    show.add {name, from?: "empty"|"<show id to copy>"|{project: <project.export file>}} -> Show
    show.rename {id, name} -> Show
    show.remove {id} -> {removed}            (refused for the last show; stops it first)
    show.start {id}, show.stop {id}          (a stopped show keeps its config)

Routing: every existing method and route addresses one show. A client picks
it by `?show=<id>` on the WebSocket URL and on REST paths, or the `show`
field in `core.subscribe`; with none, the first show (the one migrated from
the config the station was started with). The station relays calls and
events to that show's own socket and answers the station methods (show.*,
channel.*, governor.*, config.* of the station) itself. The page served to a
browser is the station's; it talks to one show at a time and can switch.

Migration: a station started with `--config godwinmix.toml` as today runs
that config as show `main` in place (not copied), so a single show setup is
unchanged, and its data stays where it is.

Governor: shows do not calibrate or keep a budget; a show's renditions ask
the station's governor over the show's link to the station
(`governor.admit` from show to station). One machine, one budget.

Events: `event/show.changed {show}` and `event/show.removed {id}` from the
station to every client.

## Routing view (page)

One screen: every input (channel streams, each show's sources) down the
side, every output (channel destinations, each show's outputs) across, the
format in each cell (copy, or the rendition and where it is encoded), from
`channel.list`, `output.list` per show and `rendition.plan` per scope. A
person adds an output to an input from a cell. No new backend method unless
one is missing; say which.

## Performance to report

* A second, idle show costs how much CPU and memory.
* Two shows using the same camera: the camera is opened once (busfeed),
  measured CPU against each show opening it.
* A show killed with SIGKILL: the other show drops no frame, the station
  restarts the dead one, the page says so.
