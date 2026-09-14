# The session corpus

Recorded shows, replayed on every `cargo test`.

Each `<name>.jsonl` is a session log from a real core. Each
`<name>.expect_changes.json` beside it is what that session did to the world:
what went on air, which sources went live, what the ad break did. The test in
`crates/godwinmix/tests/sessions.rs` replays every log against a test core and
fails when a state change differs.

Nothing here needs a network, a camera or a GPU. Every source is either the
deterministic `test/source` double or an address on this machine that nothing
answers on, so a session recorded on a Sunday in a church hall runs on a laptop
on Monday. That is the whole point.

| File | What it records |
|---|---|
| `takes.jsonl` | Two sources come up, three takes, a revert, a cut to the slate. |
| `ad-break.jsonl` | A break is armed, rolls, and rejoins the camera it left. |
| `source-stall.jsonl` | A camera that will not connect, then the rebuild that replaces it. |

## Turning a field bug into a regression test

This is step 5 of the loop in `docs/how-to/turn-a-bug-into-a-test.md`. Six
commands, and none of them needs the thing that broke.

1. Get the log. `gmx support-bundle` carries one, or copy
   `<runtime dir>/session.jsonl` off the machine. `godwinmix --info` prints the
   path.

2. Find the range. `gmx session show session.jsonl --from 20:13 --to 20:16`
   prints a timeline. Cut the file down to the range that matters; a case that
   is three minutes long takes three minutes to replay.

   ```sh
   gmx session show session.jsonl --from 20:13 --to 20:16 > /dev/null   # eyeball it
   awk '/"ts":"2026-09-14T20:1[3-6]/' session.jsonl > tests/sessions/issue-1234.jsonl
   ```

3. Check it replays at all.

   ```sh
   gmx session replay tests/sessions/issue-1234.jsonl --against test-core
   ```

   If the camera that misbehaved is in the bundle as a file, pass it:
   `--source-fixture bundle/cam1.mkv`. Otherwise every source that is not a
   `test://` one becomes colour bars, which is right for most bugs and wrong
   for the ones that are about the media itself.

4. Write down what should happen.

   ```sh
   gmx session replay tests/sessions/issue-1234.jsonl --against test-core \
       --write-expectations > tests/sessions/issue-1234.expect_changes.json
   ```

   Then **read the file and edit it**. What comes out is what the code does
   today, which on a fresh bug is the bug. Change the lines that are wrong to
   what should have happened. Now the test fails, which is the point of step 5.

5. Fix the bug. `cargo test -p godwinmix --test sessions` goes green.

6. Commit the log, the expectations and the fix together.

## The shape of an expectations file

```json
{
  "changes": [
    { "what": "source", "id": "cam1", "to": "live" },
    { "what": "program", "to": "cam1" },
    { "what": "source", "id": "cam1", "to": "stalled" },
    { "what": "program", "to": "cam2" }
  ]
}
```

`what` is one of `program`, `source`, `output`, `adbreak`, `hook` or `alert`.
A bare JSON array works too, which is what most people write by hand.

What is compared, and what is not:

* Timestamps, sequence numbers, running times, trace ids and idempotency keys
  are not compared. They differ on every run and always will.
* What one subject did is compared in order. `cam1` went `connecting` and then
  `live`; a run where it went the other way round is a different run.
* What two subjects did relative to each other is not. Two sources are two
  pipelines on two threads and which one publishes first is a coin toss.
* The programme is compared strictly in order, because the order of what went
  on air is the show.

## Re-recording

`dev/record-sessions.sh` records all three against a real core, the way
`dev/smoke.sh` starts one, and writes the expectations from a replay of what it
just recorded. Run it when the protocol changes in a way that moves the
recorded commands; read the diff before committing it, because a corpus that
was regenerated without anybody looking is a corpus that pins whatever the code
happened to do that afternoon.

```sh
dev/record-sessions.sh              # all three
dev/record-sessions.sh takes        # one
```
