# Nothing runs unless asked

A mixer sitting in a rack with nobody watching should cost close to nothing.
That sounds obvious and it is unusual. Most media software builds everything it
might need at startup and leaves it running, because that is easier and because
on a developer's laptop the difference does not show.

It shows on a Raspberry Pi. It shows on a five year old laptop with no hardware
encode. It shows on the electricity bill of a rack of cores that are idle
between shows. So GodwinMix builds nothing it has not been asked for, and takes
it away again when the asking stops.

This page says what is on that list, what it costs when it is running, and how
to check the claim rather than believe it.

## The list

| Thing | Built when | Removed when | Idle cost |
|---|---|---|---|
| The multiview mosaic | the first client subscribes | a linger after the last leaves | none: no pipeline exists |
| A source's thumbnail branch | the mosaic is built | the mosaic goes | none: no elements |
| The snapshot and motion tracker | something asks for a snapshot | it goes quiet for `[snapshot] idle_secs` | none: no follower thread |
| An MJPEG stream | a client opens it | the client hangs up | none |
| A PCM or Opus branch | a client opens it | the last client on that shape closes | none |
| A WHEP session | an offer is answered | the session ends | none |
| A local raw preview socket | `preview.open` | the last `preview.close` | none |
| **The programme encoder** | the first consumer takes a lease | the last one gives it up | none |

The last row is the newest and was the largest. Everything above it was already
lazy; the encoder was not.

## What the encoder was doing

The programme encode chain was built in `Mixer::build`, linked to the raw
programme tees, and taken to PLAYING with the rest of the pipeline. A core with
no output attached therefore encoded a black slate, from boot, for nobody.

Measured on an Apple M4 Pro at 1280x720, idle, no sources, no outputs, nobody
subscribed:

| Encode path | `encoder = "always"` | `encoder = "on-demand"` |
|---|---|---|
| Software x264 | 0.110 cores, 70.4 MB | 0.012 cores, 36.8 MB |
| VideoToolbox hardware | 0.035 cores, 132.5 MB | 0.012 cores, 37.4 MB |

On this machine the software encoder was eight times the rest of the idle core
put together. On a Pi 5, where software x264 is the only encoder there is, it is
the largest single cost an idle mixer pays.

Reproduce it yourself:

```bash
gmx bench --only core-idle --encode software
```

The table prints both rows side by side, so the saving is not something you have
to take on trust.

## What stays running, always

This is the part that matters more than the saving.

The **raw programme never stops**. The compositor, the audio mixer, the level
meter and both raw tees are in PLAYING from boot and stay there whatever any of
the above is doing. A source is composited the moment it arrives. A take lands
on the next frame boundary. Nothing in this design puts a pipeline build between
an operator pressing a button and the picture changing.

Only the encode chain comes and goes: the queue, the conversion, the encoder,
its parser and the encoded tee. Those are downstream of the picture, not in it.

Two things follow that had to be got right.

**The first output must not wait a GOP.** Attaching the chain ends with an
upstream force-key-unit event, so the first frame out of a freshly started
encoder is a keyframe. An output that arrives at an idle core gets a decodable
picture immediately, not two seconds of nothing.

**The frame interval must not move.** The raw tees carry `allow-not-linked`, so
a branch joining or leaving is nothing to them; the state changes happen on the
mixer thread and not on a streaming thread; and nothing is ever blocked. The
programme's frame interval across an encoder start stays where it was.

## Why a preview cannot stop the programme

A tee pushes to its pads one after another on one upstream thread. A pad that
blocks holds all of them, including the one carrying the programme.

That is not a hypothetical. A thumbnail branch with a non leaky queue hung off
the same tee as a source's programme branch; a mosaic being torn down stopped
reading it; a second later the queue was full; the tee blocked; the liveness
probe on the programme branch saw nothing; and the supervisor judged a perfectly
healthy source stalled and restarted it. On the programme's own raw tee the same
backpressure reaches the compositor.

So every preview branch starts with a queue that is a second deep and **leaky
downstream**: the thumbnail ends, the programme return, the mosaic's own tile and
encoder queues, and every audio monitoring branch. A preview client that stops
reading loses frames. That is the correct outcome, and it is the only one that
keeps the rule at the top of the README true.

The same reasoning produces the deadline on every socket write. A client that
stops reading is disconnected after five seconds, because the task writing to it
is the task holding the mosaic subscription.

## Checking it

### `/metrics`

```bash
curl -s -H "Authorization: Bearer $TOKEN" http://localhost:8080/metrics | grep -E 'stream_clients|encoder|multiview'
```

On an idle core:

```
gmx_multiview_subscribers 0
gmx_multiview_fps 0
gmx_stream_clients{kind="mjpeg"} 0
gmx_stream_clients{kind="opus"} 0
gmx_stream_clients{kind="pcm"} 0
gmx_stream_clients{kind="preview"} 0
gmx_stream_clients{kind="unixfd"} 0
gmx_stream_clients{kind="whep"} 0
gmx_encoder_running 0
gmx_encoder_consumers{kind="output"} 0
gmx_encoder_consumers{kind="record"} 0
gmx_encoder_consumers{kind="whep"} 0
```

Every kind is written on every scrape, zeros included, so a dashboard built
against a running show does not break when the show ends.

Open a stream and watch a number move:

```bash
curl -sN -H "Authorization: Bearer $TOKEN" http://localhost:8080/mjpeg/sheet > /dev/null &
sleep 2
curl -s -H "Authorization: Bearer $TOKEN" http://localhost:8080/metrics | grep 'kind="mjpeg"'
# gmx_stream_clients{kind="mjpeg"} 1
kill %1
```

### `gmx bench`

```bash
gmx bench --only core-idle --encode software
gmx bench --only multiview
```

`multiview-idle` is asserted rather than sampled: there is no mosaic object to
cost anything, and the row says so. Every other row is a measurement over a
thirty second window after a warm up.

### The pipeline itself

The strongest check does not trust a counter at all. `gmx dot program` prints the
programme pipeline as Graphviz:

```bash
gmx dot program | grep -c venc
```

On an idle core with `on-demand`, the encoder is there as an element but at NULL
and not linked to the tee. The tests assert exactly that, by reading the
element's state rather than a flag.

## Turning it off

```toml
[program]
# "on-demand" is the default. "always" is what every release before this one did.
encoder = "always"
```

Use `always` if you would rather spend the CPU than think about it, or if you
are measuring something and want one fewer variable. Nothing else changes.

The other switches are where they always were: `[multiview] enabled`,
`[multiview] linger_secs`, `[snapshot] enabled` and `[snapshot] idle_secs`.

## The rule underneath

From the project's principles: *nothing runs unless asked. No multiview, meters,
thumbnails, snapshots, telemetry or push streams unless a client wants them.*

It is worth stating as a rule rather than an optimisation because it only holds
if it is applied every time. One subsystem that builds itself at startup because
it was easier is the whole claim gone, and nobody notices until somebody runs
the thing on a Pi.

So the test for a new feature is: with nothing asking for it, is there an object?
If the answer is yes, it is not finished.

## See also

* [see and hear the mixer from anywhere](../how-to/preview-and-audio.md), the
  streams this page counts
* [streams reference](../reference/streams.md), every route and metric
* [why the programme never stops](why-the-programme-never-stops.md), the rule
  this one serves
* [footprint](footprint.md) and [footprint budgets](footprint-budgets.md), the
  numbers and what they are measured against
