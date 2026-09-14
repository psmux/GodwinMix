# The session log

One append only JSONL file holding every command, every event and every
supervisor decision, in order. It is the file a bug report is, and it is the
file a regression test is made from.

```
<runtime dir>/session.jsonl
```

`godwinmix --info` prints the path. The runtime directory is
`GODWINMIX_RUNTIME_DIR` when it is set, and a `.godwinmix` directory beside the
config otherwise.

## Append only, as a property rather than a promise

There is no method anywhere that truncates, rewrites or deletes a record. The
file is opened with `append(true)`, so every write goes to the end whatever any
handle's position is. No RPC method exposed to any client reaches anything but
`record`. A test holds that line.

METR's incident list includes agents turning logging off. This one cannot be
turned off by anything it is logging.

Rotation is renames only. At 50 MB the current generation becomes
`session.1.jsonl` and a new one starts; five generations are kept beside it,
and a range query reads across them. Nothing already written is edited.

## A record

```json
{"seq": 41, "ts": "2026-09-14T20:13:58.402Z", "kind": "command", "method": "program.take",
 "token_id": "desk", "idempotency_key": null, "trace_id": "1fd332a5e4c29df9",
 "params": {"source": "cam-wide"}}
```

Every record has `seq`, `ts` and `kind`. `seq` is this log's own counter, so
the order in the file is the order things happened even where two subsystems
produce records at once. `ts` is RFC 3339 in UTC to the millisecond, which
sorts lexically, which is how a range query finds its ends without parsing.
`trace_id` ties a record to the log lines and the plugin output from the same
call.

| `kind` | Written when | Fields |
|---|---|---|
| `start` | the log is opened | `runtime_dir`, `version` |
| `command` | a **mutating** call is accepted, before it runs | `method`, `token_id`, `idempotency_key`, `params` |
| `event` | anything is published on the event stream | `event` (the whole event, `type` tagged) |
| `decision` | the supervisor acts | `instance`, `decision`, `why`, `numbers` |
| `gap` | the recorder fell behind | `missed` |

Reads are not commands and are not recorded. A log with a UI's status polls in
it buries the take that went wrong.

Meters and scrubber positions are not recorded either. Three hundred lines a
minute of "the level was -21 dBFS" is not something a replay needs.

A decision carries the numbers that decided it, in the style the supervisor
already uses: not "cam1 rebuilt" but "cam1 rebuilt: no buffers for 10 s, video
was 2,427 ms in the programme's future".

## Reading it

```sh
gmx session show session.jsonl                        # the whole thing as a timeline
gmx session show session.jsonl --from 20:13 --to 20:16
gmx session show session.jsonl --all                  # including what the timeline folds
```

`--from` and `--to` are prefixes of the time of day, so `--from 20:13` works
without typing a whole timestamp.

The timeline is one sentence per thing that happened:

```
20:13:08.679 desk called source.add cam1 test://smpte
20:13:08.697 source cam1 is connecting
20:13:08.735 desk called program.take cam1
20:13:08.735 programme -> cam1 at 231 ms
20:13:08.735 source cam1 is live
20:13:09.788 source hall is failed
20:13:13.808 desk called source.remove hall
```

The whole status document is published whenever the shape of the show changes,
so a source going live is that document twice with one field different. `show`
prints the difference rather than the document, and never prints the same state
twice.

Over the protocol, `core.session_log` returns the recent records, capped at
4 MB, newest kept. `gmx support-bundle` carries the last hour.

## Replaying it

```sh
gmx session replay session.jsonl --against test-core
gmx session replay session.jsonl --against test-core --source-fixture bundle/cam1.mkv
gmx session replay session.jsonl --against test-core --fast
gmx session replay session.jsonl --against test-core --write-expectations > expect_changes.json
```

The recorded commands are re-issued, at the timing they were recorded at,
against a core built in the same process: 1280x720x30, no outputs, no
multiview. The state changes it produces are compared with the ones in the
file. It exits non zero when they differ.

`--against test-core` is the only thing there is to replay against. Replaying
against a live mixer would mean re-taking somebody's programme.

**Sources are replaced.** Anything that is not already deterministic becomes
colour bars, so a session recorded in a church hall runs on a laptop with no
network. Kept as they were: `test://` URIs, `file://` URIs whose file is on
this machine, and loopback addresses (which fail the same way everywhere, which
is often the bug). `--source-fixture` puts one media file in the place of every
source, which is what the file in a support bundle is for.

**Some commands are skipped**, and the report says which and why: `output.*`
(a test core has no outputs, and a replay must not publish to a real CDN),
`plugin.add` and `plugin.remove` (a replay does not install software),
`media.upload` and `media.convert`, and `core.shutdown` (the replay ends by
itself).

**An ad break gets a stand in clip** when the one that rolled on the night is
not on this machine: two seconds, made with GStreamer in a temp directory. The
break still arms, rolls and ends.

## State deltas

What a replay is graded on. Timestamps, sequence numbers, running times, trace
ids and idempotency keys differ on every run and are not compared. What the run
*did* is compared.

| `what` | `id` | `to` |
|---|---|---|
| `program` | | the source id, or `slate` |
| `source` | source id | `connecting`, `live`, `stalled`, `failed`, `removed` |
| `output` | output id | `connecting`, `live`, `retrying`, `failed` |
| `adbreak` | | `armed`, `on air`, `ended` |
| `hook` | hook name | `blocked` |
| `alert` | severity | |

Ordering: what one subject did is compared in order, and what two subjects did
relative to each other is not. `cam1` going `connecting` then `live` is
ordered, and a run where it went the other way round is a different run. `cam1`
going live against `cam2` connecting is two pipelines on two threads, and which
publishes first is a coin toss the machine tosses. A test that failed on that
is a test nobody trusts.

## Comparing two runs

```sh
gmx session diff before.jsonl after.jsonl
```

```
program: expected [cam1, cam2, cam1, slate], got [cam1, __ad__, cam1]
adbreak: expected [], got [armed, on air, ended]
```

Exits non zero when they differ.

## Turning one into a test

`tests/sessions/` is the regression corpus and every file in it runs in
`cargo test`. The loop is in
[`docs/how-to/turn-a-bug-into-a-test.md`](../how-to/turn-a-bug-into-a-test.md)
and the file format is in `tests/sessions/README.md`.

## See also

* [`docs/how-to/debug-a-show.md`](../how-to/debug-a-show.md) for the rest of
  the tools: `gmx events`, `gmx trace`, `gmx logs`, `gmx dot`.
* [`docs/explanation/evals.md`](../explanation/evals.md) for the other thing
  state deltas are used for.
