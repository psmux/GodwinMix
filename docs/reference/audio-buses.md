# Audio buses: the master, buses, cue and mix minus

The programme mix has a master fader and mute. Beside it you can add buses:
mixes of their own, each with its own master, fed from every source. A bus
can be played on a sound card on the mixer's machine, heard in a browser, or
be the sound of an output. How to use them is in
[mix audio with buses](../how-to/mix-audio-with-buses.md).

## The `[audio]` table

```toml
[audio.master]
gain = 1.0        # 0.0 silent, 1.0 unity, up to 10.0
muted = false

[[audio.bus]]
id = "monitor"            # a slug: lower case letters, digits, single dashes
name = "Stage monitor"    # what the desk shows; the id when left out
gain = 1.0                # the bus master
muted = false
sends = { cam1 = 0.5, mic = 1.0 }
device = "default"        # play it on a sound card here

[[audio.bus]]
id = "guest-return"
minus = "guest-1"         # the programme without this source

[[audio.bus]]
id = "phones"
cue = true                # the sources marked cue; the programme while none is
```

| Key | Default | What it does |
|---|---|---|
| `master.gain` | 1.0 | The programme master, 0.0 to 10.0. |
| `master.muted` | false | Mutes everything that goes out. The meters on `pgm-level` read after it. |
| `bus.id` | required | A slug, at most 48 long. `program` and `programme` are taken. |
| `bus.name` | the id | A label for the desk. |
| `bus.gain`, `bus.muted` | 1.0, false | The bus master. |
| `bus.sends` | none | Level per source id, 0.0 to 10.0. Read only on a bus with neither `minus` nor `cue`. A source with no send is not on the bus. |
| `bus.minus` | none | Makes the bus a mix minus: the programme as heard, without this source. |
| `bus.cue` | false | Makes the bus a cue bus. Cannot be set with `minus`. |
| `bus.device` | none | `default` for the system's choice, or a sound output's id, name or number from `audio.device.list`. |

An output takes its sound from a bus with `audio`:

```toml
[[outputs]]
id = "iso-recording"
uri = "record://programme"
type = "record/output"
audio = "recording-b"
```

Left out, the output has the programme's sound, as it always had. An output
that asks for a rendition cannot take a bus yet; the add is refused and says
so.

The master and the buses are saved in the runtime store
(`godwinmix.runtime.toml`) whenever they change, and that copy is what the
next start reads. The cue marks are not saved.

## The three kinds of bus

Every bus reads each source after its fader and its meter and before its
mute. What the bus does with a source depends on its kind:

| Kind | Set by | A source on it is at |
|---|---|---|
| `sends` | neither key | its send level, and silent while the source is muted |
| `minus` | `minus = "<source>"` | what the programme gives it, so it follows takes, crossfades and audio follows video; the named source is left out |
| `cue` | `cue = true` | 1.0 when marked cue, muted or not, 0 otherwise; with nothing marked, what the programme gives it |

The buses are fed ahead of the programme master, so muting the programme
does not mute a guest's return. A source's programme side filters sit after
its mute and are not on the buses; its input side filters, where sound
filters go by default, are.

A change of level moves over about 80 ms rather than jumping, so a send moved
from 0 to 1 does not click.

## Methods

Every method is on `/rpc` and over REST at the path its name makes
(`audio.bus.add` is `POST /api/v1/audio/bus/add`). The answers are the shapes
in [protocol.md](../../protocol.md).

| Method | Scope | Params | Answers with |
|---|---|---|---|
| `audio.master.set` | operate | `gain`, `muted`, each optional | the master |
| `audio.bus.list` | read | none | the master, every bus, the cue marks |
| `audio.bus.add` | operate | a bus, as in the table above | the bus |
| `audio.bus.set` | operate | `id`, and any of `name`, `gain`, `muted`, `minus`, `device` | the bus |
| `audio.bus.remove` | operate, destructive | `id` | `{"id": ...}` |
| `audio.send.set` | operate | `bus`, `source`, `level` | the bus |
| `audio.cue.set` | operate | `source`, `cue`, and `solo` to clear every other mark first | the master, every bus, the cue marks |
| `audio.device.list` | read | none | `devices`: each sound output's `index`, `id`, `name` and `api` |

In `audio.bus.set`, an empty `device` stops playing the bus on a sound card,
and an empty `minus` makes a mix minus an ordinary bus. A send at 0 takes the
source off the bus.

A bus in the answers carries its config and four more keys: `kind` (`sends`,
`minus` or `cue`), `outputs` (the outputs whose sound it is), `device_state`
(why its sound card is not playing, or that it is being opened) and `listen`
(the `/pcm/` path to hear it).

The status snapshot carries the same as `audio`, and `event/meters` carries
each bus's peaks in `buses`, by id, read after the bus master.

### MCP tools

`set_master_audio`, `list_audio_buses`, `add_audio_bus`, `set_audio_bus`,
`set_audio_send` and `set_audio_cue`, all behind `search_tools`.

## Hearing a bus

`/pcm/bus:<id>` and `/opus/bus:<id>` are the monitoring doors of
[see and hear the mixer](../how-to/preview-and-audio.md) with a bus as the
target. Nothing is built until a client connects, and a bus that is removed
ends its listeners' streams.

A bus with a `device` plays on that sound card. The card's sink runs in a
pipeline of its own beside the programme, fed from a leaky queue, and it does
not wait on the clock: a card that is unplugged or slow to open costs that
bus's sound and nothing else. Its last error is in `device_state`.

## Errors

| Refusal | Code | `data` |
|---|---|---|
| No such bus or source | -32004 | `kind`, `id`, `valid` (the ids there are) |
| A bad id, a level out of range, a bus that is both cue and a mix minus, a send to a bus that has none | -32602 | `field` |
| Removing a bus an output takes its sound from | -32001 | `outputs` |

Each message says what to do instead, for example: `bus monitor is the sound
of guest-out. Point it back at the programme with output.set (audio left out),
or remove it, then remove the bus.`

## What is built

Nothing, until the first bus. Then, in the programme pipeline:

```text
 per source, once for every bus
 pgm-alevel -> pgm-asend (tee) -> pgm-amute -> amix
                   \-> pgm-abq (leaky queue) -> pgm-abtee (tee) -> one pad on each bus

 per bus
 bus-<id>-silence -> bus-<id>-mix -> bus-<id>-caps -> bus-<id>-master -> bus-<id>-level -> bus-<id>-tee
```

The send tee goes in and comes out with the pad above it blocked at an idle
point, the way a filter is inserted live, so adding or removing a bus costs
the programme no sound. The last bus to go takes every send point with it,
and the programme pipeline is what it was before the first. The test
`no_bus_means_the_programme_is_built_as_it_always_was` checks that element
for element.

A bus feeding an output gets an encoder of its own (`bus-<id>-enc-*`, the
programme's choice of encoder) while at least one output reads it.

## Cost

With no bus there is nothing to cost: the programme is built exactly as
before, and the master is a `volume` at unity, which passes its buffers
through untouched.

Two buses (one with four sources sent to it, one mix minus) on a programme of
four `test://` sources with their tones, a 320x180 canvas at 30 frames, cost
0.6, 2.7 and 1.5 percent of one core in three runs on an Apple M4 Pro
(14 cores) on 29 October 2026, a little under 2 percent on average. The
programme alone was 4 to 6 percent. The machine was shared with other work
(load average near 7), which is most of the spread between runs. It has not
been measured on a Raspberry Pi 4 yet, whose cores are several times slower.

Each bus adds one `audiomixer` thread and one silence source; each source adds
one queue thread for all buses together. A bus playing on a sound card adds a
small pipeline of its own, and one feeding an output adds an encoder.

Measure it on your own machine with:

```sh
cargo test -p godwinmix-core --release --test bus_cost -- --ignored --nocapture
```

`GMX_BUS_SOURCES` sets the number of sources and `GMX_BUS_CANVAS` the canvas
(`1280x720`).
