# Show health: alarms, thresholds and thumbnails

Every show has a health: one state and the alarms behind it. A show with
compositing off is watched by the vitals in the direct host
(`plugins/ingest/src/direct/vitals/`); a show that composites watches its own
programme (`crates/godwinmix-core/src/vitals/`). Both report the same shape,
and the station hands it to every client as `event/show.health {id, health}`.
The shapes are in `crates/godwinmix-protocol/src/health.rs`.

## The shape

```json
{"state": "alarm",
 "alarms": [
   {"kind": "black", "since_ms": 1790000000000, "detail": "100% of the picture is black (luma at or under 38)."},
   {"kind": "loss", "since_ms": 1790000003000, "detail": "41 packets lost in 10 s."}
 ]}
```

| `state` | When |
|---|---|
| `ok` | running, nothing wrong |
| `warning` | only `cc-errors` or `loss`: the feed is damaged but still arriving |
| `alarm` | any other alarm |
| `off` | not monitored: the show is stopped or has no input. The station sets this; the measuring side never sends it |

`since_ms` is unix milliseconds when the condition began, not when its
duration ran out: a picture that went black at 12:00:00 with a four second
threshold raises its alarm at 12:00:04 carrying 12:00:00. It stays the same
on every read and every event until the alarm clears; the station keeps the
first start it saw for each kind, so an alarm it adds itself (`no-input` when
nothing has arrived, `output-failed`, `governor-refused`) and a window alarm
whose window slides (`cc-errors`, `loss`) both show their true age. An alarm
that clears and comes back has a new start. Alarms come oldest
first. `detail` is one sentence for a person, and its numbers move without an
event being sent.

An event goes out when the state changes or the set of alarm kinds changes,
never for a number alone. The numbers themselves are read with `show.stats`.

## The alarms

| Kind | Raised when | Who measures it |
|---|---|---|
| `no-input` | nothing is publishing the show's input | direct host |
| `stall` | the input is up but no packet came for `stall_secs`. Hides black, freeze and silence while it holds. For a show that composites, also: its process died or lost its link to the station | direct host; the station for a show process that went |
| `black` | at least `black_ratio` of the picture's pixels have a luma at or under `black_luma`, for `black_secs`. Hides freeze | both |
| `freeze` | the mean luma difference between two looks stays under `freeze_diff` for `freeze_secs` | both |
| `silence` | the sound's peak stays under `silence_db` for `silence_secs`. For a show that composites, only while a source with sound is heard on programme | both |
| `cc-errors` | at least `cc_errors` MPEG-TS continuity errors inside `window_secs` | direct host, from the input's counters |
| `loss` | at least `loss` packets lost inside `window_secs` | direct host, from the input's counters |
| `output-failed` | an output's state is `failed`. One alarm per output, named in `detail` | both |
| `governor-refused` | the governor refused a rendition the show asked for | the station |
| `shed` | the governor stopped a rendition to keep what is on air whole | the station for direct shows, the show itself when it composites |

## Thresholds

Per show. A duration of zero switches that check off, and a check that is
off decodes nothing for itself.

| Field | Default | Meaning |
|---|---|---|
| `black_secs` | 4 | seconds black before `black` |
| `black_luma` | 38 | the 8 bit luma at or under which a pixel counts as black: ten percent of the way from video black (16) to white (235), as ffmpeg's blackdetect has it |
| `black_ratio` | 0.98 | the share of pixels that must be black |
| `freeze_secs` | 10 | seconds unchanged before `freeze` |
| `freeze_diff` | 0.002 | the mean luma difference, 0 to 1, under which two looks count as the same picture |
| `silence_secs` | 10 | seconds quiet before `silence` |
| `silence_db` | -60 | the peak, in dBFS, under which the sound counts as quiet |
| `stall_secs` | 3 | seconds without a packet before `stall` |
| `cc_errors` | 5 | continuity errors inside the window before `cc-errors`; 0 is off |
| `loss` | 20 | lost packets inside the window before `loss`; 0 is off |
| `window_secs` | 10 | the window the two counters are judged over |

`freeze_diff` was set from measurement: a still test card through x264 and
back measured between 0 and 0.0001 from one keyframe to the next, and moving
bars between 0.24 and 0.30.

For a direct show the thresholds travel in the table row's
`monitor.thresholds` (`docs/reference/direct-shows.md`). A show that
composites takes them, beside `alarms`, from two methods of its own, which
the station calls with the settings it keeps for the show:

| Method | REST | What it does |
|---|---|---|
| `vitals.get {}` | `GET /api/v1/vitals` | `{health, settings}`: the health last judged (null in the first second) and the settings in force |
| `vitals.set {alarms?, black_secs?, ...}` | `POST /api/v1/vitals/set` | new settings, flat as in the table above; a field left out takes its default. Applies within a second, nothing restarted. Answers as `vitals.get` |

`alarms` there switches the black, freeze and silence checks on, as a direct
show's `monitor.alarms` does, and keeps a mosaic up for black and freeze
while nobody looks. It is off unless set. The same fields can start in the
show's config as `[vitals]` (`docs/reference/configuration.md`).

A person sets them from the wall with `show.set {id, alarms: {enabled,
black_ms, freeze_ms, silence_ms, silence_dbfs}}` (`docs/reference/shows.md`).
The station keeps that object with the show and calls `vitals.set` with it
each time the show's process says hello on the link, so a show started again
gets the same settings, and again whenever `show.set` changes them while it
runs. The station translates on the way: `enabled` becomes `alarms` (off when
left out), each `*_ms` becomes the matching `*_secs`, and `silence_dbfs`
becomes `silence_db`. A field the wall never set is left out, so the vitals'
default holds for it. A show with no `alarms` set is not called at all and
keeps its own `[vitals]`.

## How a direct show is measured

The vitals read the show's input from the hub like any other reader, and
nothing they do can slow it: a reader that falls behind loses GOPs in its
own queue.

* Only keyframes are decoded, at most one a second, and every
  other frame is dropped before anything is copied. A copy only show's black
  and freeze checks therefore look at one picture per keyframe: they are good
  to about a second, or to the GOP when that is longer than a second, and a
  freeze shorter than that is not seen. A show whose outputs already decode
  the input for a rendition hands one decoded picture a second from that
  decode, and then none of its keyframes are decoded a second time.
* Sound is three frames in a row, once a second, about 64 ms out of every
  1000 for AAC and 72 ms for MPEG layer II. The first only primes the
  decoder; the peak is read from the other two. AAC, MPEG audio (layers I to
  III, MP3 included), AC-3 and E-AC-3 are measured, each decoded as its own
  frames say it is. A sound codec this machine has no decoder for is not
  measured, and its show never raises `silence`.
* A few worker threads (two by default) shared by every
  show, each with one decoder per codec, used one keyframe at a time and
  flushed between shows. Each show has one waiting picture and one waiting
  burst of sound at most, and a newer one replaces an older one that was not
  reached yet, so when there are more keyframes than the workers can decode
  every show is still served, each a little less often. The machine's
  hardware decoder is used when it has one (VideoToolbox on a Mac, VA on
  Linux, NVDEC, Direct3D 11) and libav or dav1d otherwise; a hardware decoder
  that gives nothing back three times in a row is set aside for a minute.
* The keyframe decode runs while the row's `monitor.alarms` is on
  (the default for a direct show) with a picture threshold above zero, or
  while somebody is looking. The sound decode runs only while `alarms` is on
  and `silence_secs` is above zero. With both off and nobody looking nothing
  is decoded at all; a test holds that to zero jobs.

## How a show that composites is measured

* With `[vitals] alarms` off, which is the default, black, freeze and
  silence are not judged at all, as for a direct show with `monitor.alarms`
  off. Failed and shed outputs are judged either way.
* Sound comes from the programme meter, which posts its peaks ten times a
  second whether anyone listens or not. It costs nothing new. Silence is
  judged only while the mixer hears a live source with sound on programme,
  which it decides twice a second as it sets each source's volume. The
  slate, or a scene whose sources carry no sound, has nothing to fall quiet
  and raises no `silence`. A muted source on programme still counts.
* Pictures come from the multiview's programme cell: the snapshot tracker
  already decodes the mosaic and scores motion per cell, so black is one more
  JPEG decode a second and freeze is the tracker's motion score. The mosaic
  exists only while somebody is looking, and with the alarms on the show
  keeps one up for them.
* Outputs come from the status once a second: `failed` and `shed`.

The show sends `event/health {health}` to its own clients, and the same
health to its station on the link (`show.health {health}`, a line beside
`show.on_air` and `show.load`), once when it links and then whenever the
state or the set of alarm kinds changes. The station keeps the last one per
show and adds the alarms of the show's input when it has one (the direct host
reads that input, so `no-input`, `stall`, `cc-errors` and `loss` come from
there). It sends the result on as `event/show.health {id, health}` and serves
it in `show.list` and `show.stats`.

What the show said stops counting the moment its link closes. A show whose
process died, was killed or lost its link reads as `alarm` with one `stall`
alarm, `since_ms` the moment the link closed, until a new process has linked
and sent the first health it judged, about a second after it starts. A show
the station gave up on (`failed`) reads the same way. A show a person
stopped reads `off`, and so does one that was started on purpose and has not
linked yet.

## Thumbnails

The wall asks `GET /api/v1/shows/{id}/thumbnail.jpg?width=160` for the rows on
screen. For a direct show the station calls the direct host:

| Call | Answer |
|---|---|
| `direct.thumbnail {show, width?}` | `{jpeg, width, height, at_ms}`, the JPEG in base64; `{pending: true}` while the first picture is on its way; `{status: 404, why}` for a show the host does not run |

A request keeps pictures coming for that show for ten seconds, at the width
it asked for (16 to 640, made even; a request that names none keeps the last
width asked, 320 until one is), from the next keyframe on. A show nobody
has asked about for ten seconds has its JPEG thrown away, and if its alarms are off its keyframes stop being decoded. The
JPEG is made from the 320 pixel picture the checks already decoded, so a
thumbnail costs one small JPEG encode a second and no decode of its own.

For a show that composites the station calls the show's own
`program.thumbnail {width?}`, which answers in the same shape: `{jpeg, width,
height, at_ms}`, or `{pending: true}` while the first picture is on its way.
A request keeps pictures coming for ten seconds, as the host's does. The
picture comes from a branch on the show's raw programme tee, behind a leaky
queue of its own: the rate is cut to one frame a second before anything is
scaled, the frame is scaled to 320 wide and kept, and the JPEG is made at the
asked width (16 to 320) when somebody asks. No mosaic is built for it, and
with nobody asking for ten seconds the branch is taken off the tee and its
frame thrown away. `gmx_stream_clients{kind="thumbnail"}` on the show's
`/metrics` is 1 while the branch runs.

## What it costs

Measured on an Apple M4 Pro with `cargo test -p gmx-ingest --release
direct::vitals::bench -- --ignored --nocapture`: ten seconds of 1080p30
H.264 at 6 Mbit/s with a keyframe a second (a moving zone plate, fine detail
everywhere, which is the expensive case for a decoder) and AAC, replayed in
real time into N hub publications. CPU is in percent of one core, measured
with `ps` over 20 seconds, with the replay alone taken off.

| Case | Hardware decode | CPU decode |
|---|---|---|
| One 1080p show, keyframe thumbnail plus black, freeze and silence | 0.5%, 14 MB | 2.0%, 41 MB |
| One 1080p keyframe, wall time | 6.0 ms | 23 ms |
| 200 shows, sound only (silence) | 1.1% in all | 1.1% in all |
| 200 shows, alarms on, nobody looking | 87 to 90%, about 30 MB | 193% on two workers, each show a picture about every 2 s |
| 200 shows, alarms on, a wall of 40 asking every second | 87%, no more than without the wall | |

The memory is the workers' decoders and is the same for 1 show or 200; a
show adds its last picture's luma (14 KB) and, while somebody looks, its
JPEG. On a machine with no hardware decoder two workers cap the cost at two
cores, and with many detailed 1080p feeds the checks slow down rather than
take more: a black or freeze alarm then takes a second or two longer to
come.

A 720p show that composites, three sources, nobody looking: keeping the
mosaic up for the picture alarms costs 3.9% of one core (9.2% without, 13.1%
with), measured by `cargo test -p godwinmix-core --release --lib
vitals::tests::cost -- --ignored --nocapture`.
