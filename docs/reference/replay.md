# Replay

Instant replay and the replay buffer: rings of encoded picture and sound per
angle, events marked on them, a `replay/source` that plays one back, and a
save to MP4. See [the how to](../how-to/instant-replay.md) for using it.

## `[replay]`

| Key | Type | Default | What it is |
|---|---|---|---|
| `seconds` | integer, 1 to 1800 | 60 | How long each angle keeps |
| `angles` | list of strings | empty | Source ids, and `programme` for the programme. Empty is off |
| `bitrate_kbps` | integer | 0 | The bitrate source angles are encoded at. 0 is the programme's |

The same object is what `replay.start` takes. Started at runtime, it is
written to the runtime store and read back at the next start.

## What is built, and when

Nothing while replay is off. For each source angle, two branches go on the
source's own tees, inside the source's pipeline:

```text
{id}-vtee -> leaky queue (4 frames) -> videoconvert -> encoder -> parser -> ring
{id}-atee -> leaky queue (50 buffers) -> audioconvert -> audiorate -> encoder -> parser -> ring
```

The encoders and parsers are the codec catalogue's choice for the programme
on this machine, with a keyframe every second. The programme angle is two
leaky queues on `venc-tee` and `aenc-tee`, and it holds the programme
encoder on while it records, as an output does. No output encodes a single
source today, so a source angle always has its own encoder.

The queues drop their oldest buffer when full. An encoder that falls behind
loses its own frames, counted in `dropped`, and the source and the
programme lose none. A source that is removed takes its branches with it;
its ring stays, and a source added with the same id records into it again.
`replay.stop` takes the branches out
and frees the rings: every pipeline is then as it was before.

## The ring

Each ring holds whole GOPs, oldest first, and drops the oldest GOP only
while what is left still covers `seconds`. It holds at least `seconds` once
it has run that long, and never more than `seconds` plus one GOP (a second,
on a source angle). Sound older than the oldest picture is dropped with it.
Times are the shared clock's, so the same moment lines up on every angle.

## Methods

| Method | Scope | Params | Answers |
|---|---|---|---|
| `replay.get` | read | none | `ReplayStatus` |
| `replay.start` | operate | `ReplayConfig` | `ReplayStatus` |
| `replay.stop` | operate, destructive | none | `ReplayStatus` |
| `replay.mark` | operate | `point` (`in` or `out`), `seconds`, `name`, `angle` | `ReplayEvent` |
| `replay.event.list` | read | none | `{events}` |
| `replay.event.rename` | operate | `id`, `name` | `ReplayEvent` |
| `replay.event.delete` | operate | `id` | `{id}` |
| `replay.play` | operate | `event`, `seconds`, `angle`, `speed`, `source`, `take`, `transition` | `PlayStatus` |
| `replay.set` | operate | `source`, `angle`, `speed` | `PlayStatus` |
| `replay.save` | operate | `event`, `angle`, `name` | `SaveStatus` |

`replay.mark` with `point: "in"` opens an event at now, or `seconds` before
now; a second in point before an out replaces the open one. `point: "out"`
closes the open event. `seconds` alone makes a whole event of the last that
many seconds. Event ids are `replay-1`, `replay-2` and so on, never reused
while the mixer runs. `in_ms` and `out_ms` are milliseconds of the mixer's
clock, the same clock as `now_ms` in `replay.get`.

`replay.play` plays `event`, or without one the last `seconds` (10 when left
out). The angle is the one named, then the event's own, then `programme`
when it is recorded, then the first. `speed` is 0.1 to 1.0. It loads the
source `source` (default `replay`), adding it with `replay://` when it is
not there, and plays from the start. With `take` it then calls
`program.take` with `transition`.

`replay.set` changes the angle or speed of a replay that is playing, from
the moment on screen, with one flushing seek.

`replay.save` writes `event`, or the whole ring of `angle`, to
`Replays/<name>.mp4` in the media folder. `name` is made from the event or
`buffer`, the angle and the time when left out, and a name that is taken
gets `-2`, `-3`. It answers when the file is closed.

The agent tools, behind `search_tools`, are `get_replay`, `start_replay`,
`mark_replay`, `play_replay`, `set_replay` and `save_replay`.

### Refusals

Every refusal says what to do next, with a `data` object:

| When | Code | `data` |
|---|---|---|
| Replay is off, or an angle holds nothing of that stretch yet | -32001 | `next: "replay.start"` |
| An angle or event that is not there | -32004 | `kind`, `id`, `valid` (the ones there are) |
| `seconds` out of range, `speed` outside 0.1 to 1, no `point` and no `seconds` | -32602 | `field` |
| `replay.set` with no replay source | -32004 | `next: "replay.play"` |
| `source` names a source of another kind | -32001 | `source` |

## `replay/source`

| | |
|---|---|
| Address | `replay://`, or `replay://cam1` for that angle |
| Params | `event`, `angle`, `seconds`, `speed`, `at_end` |
| Capabilities | `seek`, `restart-in-place`, `health` |

It reads the rings and decodes; it encodes nothing. Each frame is the ring's
own buffer with its time counted from the in point and divided by the
speed, so at half speed a frame lasts two canvas frames and the
normaliser's `videorate` shows it twice. Frames from the keyframe before the
in point are decoded with `DECODE_ONLY` and not shown, which puts the first
frame shown on the in point exactly. Sound plays at 1.0 only and is muted
below it. At the end it holds the last frame (`at_end`, as a clip).

Its kind answers `load` (`event`, `seconds`, `angle`, `speed`), `set`
(`angle`, `speed`) and `state`. `load` and `set` hold the feed and answer
with the position to seek to; `replay.play` and `replay.set` then seek
through the mixer, so the aligner is reset first.

## Keys and the panel

The Replay panel (View > Replay, `core/replay` in a preset's layout) loads
when it is first shown. Three keys work from anywhere on the desk while
nothing is being typed, and fetch the panel's code on their first press:

| Key | Command | What it does |
|---|---|---|
| I | `replay.mark-in` | Mark in |
| O | `replay.mark-out` | Mark out |
| P | `replay.play-last` | Play the last event at the speed picked |

The panel reads `replay.get` once a second while it is on screen.

## What it costs

COST_TABLE

## Not yet

Replay has no events of its own on `/ws`; the panel and Companion read
`replay.get` and `replay.event.list`. Playback slower than real time is
silent rather than slowed down. Faster than real time is not offered.
