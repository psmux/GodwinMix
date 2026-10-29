# Sound filters

Nine filter types work on sound: `gain/filter`, `eq/filter`, `gate/filter`,
`expander/filter`, `compressor/filter`, `limiter/filter`, `delay/filter`,
`denoise/filter` and `loudness/filter`. Each goes on one source or on the programme with
`filter.add` or a `[[filters]]` entry (see
[clean up a microphone](../how-to/clean-up-a-microphone.md)). Each runs in
place on the thread that carries the samples, adds no delay unless this page
says so, and reads the setting names OBS writes as well as its own, so an
imported filter keeps its sound.

Every setting has a default, so `params = {}` always works. A value out of
range is refused with the filter, the field and the range in the message, and
the filter keeps the settings it had. `filter.set` changes only the keys it
names.

Every type takes `bypass` (`true` or `false`, default `false`): the filter
stays in place with its settings and lets the sound through untouched.

## Where a sound filter goes

| `attach` | Where |
|---|---|
| `{ source = "mic" }` | on the source's input, before its fader (the default) |
| `{ source = "mic", side = "programme" }` | on the source's programme branch, after its fader and mute |
| `{ programme = true }` | on the programme mix, before every output |

Filters at one place run one after the other, in the order they were added.
A scene item has no sound of its own, so a sound filter on an item does
nothing; it lets the sound through and the log says so once.

## `gain/filter`

| Setting | OBS name | Default | Range | What it does |
|---|---|---|---|---|
| `gain_db` | `db` | 0 | -60 to 30 | the change in level, in dB. A change is ramped over one buffer, so it never clicks |

## `eq/filter`

| Setting | OBS name | Default | Range | What it does |
|---|---|---|---|---|
| `low_cut_hz` | | 0 (off) | 0 to 1000 | a 12 dB per octave high pass at this frequency |
| `low_gain_db` | `low` | 0 | -24 to 24 | a low shelf |
| `low_hz` | | 120 | 20 to 20000 | where the low shelf turns |
| `mid_gain_db` | `mid` | 0 | -24 to 24 | a bell in the middle |
| `mid_hz` | | 1000 | 20 to 20000 | the bell's centre |
| `mid_q` | | 0.7 | 0.1 to 10 | the bell's width; higher is narrower |
| `high_gain_db` | `high` | 0 | -24 to 24 | a high shelf |
| `high_hz` | | 8000 | 20 to 20000 | where the high shelf turns |
| `bands` | | none | up to 10 | bands of your own, after the ones above: `[{ type, hz, gain_db, q }]` with `type` one of `peak`, `low_shelf`, `high_shelf`, `low_cut`, `high_cut` |

A band set to 0 dB is left out, so `{}` changes nothing and costs nothing.

## `gate/filter`

| Setting | OBS name | Default | Range | What it does |
|---|---|---|---|---|
| `open_threshold_db` | `open_threshold` | -26 | -96 to 0 | the gate opens when the level passes this |
| `close_threshold_db` | `close_threshold` | -32 | -96 to 0 | and starts to close under this. It must not be above the open level |
| `attack_ms` | `attack_time` | 25 | 0 to 500 | how long it takes to open |
| `hold_ms` | `hold_time` | 200 | 0 to 2000 | how long it stays open under the close level before it closes |
| `release_ms` | `release_time` | 150 | 0 to 2000 | how long it takes to close |
| `floor_db` | | -96 (silence) | -96 to 0 | how far down it goes when closed |

## `expander/filter`, `compressor/filter` and `limiter/filter`

One gain computer with three curves. The level is the loudest channel of each
sample, so the stereo image never moves.

| Setting | OBS name | Expander | Compressor | Limiter | Range |
|---|---|---|---|---|---|
| `threshold_db` | `threshold` | -40 | -18 | -1 | -60 to 0 |
| `ratio` | `ratio` | 2 | 4 | not used | 1 to 20 (expander), 1 to 32 (compressor) |
| `knee_db` | | 0 | 6 | 0 | 0 to 24 |
| `attack_ms` | `attack_time` | 10 | 6 | instant | 0 to 500 |
| `release_ms` | `release_time` | 50 | 60 | 60 | 1 to 2000 |
| `output_gain_db` | `output_gain` | 0 | 0 | 0 | -32 to 32 |

The expander pushes sound under the threshold further down by the ratio. The
compressor brings sound over the threshold down by the ratio, with a soft knee
of `knee_db`. The limiter's attack is instant and its output is clamped, so
nothing leaves it above `threshold_db`.

## `delay/filter`

| Setting | OBS name | Default | Range | What it does |
|---|---|---|---|---|
| `delay_ms` | | 0 | 0 to 2000 | how much later the sound comes out |

The timestamps are left alone, so the mixer sees no change in timing. A change
of delay on air starts again from silence, which is a short gap. OBS's
`async_delay_filter` is a picture delay and is not read as this one.

## `loudness/filter`

Rides the level towards a loudness target. The loudness is ITU-R BS.1770's,
K-weighted and taken over EBU R128's short-term window of three seconds; a
stereo 1 kHz tone at 0 dBFS reads 0 LUFS, as EBU Tech 3341 says it should. The
gain moves towards the target at no more than `speed_db_per_s`, and holds still
while the sound is under `gate_lufs`, so a pause between words is not lifted.
It follows the average, so put a `limiter/filter` after it to catch the peaks.

| Setting | Default | Range | What it does |
|---|---|---|---|
| `target_lufs` | -14 | -40 to -5 | the loudness to ride towards. YouTube and Spotify play at about -14, a podcast at -16, EBU broadcast at -23 |
| `max_boost_db` | 12 | 0 to 30 | the most it lifts a quiet source |
| `max_cut_db` | 12 | 0 to 30 | the most it takes off a loud one |
| `speed_db_per_s` | 2 | 0.1 to 20 | how fast the gain may move |
| `gate_lufs` | -50 | -70 to -20 | under this the gain holds still |

## `denoise/filter`

| Setting | Default | Range | What it does |
|---|---|---|---|
| `amount` | 1 | 0 to 1 | how much of the cleaned sound is used; 0 is the original |

RNNoise, in the pure Rust `nnnoiseless`. It adds 10 ms of delay to the source
and needs the canvas at 48000 Hz; at another rate `filter.add` is refused
with the setting to change. It is behind the `denoise` cargo feature, which
the `godwinmix` binary turns on by default. On steady white hiss it measured
about 6 dB down; it does better on fans, hum and room noise, which is what it
was trained on.

## Cost

Measured as described in
[clean up a microphone](../how-to/clean-up-a-microphone.md#what-it-costs): under
0.1% of one core for each filter but `denoise/filter`, which is about 1%, on
an Apple silicon MacBook. Not yet measured on a Raspberry Pi.
