# Reference: the resource governor

`crates/godwinmix-govern`. One budget for the whole machine: CPU, hardware
encoder sessions and load, memory, uplink. Everything that costs one of
those asks the governor before it starts. Why it works this way is in
[resource-governor.md](../explanation/resource-governor.md).

The station starts one with the core and the rendition graph asks it
before every encoder it starts; `governor.status`, `governor.calibrate` and
`event/governor.shed` are its surface on the wire, documented in
[renditions.md](renditions.md). Previews and thumbnails do not ask it yet.

## Features

| Feature | What it adds | Needs |
|---|---|---|
| (default) | admission, tickets, headroom, shedding, the load sampler, the profile, the store | nothing beyond the protocol crate, serde and libc |
| `calibrate` | the timed encodes that measure a machine | GStreamer |

## Admission

```rust
let g = Governor::new(GovernorConfig::default(), profile);
g.start_sampling()?;                       // once a second, on its own thread

match g.admit(cost, "the 720p30 rendition for youtube") {
    Admit::Granted(ticket) => { /* start it; keep the ticket while it runs */ }
    Admit::Refused { need, have, advice } => { /* say advice.text; use advice.fits */ }
}
```

| Call | Returns | Notes |
|---|---|---|
| `admit(cost, what)` | `Admit` | `what` is the words the refusal and any shed alert use |
| `admit_encode(slot, shape, what, kind)` | `Admit` | priced from the profile; a software encoder that does not fit at its configured preset is granted at the slowest preset that does, named by `Ticket::preset()` |
| `admit_claim(claim)` | `Admit` | the general form: `Claim::new(what, cost).kind(Kind::Preview).on("nvidia")` |
| `headroom(device)` | `Cost` | what would be granted now, on the CPU or with `device` |
| `held()` | `Vec<Held>` | every ticket in force |
| `load()` | `Load` | the latest sample; lock free |
| `reserve()` | `u32` | the CPU kept free, thousandths of a core |
| `shed()` | `Vec<ShedStep>` | what to stop or slow, in order; empty unless the machine is over its line |
| `set_profile(profile)` | | after a calibration |

A `Ticket` gives its share back when it is dropped. `Ticket::lower(preset,
cost)` records that a shed step's faster preset has been applied. A ticket
that outlives its governor does nothing when dropped.

`Admit::error_data()` is the `data` object for a protocol error carrying a
refusal: `{need, have, short, fits}`.

### Costs

`Cost` is the protocol's (`godwinmix_protocol::rendition::Cost`). All five
fields are counted.

| Field | Unit | What is left is |
|---|---|---|
| `cpu_millicores` | thousandths of a core | cores × 1000, less the reserve, less other programs at their peak over the last ten seconds, less the larger of this process's measured load and the sum of its tickets |
| `device_millis` | thousandths of one hardware device | 900, less the larger of the device's measured load (where the platform gives it) and the sum of tickets on it |
| `device_sessions` | sessions | the limit calibration found, less the sessions held; `u32::MAX` when no limit was found |
| `memory_mib` | MiB | available memory, less a tenth of the total (at least 512 MiB), and never more than the total less that reserve less every ticket's memory |
| `egress_kbps` | kbit/s | four fifths of `uplink_kbps` less the tickets' egress; `u32::MAX` when the uplink is not known |
| `ingress_kbps` | kbit/s | what arrives on a station: every channel stream and every direct show's input, as last counted. 0 on a core with no station |

### Refusals

`advice.text` is one or two sentences. On an eight core machine with five
cores taken by something else:

```
the 1080p60 HEVC rendition needs 6.0 cores and 3.3 cores is free.
1080p30 H.264 on h264-software-x264 fits, or 1080p60 H.264 on the GPU encoder h264-gpu.
```

`advice.short` names what is short: `cpu`, `device`, `sessions`, `memory`,
`uplink`. `advice.fits` lists, for each encoder calibrated, the largest of
1080p60, 1080p30, 720p60, 720p30 and 480p30 that would be granted now, largest
first, with its cost. A machine not yet calibrated says so at the end of the
text.

## The reserve

Worked out, never typed.

| Machine | Base | Plus |
|---|---|---|
| desktop (`GovernorConfig::desktop`, set by the station when the page runs on the same machine) | a fifth of the cores, at least 1.5 cores | how far load has jumped above its mean in the last ten seconds, up to a quarter of the machine |
| headless | a twelfth of the cores, at least half a core | the same |

The total never passes half the machine. The optional override replaces all
of it.

## Shedding

`shed()` starts returning steps once measured load has eaten half the
reserve, and returns enough to be back under capacity less the reserve.

| Order | What | Action |
|---|---|---|
| 1 | `Kind::Thumbnail` | stop |
| 2 | `Kind::Preview` | stop |
| 3 | `Kind::Rung { index }`, index above 0, the lowest rung first | stop |
| 4 | software encodes not stopped above, lower rungs, then `Kind::Other`, then `Kind::Programme` | the next faster measured preset |

Within one kind the dearest goes first. The programme, rung 0 and
`Kind::Other` are never stopped. Hardware encodes have no preset to lower.
Each `ShedStep` has `ticket`, `what`, `action` (`drop` or `lower-preset` with
`to` and the new `cost`), `frees`, and `why`, the alert text:

```
Stopped multiview (a preview) to keep what is on air: the machine was 0.4 cores over what it can hold.
Moved youtube 1080p to the superfast preset, which looks a little softer, because the machine was 3.1 cores over what it can hold.
```

The governor decides; the caller stops or changes each thing and raises the
alert.

## The profile

`Profile::from_calibration(cal)`. Its methods are the planner's `CostModel`
trait, by name and argument:

| Method | Returns |
|---|---|
| `encoders(codec: VideoCodec)` | `Vec<EncoderSlot>`, hardware first, then cheapest on the CPU |
| `encode_cost(slot: &EncoderSlot, shape: &VideoShape)` | `Cost` at the catalogue's settings |
| `scale_cost(from: &VideoShape, to: &VideoShape)` | `Cost` |
| `decode_cost(shape: &VideoShape)` | `Cost`, software decode |
| `audio_cost(shape: &AudioShape)` | `Cost` |

and beyond it: `encode_cost_at(slot, shape, preset)`, `presets(slot)`
(fastest first), `preset_that_fits(slot, shape, millicores)`,
`faster_preset(slot, current)`, `session_limit(device)`, `devices()`.

Costs between the calibrated shapes follow a straight line in megapixels per
second through the measured points. Software encode CPU is multiplied by
`software_margin` (1.3), for pictures harder than test bars. Hardware encodes
cost one session and a share of the device equal to the wall time a second
of that picture took. Memory is eight frames of 4:2:0 plus 8 MiB, an
estimate. `Profile::uncalibrated()` has no encoders and prices everything
from cautious figures for a four core laptop.

## Calibration

`calibrate(&candidates, &audio, &Options::default()) -> Calibration`, behind
the `calibrate` feature. Blocking; run it off any streaming thread.

A `Candidate` is one catalogue encoder: its `EncoderSlot`, element, parser,
software decoder and a `configure` closure that applies the catalogue's
properties for a shape. The caller builds the list from the codec catalogue
with the core's own presence check and property code;
`godwinmix_core::render::candidates` is that glue, and the station and the
end to end test both use it.

| Step | What runs |
|---|---|
| baseline | the test source alone, at each shape, to take off every run |
| encode | 30 frames of scrolling SMPTE bars at 720p30 and 1080p30 through each candidate |
| presets | for an element with `speed-preset`: `ultrafast`, `superfast`, `veryfast`, `faster` at 1080p30, stopping once one would need the whole machine |
| sessions | per hardware device: 640x360 live sessions opened one at a time until one fails to produce a frame in 1.5 s, or eight are open; then all closed |
| scale | 1080p30 to 720p NV12 |
| decode | encode, parse and decode at 1080p30, less the encode, per codec |
| audio | one second of 48 kHz stereo through each audio encoder |

Each run is timed from the first frame into the element under test to the
last frame at the sink, by this process's CPU time and the wall clock. The
optional steps are skipped past `Options::deadline` (8 s) and say so in
`notes`.

On an M4 Pro with 14 cores (GStreamer 1.28.7, a debug build of the test)
the whole calibration took 5.0 s. What it measured, before the software
margin:

| Entry | 720p30 | 1080p30 | Other |
|---|---|---|---|
| `h264-videotoolbox` | 0.05 core, 8% of the engine | 0.07 core, 11% | 8 sessions opened, none refused |
| `h265-videotoolbox` | 0.04 core, 7% | 0.07 core, 12% | |
| `h264-software-x264` (`veryfast`) | 0.17 core | 0.37 core | `ultrafast` 0.22, `superfast` 0.28, `faster` 0.45 at 1080p30 |
| `h265-software` (`ultrafast`) | 0.44 core | 1.04 cores | |
| `av1-software` (preset 10) | 0.27 core | 0.51 core | |

Decode came to about 2 thousandths of a core per megapixel a second for H.264,
HEVC and AV1, scaling to 0.7, AAC to 0.02 core and Opus to 0.005.

## The store

`Store::new(runtime_dir)` keeps `governor/calibration-<fingerprint>.json`
under the mixer's runtime directory. The fingerprint is sixteen hex digits
over the CPU model, core count, memory to the GiB, OS, architecture,
GStreamer version and every candidate's id and element; any of them changing
means a new file.

`decide(&store, fingerprint, on_air, asked)`:

| Situation | Decision |
|---|---|
| asked | `Measure`, even on air |
| a file for this fingerprint | `Use(cal)` |
| none, on air | `Wait { stand_in }`, the newest file of any fingerprint, or none |
| none, off air | `Measure` |

## Config

`[governor]` in the station's config. Both keys are absent by default.

| Key | Type | Meaning |
|---|---|---|
| `reserve_cores` | number | Advanced. Cores to keep free, in place of the worked out reserve |
| `uplink_kbps` | integer | The uplink, when known. Without it nothing is refused for bandwidth |

## Load sampling

| Platform | Machine CPU | This process | Memory available | Hardware encoder load |
|---|---|---|---|---|
| Linux | `/proc/stat` | `getrusage` | `/proc/meminfo` `MemAvailable` | AMD `gpu_busy_percent` in sysfs, as `va`; NVIDIA not read |
| macOS | `host_statistics` | `getrusage` | `host_statistics64`, free plus inactive plus speculative pages | not read |
| Windows | `GetSystemTimes` | `GetProcessTimes` | `GlobalMemoryStatusEx` | not read |

Where a device's load is not read, the governor counts the calibrated share
of each ticket on it. One sample costs about 2.3 µs in a release build on an
M4 Pro, 0.0002% of a core at one a second; an admission and its release
about 140 ns.
