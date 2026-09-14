# Why the programme never stops

This is the one rule the whole design is built around, and everything that
looks odd elsewhere follows from it.

## The constraint

An RTMP output has to look like a single unbroken stream. Monotonic timestamps,
no gaps, codec parameters that never change. A viewer's player has a few
seconds of buffer and a decoder configured once from the first keyframe; hand
it a discontinuity and, at best, it stalls and rebuffers, and at worst it drops
the connection and the CDN counts you as having gone offline.

So the output encoder is started once and runs until the broadcast ends. That
is the axiom. Everything that changes during a broadcast has to happen
somewhere the encoder cannot see.

## Where the change happens instead

Upstream, in raw video and raw audio, where switching source is a property
change on a compositor pad.

```
 source 1 ──┐ own pipeline                      program pipeline
 source 2 ──┤ rtmp2src → decode → normalise ──▶ compositor ─┬─▶ encoder ─▶ tee
 source N ──┘ (proxysink/proxysrc boundary)     audiomixer  │
                                                            │
 slate (black) ────────────────────────────────▶ always on  │
 silence ──────────────────────────────────────▶ always on  │
```

Taking a source sets `alpha` on its pad and ramps the volume on the audio
mixer's. The compositor produces its next frame on schedule from whatever its
pads currently say. The encoder receives one more frame, exactly on time, with
the same caps as the last one. It cannot tell that anything happened, so
nothing downstream reconnects.

Adding a source builds a new pipeline and attaches it. Removing one detaches
and tears down. Neither touches the programme pipeline's streaming threads. The
measured cost of adding or removing a source while live is a largest inter
frame interval of 34 ms at 30 fps, which is one frame.

## Three pipelines, not one

A `GstPipeline` is the unit of error propagation in GStreamer. An error posted
by any element travels to its pipeline's bus and tears that pipeline down. So
anything that can fail gets its own.

* **Every source.** A camera that drops, errors, or sends garbage cannot post a
  bus error into the programme pipeline.
* **Every output.** A failing RTMP sink returns a flow error that would
  otherwise travel back through the tee and tear down the programme's streaming
  threads. One dead CDN would take the whole broadcast with it.
* **The multiview.** A preview encoder problem cannot touch programme.

They are joined by `proxysink` and `proxysrc`, which pass buffers between
pipelines without joining their error domains.

## Two things that make the output survive everything dying

**`force-live` on the compositor and the audio mixer.** They emit black and
silence on schedule even with every input dead. Without it, a compositor with
no live pads simply stops producing, and a stopped compositor is a stopped
encoder.

**A slate at the bottom of the z order, permanently.** Losing a source reveals
black rather than freezing on its last frame. It is not a fallback that gets
switched in; it is always there, underneath everything, and it costs nothing.

Both cameras dead, both RTMP servers gone: the output stays live and decodable,
showing black. That is on the verification table in the README, measured by
ffmpeg rather than by the mixer's own code.

## The outage buffer, and why it leaks

`queue_secs` (default 5 seconds) of encoded data sits on the **programme** side
of the proxy boundary, so it survives a reconnect that rebuilds the output
pipeline. A destination that goes away for three seconds comes back and gets
the three seconds it missed.

It leaks downstream rather than blocking. A full queue drops its oldest data
instead of applying backpressure. That is deliberate: a slow destination must
not be able to push back on an encoder that every other destination shares. One
badly behaved CDN would otherwise stall the programme for everybody.

This is the shape of every trade in this codebase. Given a choice between
losing data on one path and stalling the programme, the programme wins.

## What this rules out

* **The canvas cannot change while live.** Resolution, frame rate and sample
  rate are fixed when the encoder starts. Changing them means restarting the
  encoder, which means a new stream.
* **Nothing may block a streaming thread.** Not a plugin, not a UI client, not
  a metrics scrape, not a disk write. Work that could block goes to another
  thread and hands its result over.
* **A crash must cost one source.** This is why plugins are separate processes.
  See [why plugins are processes](why-plugins-are-processes.md).
* **Nothing runs unless asked.** The multiview, the snapshot tracker and the
  telemetry probes all cost CPU on a machine whose spare CPU is what keeps the
  encoder on time. They stop when no client wants them.

## How it is checked

Not by argument. The verification table in the README was produced against two
independent RTMP servers, with sources generated by ffmpeg and the output
measured by ffmpeg, so the measurement shares no code with the mixer. The
numbers that matter: 30 fps with min and max both 30 across every take, zero
disconnects in the server's own record, one reconnect per killed connection,
and the largest gap at any add, remove or take of 34 ms.

Every phase of the plan re runs that table as an acceptance gate. A change that
makes the mixer nicer and the frame interval worse is not an improvement.
