# Debug a show

A camera went to slate at 20:14 and the stream is still running. This is the
order to work in. Every command here is read only except `log.set` and
`log.gst`, and neither of those can stop the programme.

If you are setting a machine up rather than fixing one, start at `gmx doctor`
and stop after it.

## 1. `gmx doctor`: is the machine capable of this at all

```
gmx doctor
```

One line per check, verdict in the left hand column, and a non zero exit code
when something the default pipeline actually needs is missing. On a clean
machine it names every absent GStreamer element and which package it comes
from, in the words of the platform you are standing in front of.

```
ok    gstreamer         1.28.0
ok    elements          all 23 elements the default pipeline needs are here
warn  video encoder     x264enc only, which is software. Expect a core per 1080p30 encode
ok    audio encoder     avenc_aac will be chosen; also here: fdkaacenc
ok    config            godwinmix.toml parses: 3 sources, 1 outputs, canvas 1920x1080@30
ok    control port      0.0.0.0:8080 is free
ok    runtime directory /etc/godwinmix/.godwinmix is writable
ok    disk space        41.2 GB free under /etc/godwinmix/.godwinmix
ok    machine class     8 cores, 16.0 GB: the gallery should default to 'live' (enough of both to decode a tile per source)

8 checks passed, 1 worth reading
```

`--json` gives the same thing for a script. `--config` points it at a config
somewhere else.

A failing check is worth reading twice before doing anything else. Most "the
stream will not start" reports are one of these lines.

## 2. `gmx logs`: what was the mixer saying at the time

```
gmx logs --instance cam1 --since 20:13
```

Filters across the core log and every plugin log. `--level debug` narrows by
level, `--trace <id>` by one command, `-f` follows, `-n` sets how much history
to print first. `--since` takes either a full timestamp or a time of day, both
UTC, because that is what the files are written in.

Log files live under the runtime directory, which is `.godwinmix` beside your
config unless `GODWINMIX_RUNTIME_DIR` says otherwise:

```
.godwinmix/godwinmix.log          the complete timeline, 50 MB, 5 generations
.godwinmix/plugins/cam1.log       cam1's lines on their own
.godwinmix/session.jsonl          every command, event and decision, append only
```

On a terminal the mixer's own output is human readable. Anywhere else, a
service unit or a container, it is JSON lines, one object per line, so
`journalctl`, Loki or `jq` read it without a parser. `--log-format json` or
`--log-format human` overrides the guess.

## 3. `log.set`: turn the interesting log on, without a restart

The reason the log you want is never on is that turning it on has always meant
restarting, and restarting loses the fault. It does not here.

```
curl -X POST localhost:8080/api/v1/log/set \
  -H 'content-type: application/json' \
  -d '{"instance": "cam1", "level": "debug"}'
```

That raises cam1 alone. Every other source stays where it was, which matters
when the reason you are debugging is that the box is already busy. A module
works the same way:

```
curl -X POST localhost:8080/api/v1/log/set \
  -d '{"target": "godwinmix_core::mixer", "level": "trace"}'
```

A target matches as a module path prefix and the longest match wins, so
`godwinmix_core::mixer` also reaches everything under it and a more specific
override still beats it.

The prefix is the Rust module path, so it names the crate the code is in:
`godwinmix_core` for the mixing (the mixer, sources, outputs, plugins, scenes),
`godwinmix` for the control plane, the CLI and the MCP server, and
`godwinmix_protocol` for the contract. A bare `godwinmix` still catches all
three, because it is a prefix of the other two.

Put it back with `{"level": "default"}` on the same instance or target. Ask
what is in force with `GET /api/v1/log/levels`.

`RUST_LOG` still sets the starting point at launch. After that these calls win.

### GStreamer's own debug

When the fault is inside an element rather than inside the mixer:

```
curl -X POST localhost:8080/api/v1/log/gst \
  -d '{"categories": "rtmp2src:6,rtpjitterbuffer:5", "duration_secs": 60}'
```

`categories` is the `GST_DEBUG` spelling you already know. It goes back down on
its own after `duration_secs`, because the one certainty about a debug firehose
is that whoever turned it on will forget. A minute is the default and it is
usually enough: reproduce the fault inside the window.

GStreamer's debug system is per process rather than per pipeline, so this
raises the category for every pipeline in the mixer. The `instance` field is
accepted and recorded, and it will narrow once sources run as sidecars.

## 4. `gmx trace <id>`: one command's story

Every control call gets a trace id. It travels into every log line the call
produces, and into the session log entry for the command itself.

```
gmx trace 4bf92f3577b34da6a3ce929d0e0e4736
```

That prints everything carrying that id, from the core log, from every plugin
log and from the session log, in time order, with the duplicates dropped. It is
how you follow a take from the request that asked for it to the frame it landed
on.

If your client already speaks OpenTelemetry, send a W3C `traceparent` header
and the mixer uses your id instead of minting one. It comes back on the
response either way, so a client that did not send one can still find out what
it was:

```
curl -i localhost:8080/api/v1/pipeline/clock | grep -i traceparent
```

## 5. `gmx dot`: which queue filled

```
gmx dot cam1 | dot -Tsvg > cam1.svg
```

The pipeline as Graphviz, now, for one pipeline. `programme` and `multiview`
are names too. This is what `GST_DEBUG_DUMP_DOT_DIR` would have given you if you
had thought to set it before the show.

Three more, over HTTP, when the picture is not enough:

```
curl 'localhost:8080/api/v1/pipeline/queues?name=cam1'    # fill per queue, fullest first
curl 'localhost:8080/api/v1/pipeline/latency?name=cam1'   # what each stage declares
curl  localhost:8080/api/v1/pipeline/clock                # base times and running times
```

`queues` sorts by the fullest of the three limits (buffers, bytes, time). The
queue at the top is the one that is blocking. `GET /api/v1/pipeline/list` says
what names exist, and every one of these answers with that list when you get a
name wrong.

## 6. `gmx support-bundle`: keep it

```
gmx support-bundle
```

One zip, ready to attach to an issue:

```
versions.txt                 godwinmix, GStreamer, OS, when it was taken
doctor.txt                   every check, as above
config.redacted.toml         your config with every secret replaced
levels.json                  which log levels were in force
logs/godwinmix.log           the last 10 MB
logs/cam1.log                the same per plugin
session-last-hour.jsonl      every command, event and decision
dot/programme.dot            every pipeline's graph
latency/cam1.json            and its latency
queues/cam1.json             and its queue fill
metrics.txt                  a /metrics snapshot
status.json                  what the mixer thought it was doing
```

Everything that authorises anybody is taken out: values under a key that says
what it is (`token`, `secret`, `password`, `key`, `auth`), and stream keys
sitting unmarked at the end of a URL. `rtmp://a.rtmp.youtube.com/live2/abcd-efgh`
becomes `rtmp://a.rtmp.youtube.com/live2/REDACTED`, so a reader can still see
which service you were publishing to. A config that does not parse is left out
entirely rather than shipped unredacted.

Read it before you attach it to anything public. It is your machine, and the
redaction is a list of what we know about.

It works with the mixer stopped, which is the state you are usually in when you
want one. In that case the pipeline graphs and the metrics are missing, because
those only exist while something is running, and everything else is there.

`--url` and `--token` point it at a mixer on another machine. `--out` names the
file.

## 7. Graphs, when this is not the first time

`/metrics` is Prometheus text format, unauthenticated by default because that
is how a Prometheus server scrapes. See `docs/reference/metrics.md` for the
list. The one to put on a dashboard first is
`gmx_programme_frame_interval_ms`: if the programme is dropping frames, that
histogram shows it before anybody watching does.

## 8. `--startup-report`: this box takes too long to come up

```
godwinmix --startup-report
```

Per stage and per plugin, with anything over 250 ms named. Useful when a box
that used to start in two seconds now takes twenty and nobody knows which
source is responsible.

```
startup: 1840 ms in total
      12.4 ms  gstreamer init  stage
     108.9 ms  config          stage
     412.1 ms  mixer build     stage  SLOW, over 250 ms
     903.7 ms  cam1            source SLOW, over 250 ms
      44.2 ms  cam2            source
over 250 ms: mixer build, cam1
```

The same report is `GET /api/v1/core/startup_report` on a running mixer, so you
can ask after the fact.

## Reading and replaying the session log

The session log is not just a file to grep. Three commands work on it:

```sh
gmx session show .godwinmix/session.jsonl --from 20:13 --to 20:16
gmx session replay bundle/session.jsonl --against test-core --source-fixture bundle/cam1.mkv
gmx session diff before.jsonl after.jsonl
```

`show` is a timeline in sentences. `replay` re-issues the recorded commands at
their recorded timing against a core built in this process, with the sources
replaced by deterministic doubles, and compares what changed. That is what
turns a fault from a church hall on a Sunday into a file that fails on a laptop
on Monday.

[Turn a bug into a test](turn-a-bug-into-a-test.md) is the whole loop, and
[the session log reference](../reference/session-log.md) is the file format.

## What is not here yet

`gmx stats` and `gmx events` are Phase 2.
