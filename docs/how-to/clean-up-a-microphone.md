# Clean up a microphone with sound filters

Take the rumble, the room and the hiss out of a microphone, even out a voice
that comes and goes, and keep the whole show under a ceiling. Every filter
here runs inside the mixer, on the CPU, with no plugin to install, and any
of them can be added, changed or removed while you are live.

What there is: a low cut and tone controls (`eq/filter`), a noise gate, an
expander, a compressor, a limiter, a delay to bring sound back in line with a
late picture, noise suppression, and a plain gain. The settings of each are in
[the sound filters reference](../reference/sound-filters.md).

## From the page

1. Open the **Audio** panel. Each source with sound has a row there.
2. Press **FX** on the microphone's row.
3. Press **Voice** for a speaking voice. It adds four filters in this order: a
   low cut at 90 Hz, a noise gate, a compressor and a limiter. Or pick one
   filter at a time from **Add a filter**.
4. Move a slider and hear it at once. **On** turns a filter off without
   losing its settings (it reads **Off** then), and **Remove** takes it out.

Filters on one source run one after the other in the order they were added.
They stay when the mixer restarts.

## Over the API

`filter.add` puts a filter on a source, `filter.set` changes the settings it
names and keeps the rest, and `filter.remove` takes it out.

```sh
gmx ctl rpc filter.add '{"id":"pulpit-eq","type":"eq/filter","source":"pulpit-mic","params":{"low_cut_hz":90}}'
gmx ctl rpc filter.add '{"id":"pulpit-comp","type":"compressor/filter","source":"pulpit-mic"}'
gmx ctl rpc filter.set '{"id":"pulpit-comp","params":{"threshold_db":-24}}'
gmx ctl rpc filter.list
```

`filter.list` answers with each filter's settings as they are now. An agent
does the same with the `add_filter` tool.

## In the config file

```toml
[[filters]]
id = "pulpit-eq"
type = "eq/filter"
attach = { source = "pulpit-mic" }
params = { low_cut_hz = 90, high_gain_db = 2 }

[[filters]]
id = "programme-ceiling"
type = "limiter/filter"
attach = { programme = true }
params = { threshold_db = -1 }
```

A filter on the programme works on everything that goes out, after the mix.
A limiter there stops one loud source from distorting every destination.

## Which filter for which trouble

| You hear | Use |
|---|---|
| Rumble, air conditioning, handling noise | `eq/filter` with `low_cut_hz` from 80 to 120 |
| The room, a fan or traffic between words | `gate/filter`, or `expander/filter` for a softer result |
| Steady hiss, hum or fan noise under the voice | `denoise/filter` |
| A speaker who is loud then quiet | `compressor/filter` |
| A shout or a dropped microphone distorting the stream | `limiter/filter`, on the source or on the programme |
| Lips moving before the words, from a late camera | `delay/filter` on the microphone, `delay_ms` as late as the picture is |
| A source that is simply too quiet or too loud | the fader first, `gain/filter` when it needs more than the fader gives |

## Bringing a scene collection from OBS

An [OBS import](import-from-obs.md) brings each source's sound filters across
with their settings: gain, noise gate, noise suppression, compressor, limiter,
expander and the three band equaliser. They stay on their source, as in OBS,
and are added with it. A filter switched off in OBS comes across switched off.

## What it costs

Measured on an Apple silicon MacBook with 48 kHz stereo, as a share of one
core per filter: gain, EQ, gate, expander, limiter and delay each under 0.05%,
the compressor 0.07%, and noise suppression about 1%. A Raspberry Pi 4 has not
been measured yet; expect several times those figures. The command that
measured them:

```sh
cargo test -p godwinmix-core --release --features denoise --lib cost_of_each -- --ignored --nocapture
```

Noise suppression also adds 10 ms of delay to that source and needs the
canvas at 48000 Hz, which is the default.
