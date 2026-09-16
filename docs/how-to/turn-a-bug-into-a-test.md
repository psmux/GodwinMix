# Turn a bug into a test

Somebody says "cam1 went to slate at 20:14 and I don't know why". This page is
what to do with that, end to end. It takes about twenty minutes and the last
step leaves a test behind that will not let it come back.

The discipline is LiveboxMix's own: a regression for every bug found during
testing. What is new is that the regression can be made from a file rather than
from a written description of what somebody thinks happened.

## 1. See it

The report is either a person or an event:

```sh
gmx events --since 20:13 --severity warning
```

## 2. Find it

Four commands, all against the same run, all tagged with the same trace id.

```sh
gmx events --since 20:13 --instance cam1
# stalled: no buffers 10 s, video_behind_ms=-2427

gmx trace 1fd332a5e4c29df9        # the take at 20:13:58 and everything it touched
gmx logs --instance cam1 --since 20:13   # the sidecar's own traceback
gmx dot cam1 | dot -Tsvg > cam1.svg      # the queue that filled
```

And the timeline, which is usually where it becomes obvious:

```sh
gmx session show ~/.godwinmix/session.jsonl --from 20:13 --to 20:16
```

```
20:13:58.402 desk called program.take cam1
20:13:58.410 programme -> cam1 at 58402 ms
20:14:08.112 cam1 rebuild: no buffers for 10 s {"behind_ms":-2427}
20:14:08.140 source cam1 is stalled
20:14:08.140 programme -> slate
```

## 3. Keep it

```sh
gmx support-bundle
```

One zip: the session log, the logs, the config with the secrets taken out, the
pipeline graph, and `gmx doctor`. Attach it to the issue. Everything after this
point is done from the zip, on your own machine, with the church hall closed.

## 4. Reproduce it

```sh
gmx session replay bundle/session.jsonl --against test-core \
    --source-fixture bundle/cam1.mkv
```

The recorded commands are re-issued, with the same gaps between them, against a
core built in this process. Every source that is not deterministic becomes
colour bars, or the fixture you named. The state changes it produces are
compared with the ones in the file, and the differences are printed.

If it reproduces, you have the bug on a laptop.

If it does not, that is information too. The commands were the same and the
outcome was not, so the cause is something the session log does not carry:
the hardware, the network, the version, the wall clock. `gmx doctor` in the
bundle is the next place to look.

## 5. Pin it

Cut the log down to the part that matters, and put it in the corpus.

```sh
awk '/"ts":"2026-09-14T20:1[3-6]/' bundle/session.jsonl \
    > tests/sessions/issue-1234.jsonl

gmx session replay tests/sessions/issue-1234.jsonl --against test-core \
    --write-expectations > tests/sessions/issue-1234.expect_changes.json
```

Now **open that expectations file and edit it**. What came out is what the code
does today, which on a fresh bug is the bug written down. Change the lines that
are wrong to what should have happened:

```json
{
  "changes": [
    { "what": "source", "id": "cam1", "to": "live" },
    { "what": "program", "to": "cam1" },
    { "what": "source", "id": "cam1", "to": "stalled" },
    { "what": "source", "id": "cam1", "to": "live" }
  ]
}
```

The last line is the fix: cam1 comes back rather than the programme going to
slate. Run the corpus and watch it fail.

```sh
cargo test -p godwinmix --test sessions
```

A failing test at this point is the point. If it passes, the case does not
capture the bug and step 5 is not finished.

## 6. Fix it

Make the change. The same command goes green.

## 7. Prove it

The replay says the logic is right. The harness says it is right on a real
pipeline under a real fault:

```sh
gmx harness up
gmx chaos stall cam1 --secs 12
```

Freeze frame, rebuild, recovery, with the programme's frame interval never over
34 ms. That is what the README claims and what `gmx chaos` measures.

The tests that assert that number measure the wall clock, so they need a
machine that can hold it. A shared CI runner cannot, even when the mixer is
right. Such a machine sets `GODWINMIX_TIMING_SLACK` to a multiplier (the hosted
workflows use `3`) and every timing test widens its budget by that much while
still printing what it measured. Leave it unset on your own machine and in
nightly; a budget that is always wide is not a check.

## 8. Commit it together

The log, the expectations and the fix in one commit. Somebody reading `git log`
in a year gets the reproduction with the change.

## What this needs from a bug report

Not a description. A file. "Here is the session log from 20:10 to 20:20"
replaces every chat transcript that ever began "it was working fine and then".

If you are reporting a bug, run `gmx support-bundle` and attach it. If you are
answering one, ask for it first and read the timeline before anything else.

## For a plugin author

The same loop, without needing the core team. Your plugin's logs are tagged
with its instance and level controllable at runtime; the SDK writes your crash
report; `gmx plugin test --offline` replays the transcript in the bundle
against your binary with no GStreamer on your machine at all; and the harness
proves the fix before you publish it.

```sh
gmx plugin test --offline --dir .        # from the transcript in the bundle
gmx plugin test --dir .                  # the eight checks, on a test core
```

## See also

* `tests/sessions/README.md`: the corpus, the file format, and how to
  re-record it.
* [`docs/reference/session-log.md`](../reference/session-log.md): what is in
  the file and what a replay compares.
* [`docs/how-to/debug-a-show.md`](debug-a-show.md): the observability tools in
  full.
* [`docs/how-to/test-a-plugin.md`](test-a-plugin.md): the conformance harness.
