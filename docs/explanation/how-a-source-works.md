# How a source works

A source produces pictures and sound for the canvas. Everything else in the
mixer treats them all the same way, which is why a camera, a file, a web page
and a command line can be swapped for each other without the programme output
noticing.

This explains the four pieces that make that true: the `Source` trait, the
`MediaEnds` it hands back, the `ProgrammeBranch` the mixer hands it, and the
supervisor that decides what to do when it stops.

## The shape

```
   its own GstPipeline                      the programme pipeline
 +-------------------------------+        +-----------------------------------+
 |  kind specific ingest         |        |                                   |
 |  (rtmp2src / uridecodebin /   |        |  pgm-vsrc-{id} -> pgm-vq-{id} ----+--> compositor
 |   fdsrc+decodebin / wpesrc)   |        |                                   |
 |          |                    |        |  pgm-asrc-{id} -> pgm-aq-{id}     |
 |          v                    |        |     -> pgm-again-{id}  (fader)    |
 |  videorate -> videoconvert    |        |     -> pgm-alevel-{id} (meter)    |
 |    -> videoscale              |        |     -> pgm-amute-{id}  (mute) ----+--> audiomixer
 |    -> {id}-vcaps  <-- FILTER  |        |                                   |
 |    -> [livesync] -> {id}-vtee |        +-----------------------------------+
 |         |        \            |                      ^
 |         |         \-> thumb branch (optional)        |
 |         v                     |                      |
 |  {id}-vproxy  ================|======================+  proxysink / proxysrc
 |                               |
 |  audioconvert -> audioresample|
 |    -> {id}-acaps  <-- FILTER  |
 |    -> {id}-aproxy  ===========|======================+
 +-------------------------------+
```

Two pipelines, joined by proxy sinks and sources. That join is the isolation
boundary: if a camera's decoder errors, the bus message stays inside the
source's own pipeline and the programme encoder never hears about it.

Everything below `{id}-vcaps` and `{id}-acaps` is identical for every source in
the system. That is what makes a take a property write on a compositor pad
rather than a renegotiation, and it is what lets a filter be dropped in without
asking what is upstream.

## The trait

`crates/godwinmix-core/src/plugin/source.rs`:

```rust
pub trait Source: Send {
    fn manifest(&self) -> &Manifest;
    fn initialize(&mut self, hello: Hello) -> Result<Ready>;
    fn start(&mut self, canvas: &CanvasCaps, thumb: bool) -> Result<MediaEnds>;
    fn stop(&mut self) -> Result<()>;
    fn configure(&mut self, params: &Params) -> Result<Configure>;
    fn health(&self) -> Health;
    fn call(&mut self, method: &str, params: Value) -> Result<Value>;
}
```

`initialize` settles what this instance is before anything is built. It
validates the params and answers with the capabilities that actually apply,
which is not always what the manifest declared: `browser/source` declares
`restart-in-place` only when it is running in our own process, because a page
in the sidecar cannot come back in place.

`start` builds the pipeline in NULL and hands back its ends. `thumb` decides
whether the thumbnail branch is built at all, because the core does no work for
a picture nobody is looking at. A source can gain or lose that branch later
without a rebuild: `InputPipeline::attach_thumb_end` asks the tee for another
pad, which the programme branch never notices.

`call` is everything the core does not model: `restart`, `client.fallback`,
`audio.set`. An unknown method answers with an error naming the ones this source
does take, rather than a bare "unknown method".

## MediaEnds

What `start` returns:

```rust
pub struct MediaEnds {
    pub pipeline: gst::Pipeline,
    pub video: gst::Element,          // proxysink at canvas video caps
    pub audio: gst::Element,          // proxysink at canvas audio caps
    pub thumb: Option<gst::Element>,  // built only when asked for
    pub health: Arc<SourceHealth>,
    pub last_video: Arc<LastBuffer>,
    pub last_audio: Arc<LastBuffer>,
    pub vcaps: gst::Element,          // the per source filter insertion points
    pub acaps: gst::Element,
    pub vtee: gst::Element,
    pub parts: KindParts,             // whatever only this kind has
}
```

The liveness probes on the two proxy sinks are installed by the shared
scaffolding, not by the kind, so no kind can forget them or put them somewhere
that reports a source as healthy while an element downstream silently discards
every buffer.

`KindParts` is the escape hatch, and it is deliberately small: whether this
source composites its own layers, the per layer gains, the placements. The core
stores these and never interprets them.

## The shared normaliser

`crates/godwinmix-core/src/plugin/kinds/normalise.rs` builds everything from
`videorate` down. A kind
builds only what sits above it and says where its dynamic pads should go:

```rust
assemble(ctx, thumb, Ingest::default().with(elements).livesync(false), |w| {
    gst::Element::link(&src, &decode)?;
    w.route(&decode, w.norm.video_entry(), w.norm.audio_entry());
    Ok(KindParts::default())
})
```

`livesync` is the one real difference between the kinds and it is not about the
protocol. A live stream drifts against our clock and has gaps, so it goes
through `livesync`, which duplicates and drops to hand the canvas a steady feed.
A file has neither drift nor gaps and its timestamps start at zero while the
programme is minutes in, so livesync judged every early frame late: an eight
second ad lost its first 1.4 seconds. A process writing to a pipe has the same
problem and paces its own output anyway.

## ProgrammeBranch

`crates/godwinmix-core/src/plugin/branch.rs`. The mixer builds one per source
and hands it over: the
two proxy sources, the two queues, the fader, the meter, the mute, and the two
mixer pads.

The order of the three audio elements is the point. The fader is ahead of the
meter so that pulling it down visibly pulls the meter down with it; a meter that
ignored the fader next to it reads as broken and an operator stops trusting
either. The mute is behind the meter so a muted source still shows its signal,
which is what lets someone confirm a camera has sound before cutting to it.

The audiomixer pad's own `volume` is deliberately not in this chain. Takes fade
that pad, and two things writing one property fight: an operator's fader would
be undone by the next take.

## The supervisor

`Mixer::tick` runs twice a second. It asks each source for its observed state
(is it delivering buffers?), and when one has said nothing for
`stall_timeout_secs` it arms a restart.

Which restart depends on what the source declared, not on what kind it is:

| Capability | What the supervisor does with it |
|---|---|
| `restart-in-place` | NULL the pipeline, ask the kind to bring its process back, start again. Without it the source is built from nothing, with the freeze frame covering the gap and the rebuild backoff. |
| `alpha` | the source composites on the programme's clock already, so the timeline aligner leaves its pads alone |
| `seek` | the source is asked whether it can be scrubbed; a kind that never declares it is not asked twice a second for the rest of the broadcast |
| `health` | the kind's own `health()` is combined with the core's buffer observation |
| `latency-report` | the declared `latency_ms` is trusted |

This is the change that matters most for a plugin author. The supervisor used to
key on a `superimposed()` flag inside the core, which meant only the core could
have a source that needed rebuilding. Now a source says what it can survive and
gets the right policy without the core knowing what it is.

The measured guarantees around it are unchanged and must stay that way: the
freeze frame on rebuild (a retired compositor pad keeps drawing its last
buffer), the backoff of three free attempts then thirty seconds doubling to
three hundred, the counter cleared on the first frame and on removal, the
process group kill, and the profile directory cleanup.

## Bus attribution

Every pipeline gets a watcher, and the watcher is told who it belongs to:

```rust
gstutil::watch_bus(&input.pipeline, BusOwner::Source(id.clone()), bus_tx)
```

An error, a warning or an end of stream arrives carrying that owner. Nothing
strips an `input-` prefix off a name any more, so the attribution of a failure
cannot depend on a naming convention nothing enforces. Level messages are still
matched by element name, but whole, never split: a source id may contain a
hyphen, and `pgm-alevel-cam-1` split on one names `cam`, which may well be a
different source that exists.

## Filters

Four insertion points, all of them between two elements that already speak the
canvas contract:

| Where | Between |
|---|---|
| per source, input side | `{id}-vcaps` and `{id}-vtee` (the thumbnail sees it too) |
| per source, programme side | `pgm-vq-{id}` and the compositor pad |
| programme, every consumer | `vmix-caps` and `vraw-tee` |
| programme, output only | `venc-q` and `venc-conv` |

At build time a filter is linked in like any other element. Live, the src pad
above the point is blocked, the link is moved underneath the block, and the
block is released. That is the same `with_pad_blocked` the outputs use for a
proxy swap, and it is measured: `inserting_and_removing_a_chroma_key_live_costs_no_more_than_one_frame`
watches the inter frame interval across an insert and a removal.
