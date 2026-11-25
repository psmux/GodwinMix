# The prompter

Scripts kept on the station, the one on the prompter now, and where every
prompter screen is in it. The methods are `prompter.*`, the events are
`prompter.changed` and `prompter.script.changed`, and the screen is the page
at `/prompter/`. How to use it is in
[Use the teleprompter](../how-to/use-the-teleprompter.md).

## Rows, the unit of every position

A position is counted in rows, and the mixer makes the rows, so every screen
breaks the text in the same places whatever its width. The rule, with `width`
being `line_chars`:

* Each line as written is broken at spaces into rows of at most `width`
  characters. A word longer than that is cut.
* A blank line is one empty row.
* A line that starts with `#` is a marker. Its words, without the `#`
  characters, are its rows, and the marker starts on the first of them. A
  bare `#` is a marker called `marker <n>`, its number counting from 1.
* The asterisks of `*italic*` and `**bold**` do not count towards the width.

A position of 2.5 is half way through the third row. A screen draws the row
the position is in inside its cue band.

The prompter's position now, at the mixer's time `now_ms`, is

```
position + speed * (now_ms - at_ms) / 1000    while playing
position                                      while paused
```

held between 0 and `rows`. The mixer sends nothing while it scrolls: every
screen works this out for itself, every frame.

## The state

`prompter.get` answers it, every move answers it, and `event/prompter.changed`
carries it.

| Field | What it is |
|---|---|
| `script` | The id of the script on the prompter, or null |
| `title` | Its title |
| `updated_ms` | When its text was last saved, in milliseconds since the Unix epoch |
| `rows` | How many rows it makes at `line_chars` |
| `markers` | `[{name, row}]`, in order |
| `position` | Where it was at `at_ms`, in rows |
| `at_ms` | When `position` was true, on the mixer's clock |
| `speed` | Rows a second, -20 to 20. Negative scrolls back |
| `playing` | Whether it moves |
| `line_chars` | The text size, as characters a row: 12 to 80, 32 to start |
| `margin` | Percent of the width left on each side: 0 to 30, 8 to start |
| `mirror` | Flip left to right, for beam splitter glass |
| `cue` | Where the cue line sits, as a fraction of the height from the top: 0.1 to 0.9, 0.33 to start |
| `now_ms` | The mixer's clock as the answer was written |
| `revision` | Moves on every change. A screen drops an answer older than the last state it applied |

A new script starts at a speed of 0.4 rows a second.

```
$ gmx ctl rpc prompter.load '{"id": "evening-news"}'
{
  "at_ms": 1793269306229,
  "cue": 0.33,
  "line_chars": 32,
  "margin": 8.0,
  "markers": [
    {
      "name": "Opening",
      "row": 0
    },
    {
      "name": "Weather",
      "row": 2
    }
  ],
  "mirror": false,
  "now_ms": 1793269306229,
  "playing": false,
  "position": 0.0,
  "revision": 17,
  "rows": 4,
  "script": "evening-news",
  "should_retry": true,
  "speed": 1.8,
  "title": "Evening news",
  "trace_id": "e1c2f147ddf7af3042dc0c9e241b0166",
  "updated_ms": 1793269306213
}
```

## Methods

| Method | Scope | Params | What it does |
|---|---|---|---|
| `prompter.get` | read | none | The state |
| `prompter.rows` | read | none | `{script, updated_ms, line_chars, rows: [{text, line, marker}]}` for the script on the prompter. `line` is the line as written, from 0; `marker` is present and true on a marker's rows |
| `prompter.script.list` | read | none | `{scripts: [{id, title, lines, markers, bytes, updated_ms}], on}`, without the text |
| `prompter.script.get` | read | `{id}` | `{id, title, text, updated_ms}` |
| `prompter.script.save` | operate | `{id?, title, text}` | Make a script or replace the one with that id. With no `id`, one is made from the title: `Evening news` is `evening-news`. Saving the script on the prompter changes the words on every screen and keeps the place |
| `prompter.script.remove` | operate | `{id}` | Forget it. When it is on the prompter, the prompter is left empty and paused at the top |
| `prompter.load` | operate | `{id, play?}` | Put a script on, at the top, paused unless `play` is true |
| `prompter.play` | operate | `{speed?}` | Start scrolling, at `speed` when given. Played from the end, it starts again at the top |
| `prompter.pause` | operate | none | Stop where it is |
| `prompter.speed` | operate | `{speed}` or `{by}` | Set the speed, or add `by` to it. It does not start or stop the prompter |
| `prompter.seek` | operate | one of `{row}`, `{fraction}`, `{marker}`, `{step}`, `{by}` | Move. `marker` is a name (any case) or a number counting from 1 as a string. `step: 1` is the next marker; `step: -1` is the start of this section, or the section before when it is already there (within half a row). `by` is rows on, or back when negative. It keeps playing or stays paused |
| `prompter.set` | operate | any of `{line_chars, margin, mirror, cue}` | Change the look on every screen. Only what is named changes. A new `line_chars` keeps the place: the same line as written, as far through it |

Every params object refuses a field it does not know.

`prompter.speed` and `prompter.seek` are not idempotent, since a retried
`by` or `step` would move twice. A speed change carries on from where the
prompter is at that moment, so something that follows the speaker's voice can
send `prompter.speed` several times a second and the screens follow without a
jump. `prompter.speed` and `prompter.play` are not written to disk, which is
what keeps that cheap.

## Errors

Each error names what was wrong and what to send instead, and `data` says the
same for a program.

| When | Code | `data` |
|---|---|---|
| An id that is not a slug | -32602 | `field: "id"`, `suggest`: a slug made from it |
| A title that is empty | -32602 | `field: "title"` |
| A text over 524288 bytes | -32602 | `field: "text"`, `bytes`, `max_bytes` |
| No script with that id | -32004 | `id`, `scripts`: the ids there are |
| A move with no script on the prompter | -32001 | `state: "empty"`, `scripts` |
| A speed outside -20 to 20 | -32602 | `field: "speed"`, `min`, `max` |
| Both `speed` and `by`, or neither | -32602 | `speed`: the speed it has |
| A seek with none, or more than one, of its fields | -32602 | |
| No marker by that name or number | -32004 | `field: "marker"`, `markers`: their names |
| `line_chars`, `margin` or `cue` out of range | -32602 | `field` |
| A read token asking for a move | -32002 | `held`, `needed` |

```
$ gmx ctl rpc prompter.seek '{"marker": "Traffic"}'
Error: prompter.seek: there is no marker 'Traffic' in this script. Its markers are: Opening, Weather.
{
  "field": "marker",
  "markers": [
    "Opening",
    "Weather"
  ],
  "retryable": false
}
```

## Events

Subscribe with `core.subscribe {events: ["prompter.*"]}`. Neither event is
sent to a client that did not ask for it, and neither is sent while the
prompter scrolls.

| Event | Payload | When |
|---|---|---|
| `prompter.changed` | the state | A load, play, pause, speed change, seek, a new look, a script saved or removed |
| `prompter.script.changed` | `{id, removed, updated_ms}` | A script was saved or removed. The text is not carried |

## On disk

`<stem>.prompter.json` beside the show's config (`godwinmix.prompter.json`
for `godwinmix.toml`), written whole and renamed into place on every change
except `prompter.play` and `prompter.speed`. It holds the scripts, the look,
the script on the prompter and its position. `show.add` copies it with the
rest of the show's files. A file that will not parse is moved aside to
`godwinmix.prompter.json.unreadable` and the show starts with no scripts; the
log says so. A core with no config file keeps its scripts in memory.

## `/prompter/`

```
GET /prompter/
GET /prompter
```

The second answers a redirect to the first. The page opens its own `/rpc`
socket, subscribes to `prompter.*` and nothing else, and reads the token the
way the desk does: from `#token=` in the address, which it moves into this
browser's storage and off the address bar, or from what this browser already
stored. A mixer that wants a token and gets none is told so on the screen.

| What runs | When |
|---|---|
| The frame loop | While the prompter plays, or for a quarter of a second while a small difference is eased out, and only while the page is visible |
| Clock samples | Five `prompter.get` calls when the socket opens, then five a minute. The quickest round trip of each five sets this screen's offset from the mixer's clock |
| `prompter.rows` | When the script, its `updated_ms` or `line_chars` changes |
| Screen wake lock | While connected, where the browser has one |

A new state that moves the target by less than three rows is eased over a
quarter of a second; more than that is drawn at once. The font size is the
largest that fits `line_chars` characters, or the widest row, between the
margins.

This screen's mirror choice is kept in `localStorage` under
`gmx.prompter.mirror`: `desk`, `on` or `off`.

Two screens of 390 and 1920 pixels in headless Chrome on one machine, the
prompter playing at 2 rows a second, were 0.004 rows apart when read at the
same moment.
