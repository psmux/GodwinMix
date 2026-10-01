# Run several shows on one machine

A show is one programme with its own scenes, sources and outputs. One machine
can run several at once: the Sunday service, a loop that runs all week, a
second room. Each show is a process of its own, so one that crashes is started
again while the others stay on air, and they all sit behind the one address
you already open in the browser. Everything below can be done from the page;
the calls after it do the same from `curl`, a script or an agent.

## From the page

The show tabs are in the top bar, just after the menu. With one show there is
its name and a `+` and nothing else, so a setup with one show looks as it
always did. Once there are two, each show has a tab. A red dot means that show
is on air. A dimmed name with a small mark means it is not running: `○`
stopped, `◌` starting, `!` failed. Hover a tab and it says which, and why a
failed one failed. On a laptop screen the tabs move to a row of their own under
the live controls, so those keep their size; on a phone the row scrolls
sideways.

To add a show, click the `+`, or choose File, New show. Give it a name and pick
how it starts: empty, a copy of the show you are on (its scenes and sources,
not its outputs), or a project file saved with File, Save project as. The page
moves to the new show once it is made. A new empty show opens on the welcome
tiles, the same as a new mixer does.

To switch, click the other show's tab. The page loads again on that show, with
`?show=<id>` in its address, and your panels stay where you put them. File,
Switch show lists every show with its state, which is quicker when there are
many.

Click the tab of the show you are on, or right click any tab, for its menu:
Rename, Start show or Stop show, and Remove show. Double click a tab, or press
`F2` on it, to rename it where it is; `Enter` keeps the name and `Escape` puts
the old one back. Remove asks first. The page will not remove the last show,
or `main`, and says why when you try. If you remove the show you are on, the
page moves to another one.

The tabs are reachable from the keyboard: `Tab` lands on the show you are on,
the arrow keys move along the row, `Enter` switches or opens the menu, and
`Delete` removes. The [keyboard reference](../reference/keyboard.md#the-show-tabs)
has the whole table.

To see every show's outputs next to every channel's, open View, Routing: see
[route inputs to outputs](route-inputs-to-outputs.md).

## Nothing to change for one show

Start GodwinMix as always:

```sh
godwinmix --config godwinmix.toml
```

That config runs as the show called `main`, in place. Its scenes, sources,
outputs and channels are where they were, the page is at the same address,
and every client that worked before still works. The process you started is
now the station, which costs about 20 MiB and no measurable CPU; `main` runs
beside it.

## Add a second show

```sh
curl -X POST localhost:8080/api/v1/shows -H 'content-type: application/json' \
     -d '{"name": "Second room"}'
```

The answer is the show, `starting`. It is `running` a second or two later:

```sh
curl localhost:8080/api/v1/shows
```

```json
{"current": "main", "shows": [
  {"id": "main", "name": "Main", "state": "running", "on_air": null, "cpu_millicores": 7, "memory_mib": 131, "restarts": 0, "programme_kbps": 0},
  {"id": "second-room", "name": "Second room", "state": "running", "on_air": null, "cpu_millicores": 9, "memory_mib": 124, "restarts": 0, "programme_kbps": 0}
]}
```

An idle second show costs under 1% of one core and about 125 MiB, plus the
device discovery plugins it starts (about 50 MiB between them).

To start from a copy of a show rather than an empty one, add
`"from": "main"`. The copy has the scenes, the sources and the settings, and
not the outputs, so it never sends a second stream to your YouTube key.

## Talk to one show

Every call you already know goes to one show. Name it with `?show=`:

```sh
curl 'localhost:8080/api/v1/sources?show=second-room'
curl -X POST 'localhost:8080/api/v1/program/take?show=second-room' \
     -H 'content-type: application/json' -d '{"source": "church"}'
```

On `/rpc`, put it on the URL (`ws://host:8080/rpc?show=second-room`) or in
`core.subscribe {"show": "second-room"}`, which moves the connection to that
show. Without a name, a call goes to `main`.

## Use one channel in two shows

Channels belong to the machine, not to a show: one RTMP port, one set of
keys, one listener. A stream published to a channel becomes a source in
`main` as before. To put the same stream in another show, read its `relay`
from `channel.list` and add it there:

```sh
curl -X POST 'localhost:8080/api/v1/sources?show=second-room' -H 'content-type: application/json' \
     -d '{"id": "church", "uri": "channel:live/main", "type": "ingest/rtmp",
          "relay": "127.0.0.1:1935", "stream": "live/main"}'
```

Both shows read the one stream the station received; the encoder publishes
once.

## Send a feed straight on, with no mixing

A headend channel, a contribution feed or a camera you only pass along needs
no scenes and no programme encode. Make it a show without compositing: one
input sent to its outputs, copied as it arrives unless an output asks for a
rendition. It has no process of its own; the direct host in the ingest
plugin runs all of them, so two hundred cost what two hundred inputs cost
and not two hundred mixers.

```sh
curl -X POST localhost:8080/api/v1/shows -H 'content-type: application/json' -d '{
  "name": "BBC One", "compositing": false,
  "input": {"uri": "udp://@239.1.1.1:5000", "program": 101},
  "outputs": [{"uri": "srt://10.0.0.9:9000"},
              {"platform": "youtube", "key": "xxxx-xxxx", "rendition": {"preset": "youtube-720p30"}}]}'
```

The answer is the show, already `running`. Its `health` says `alarm` with
`no-input` until the feed arrives. The key is sealed in the secret store and
never comes back; `has_key` says it is there. Add, change and remove outputs
with `show.output.add`, `show.output.set` and `show.output.remove`
(`POST /api/v1/shows/bbc-one/output/add`, `POST /api/v1/shows/bbc-one/output`,
`POST /api/v1/shows/bbc-one/output/remove`), each with `"output"` naming the
output. Output addresses may be `srt://`, `rtmp://`, `udp://` (multicast
too), `rtp://` or `rist://`.

To make many at once, send them all to `show.add_many`. It checks every one
first and, unless you send `"dry_run": false`, makes nothing: it answers which
would be made, which would be refused and why, and what their renditions
would cost against what this machine has free. Read that, then send the same
list with `"dry_run": false`. A show with one bad output is refused whole.
The page does this from File, New shows: see
[add shows in bulk](add-shows-in-bulk.md), and watch them all on
[the monitoring wall](monitor-many-shows.md).

`show.stats` reads the health and numbers of every show in one call, cheap
enough for once a second.

## Mix a feed after all, and back

`show.set {"id": "bbc-one", "compositing": true}` turns a feed into a mixed
show: the station gives it a folder and a process, makes the input its one
source, puts it on programme and moves the outputs to it. You can then add a
lower third or a second source as in any show. The answer says which outputs
moved and `gap_ms`, how long they were off. Measured with a 720p30 UDP feed
sent on to an RTMP server, seven switches in all: 0.6 to 0.7 seconds turning
compositing on (the show starting and the RTMP connection being made again)
and 0.5 to 1.2 seconds turning it off.

`"compositing": false` goes back, when the show has that one source, no
scene on programme and only outputs it took over. Otherwise it is refused
with what is in the way, such as the sources to remove first.

## When a show dies

The station starts it again at once and says so: `event/show.changed` with
`restarts` counted and an alert naming what happened. Every other show keeps
going. In a test with two shows recording their programmes, killing one with
`SIGKILL` brought it back in 0.64 seconds while the other recorded 624 frames
in 20.77 seconds with no gap. A show that keeps dying is left `failed` after
the seventh time in a row; `show.start` tries again.

## Stop, rename and remove

```sh
curl -X POST localhost:8080/api/v1/shows/second-room/stop
curl -X POST localhost:8080/api/v1/shows/second-room/rename -H 'content-type: application/json' -d '{"name": "Hall"}'
curl -X DELETE localhost:8080/api/v1/shows/second-room
```

A stopped show keeps its config and stays stopped until `show.start`, even
across a restart. Removing a show deletes its folder. `main` cannot be
removed, because its config is the one the station was started with; stop it
instead.

## One process, no station

`godwinmix --config godwinmix.toml --show main` runs that config in one
process with no station in front of it, exactly as releases before shows
did. Use it to compare, or where a second process is not wanted.

See [the shows reference](../reference/shows.md) for every field and rule.
