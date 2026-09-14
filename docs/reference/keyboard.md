# The keyboard

Every key the terminal UI takes, and the call it makes. `gmx-tui` shows the
same list on `?`; both come from the table in
`crates/godwinmix-tui/src/keys.rs`, so they cannot disagree with each other.

The web UI has its own map, which is changeable and saved per browser. It is
not this table: see [The web UI's keys](#the-web-uis-keys) below.

## Taking

| Key | What it does | Method |
|---|---|---|
| `1` to `9` | take the nth source on screen | `program.take {source}` |
| `0` | cut to black, which is the slate | `program.take {source: null}` |
| `Enter` | take the selected source, for when there are more than nine | `program.take {source}` |
| `r` | revert to the shot before this one | `program.revert {}` |

The number keys count the rows you can see. With a filter in force, `1` is the
first row under that filter. This is the same rule the web UI's tray follows:
a number means a slot on screen, never an index into the mixer's own list.

`r` needs `program.revert`, which is in api_level 1. A core that answers
`-32601 no such method` gets asked once: after that the key says so on the
footer and sends nothing.

## Audio

| Key | What it does | Method |
|---|---|---|
| `m` | mute or unmute the selected source | `source.audio.set {id, muted}` |
| `+` (or `=`) | the selected source's fader, one decibel up | `source.audio.set {id, gain}` |
| `-` (or `_`) | one decibel down | `source.audio.set {id, gain}` |

The protocol's fader is linear, 0.0 silent through 1.0 unity to a ceiling of
10.0, and the screen shows it in decibels because that is how an operator
thinks about it. A step is one decibel: unity is `+0 dB`, one press of `-`
sends `gain: 0.891`. The floor is -60 dB, and a step down from there is
silence.

Mute is held apart from the fader by the core, so unmuting comes back to the
level that was set.

## Ad breaks

| Key | What it does | Method |
|---|---|---|
| `a` (with no break on air) | ask for the clip, then roll it | `adbreak.start {uri}` |
| `a` (with one armed or on air) | end it, and rejoin | `adbreak.end {}` |
| `Enter` | send the clip that was typed | |
| `Escape` | give up on the prompt | |

The clip is a file path or a URI, the same thing `gmx ctl ad` takes.

## Destinations

| Key | What it does | Method |
|---|---|---|
| `o` | move to the destinations pane | |
| `s` | start or stop the selected destination | `output.reconnect {id}` |

There is no `output.start` or `output.stop` in api_level 1: a destination
exists or it does not. `s` therefore drops the connection and makes it again,
which is what an operator pressing it on a stuck destination wants. Adding and
removing destinations is `output.add` and `output.remove`, neither of which
this UI sends.

## Moving about

| Key | What it does |
|---|---|
| `Tab` | move focus: sources, destinations, alerts, and round again |
| `Shift+Tab` | the other way |
| `up` and `down` (or `k` and `j`) | move the selection in the focused pane |
| `Home` and `End` | first and last row |
| `/` | filter the lists by id or name. `Enter` keeps it, `Escape` clears it |
| `?` | the key sheet |
| `Escape` | close the sheet, or clear the filter |
| `q` | quit |
| `Ctrl+C` | quit |

Escape does not quit. A mixer that goes off the screen because somebody leant
on Escape is a mixer nobody trusts.

## While typing

`/` and `a` both open a one line prompt on the footer. While it is open the
letter keys type rather than command, `Backspace` rubs out, `Enter` accepts and
`Escape` gives up. Nothing is sent to the mixer until `Enter`.

## The web UI's keys

The web UI keeps its own map in `ui/shell/keymap.js`, changeable per browser
and shown in that UI under `?`. The overlap is deliberate: `1` to `9` take a
slot and `0` is black in both, so an operator moving between a browser and a
terminal does not have to learn a second set. Everything else differs, because
the web UI has a pointer, a tray and a scene composer and this has a keyboard.

| Chord | The web UI |
|---|---|
| `Ctrl+K` | the command palette |
| `Ctrl+N` | add an input |
| `Ctrl+F` | filter the tray |
| `F2` | rename a tile |
| `Delete` | remove the selection |
| `Ctrl+Z`, `Ctrl+Shift+Z` | undo and redo |

That table is a summary. `ui/shell/keymap.js` is the map itself, and it wins
where the two disagree.
