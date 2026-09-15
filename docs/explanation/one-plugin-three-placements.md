# One plugin, three placements

A plugin author writes one program. The operator decides where it runs by
writing one line of config. Nothing else changes: not the plugin, not the
settings form, not the tools, not the health, not the events, not the alerts.

```toml
place = "sidecar"          # a process on this machine
place = "node:studio-b"    # a process on another machine
```

That is the whole feature. This page is about why it is worth the trouble and
what it cost to make true.

## The thing it is avoiding

OBS can put a remote camera on the canvas. DistroAV and Teleport both do it,
and both deliver a flattened stream: the source appears, and its settings, its
hotkeys and its events do not. If the camera on the other machine needs its
gain changed, somebody walks to the other machine. If it fails, the local log
says a stream stopped, not why.

The reason is not laziness. It is that the remote thing is a different thing:
a network source, with a network source's properties, standing in for a plugin
that lives somewhere the local process cannot reach. Two implementations, and
only one of them knows anything.

## What is done instead

The node runs the core's own tier 2 host. Not a port of it, not a subset: the
same `SidecarSource`, the same handshake, the same lifecycle, the same restart
backoff, the same budgets. A plugin on a node is spoken to over stdin and
stderr by a process on its own machine, and it cannot tell that the machine is
not the core's. It is told `tier = "sidecar"` at the handshake, because from
where it is standing that is the truth.

What is different is one layer up. On the core, `BridgedSource` implements the
same `Source` trait every built in kind implements:

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

`configure` goes over the node's socket instead of down a pipe. `health` asks
the node, which asks the plugin. `call` forwards `seek`, `position`,
`keyframe`, `audio.set` and `tool.call` untouched. `manifest` returns the
remote plugin's manifest, which arrived in the node's hello, so
`plugin.describe` renders its settings form and `agent.state` lists its tools
exactly as it would for a plugin installed here.

The mixer never learns which it is talking to. That is the test: nothing
downstream of the trait has a branch on placement.

## Where the fork actually is

One place, and it is four lines:

```rust
// crates/godwinmix-core/src/plugin/host/mod.rs
if let Some(node) = req.cfg.placement().node() {
    return bridged::make(req, &type_id, node);
}
```

`make_source` is the function pointer every loaded source provide carries, so
every plugin source in the process funnels through it. Below that line is the
local path, unchanged. Keeping the fork to one place is not tidiness: it is
what makes "identical" checkable rather than aspirational, because there is
nowhere else for the two paths to drift apart.

## What could not be made identical

Three things, and each one is a real cost rather than an oversight.

**The media is encoded.** Raw 1080p30 I420 is 93 MB/s and does not go over a
network. So the node encodes once with its own catalogue choice and the core
decodes once on its usual hardware aware path. One encode and one decode per
remote source. It is the same price a browser source already pays, and it buys
the machine boundary, but it is not free and the latency budget makes it
visible.

**The latency is declared, not zero.** A local plugin's proxy join claims zero
latency, which is true. A network has a jitter buffer, and if nothing says so
then every sink downstream assumes zero and a local file plays early by exactly
that much. So a remote source declares a budget at ingress and the pipeline
answers the LATENCY query with it. Two remote cameras with the same budget stay
in lip sync with each other and with the file, because everything delays
equally. The number is in the config where an operator can see it.

**The clock has to be shared.** Every pipeline in the core uses the programme
clock and base time. That does not survive a network on its own, so the core
publishes its clock with a `GstNetTimeProvider` and each node slaves a
`GstNetClientClock` to it, and a node starts no plugin until that clock has
synced. Without it two cameras on one node would agree with each other and
disagree with the core, which is the worst of the three possible outcomes
because it looks like it is working.

## What it buys

An operator at the core sees `place: node:studio-b` in the source list and
nothing else changes. They open the source's settings and get the plugin's own
form. They call its tools. They read its health and its log lines, in the same
file as the supervisor's decision about it, with the same instance tag. When
the machine goes, they get an alert naming the machine, the programme holds the
freeze frame for 45 seconds as it already does for a dead local source, and
then the slate. When it comes back, the sources come back with it and nothing
was restarted.

A plugin author gets none of this to write. Their plugin declares
`placements = ["sidecar", "node"]` in its manifest and is finished. A plugin
that declares only `sidecar` is refused a node placement with an error naming
what it did declare, which is the honest answer rather than a silent
half-working one.

## The tiers, for completeness

| Tier | `place` | What it is |
|---|---|---|
| 0 | `core` | Rust, compiled in, behind the same trait as everything else |
| 1 | `in-process` | A Rust crate a custom build compiled in behind a feature |
| 2 | `sidecar` | A separate process here. Any language. The default for third parties |
| 3 | `node:<name>` | A tier 2 plugin hosted by a node on another machine |

There is no stable dylib ABI and there will not be. The audit priced one at two
to three weeks and OBS is the evidence for what it costs afterwards: every
plugin recompiled on every release, and a crash in any of them taking the
programme with it. A process boundary costs a pipe. The pipe is cheaper.

## Where to look

* [Add a second machine](../how-to/add-a-node.md), to do it.
* [Nodes reference](../reference/nodes.md), for every key and method.
* [Why plugins are processes](why-plugins-are-processes.md), for the tier 2
  argument this one builds on.
* `crates/godwinmix-core/src/node/` and
  `crates/godwinmix-core/src/plugin/host/bridged.rs`, for the code.
