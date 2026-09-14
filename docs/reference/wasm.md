# Plugins as WebAssembly components (tier W)

A `service` or a `transition` plugin can run as a WebAssembly component inside
the core process instead of as a process beside it. It is sandboxed, it is held
to a fuel allowance and a deadline on every call, and it touches no media. That
last part is not a limitation to work around: it is the design, and
[why WASM is not on the frame path](../explanation/why-wasm-is-not-on-the-frame-path.md)
says why.

The world is `wit/godwinmix-plugin.wit` in the repository. It is
`package godwinmix:plugin@1.0.0` and it has three interfaces and three worlds.

## The world against the protocol

Every method the world exposes is a method a stdio plugin already implements.
The left column is the WIT; the right is the row in the plugin protocol
reference, which is the contract a process plugin speaks on its pipe. Nothing
in the world is new, and nothing in the protocol's service or transition rows
is missing from it.

| WIT | JSON-RPC method | Params | Result |
|---|---|---|---|
| `service.initialize(hello)` | `initialize` (handshake) | the `initialize` result the core sends, as a record | the `initialize` params the plugin sends, as a record |
| `service.configure(params-json)` | `configure` | `{params}` as a JSON string | `{applied, restart_required, reason}` |
| `service.health()` | `health` | none | `{state, detail?, latency_ms?}` |
| `service.tool-call(name, arguments-json)` | `tool.call` | `{name, arguments}` split into two arguments | the MCP result object as a JSON string |
| `service.hook(name, payload-json)` | `hook` | the hook envelope `{hook, ts, payload}` as a JSON string | the hook's answer as a JSON string |
| `service.shutdown(reason)` | `shutdown` | `{reason}` | none; the instance is dropped |
| `transition.initialize(hello)` | `initialize` | as above | as above |
| `transition.render(request-json)` | `render` | `{from, to, progress, running_time_ns}` as a JSON string | `pads(..)` or `curve(..)`, the two answers `render` may give |
| `transition.configure(params-json)` | `configure` | as the service's | as the service's |
| `transition.health()` | `health` | as the service's | as the service's |
| `transition.shutdown(reason)` | `shutdown` | as the service's | as the service's |

Four methods of the protocol are deliberately absent, and all four are about
media: `start`, `stop`, `seek`, `position`, `keyframe` and `audio.set` belong
to a `source`, an `output` or a `filter`, and none of those runs here.
`discover` is absent for the same reason a `device` is not a tier W kind: a
device finds things on a network, and a component with no sockets cannot.

### Why JSON on the boundary

The params and results that carry structure cross as strings holding the same
JSON a stdio plugin would have written on its line. Byte for byte the same: the
host does not reshape anything on the way in or out.

That is worth a sentence of defence, because the component model can describe
records and the world could have used them. Three reasons it does not:

1. One wire shape. A plugin that moves between `sidecar` and `wasm` changes
   which crate it depends on and nothing else. The supervisor, the hook
   dispatcher and the transition renderer call both placements through the same
   `call(method, params)` and cannot tell them apart.
2. The protocol is allowed to grow. `render`'s params are widened for scenes
   before level 1 freezes (03 section 6); a plugin that reads them out of JSON
   keeps working, while one whose record gained a field would not link.
3. The recorded transcripts are the same file. `gmx plugin test --offline`
   replays `tests/transcript.jsonl` against a component with no core and no
   process, and the file is in the format every other plugin's is.

The records the world *does* define are the ones that never change shape: the
handshake, health, the error, and what the host granted.

## What the host grants

A component is given the minimum and told what it was given, in
`capabilities` on the handshake. It reads that and adapts; it never decides it.

| Grant | Default | How it is decided |
|---|---|---|
| `core-call` | on | on for a running core, off under `gmx plugin test` |
| `take` | off | on when the plugin asked for a `take.*` hook in its manifest |
| `events` | on | always, on a core with an event bus |
| `filesystem` | off | the manifest declares `wasi = ["filesystem"]` **and** the operator lists the plugin in `[plugins] allow_wasi` |
| `network` | off | the same two, with `"network"` |
| `fuel-per-call` | 500,000,000 | `[plugins.<name>] wasm_fuel` |
| `deadline-ms` | 500 | `[plugins.<name>] wasm_deadline_ms`, clamped to 1 to 5000 |
| `max-memory-mb` | 64 | `[plugins.<name>] max_rss_mb`, the same key a process is held to |

Neither WASI grant is given on the manifest alone. The plugin author declares
what the plugin needs and the operator decides whether this machine gives it,
which is the only arrangement where "it asked for the filesystem" is
information rather than a formality.

### What `core-call` reaches

Read methods, and `program.take` when `take` was granted:

```
core.info  core.api  agent.state  codec.list
program.get  program.history
scene.list  scene.get
source.list  source.get  output.list  output.get  filter.list
plugin.list  plugin.describe  tool.list
```

Anything else answers `-32002` and the message names the whole allow list and
says that the `sidecar` placement gets `GMX_RPC`, a token and the rest of the
protocol. A component that needs to remove a source is a component that should
be a process.

## The limits, and what happens at each

| Limit | Enforced by | What a caller sees |
|---|---|---|
| fuel, per call | wasmtime, counting executed operations | the call answers `Err`; a `render` falls back to the built in cut for that take, a hook becomes `event/hook.blocked` |
| deadline, per call | wasmtime epochs, bumped every millisecond | the same |
| memory | a `ResourceLimiter` refusing growth past the cap | the component's allocation fails and it traps; the call answers `Err` |
| the caller's own wait | the reply channel, deadline plus 50 ms | the caller gives up and the worker is cut separately |

Both a fuel allowance and a deadline are set, because they catch different
things. Fuel stops a loop that is doing work. The deadline stops a component
blocked on something that never returns, which fuel cannot see because no
operations are being executed.

Fuel and the epoch are set fresh for every call: a component that nearly ran
out on the last one is not cut short on this one, and one that ran away does
not carry its debt forward. The handshake gets the default allowance rather
than the per call one, so a tight `wasm_fuel` means short hooks and not a
plugin that will not load.

### A trap costs the instance, not the plugin

A component that traps, by any of those routes, cannot be entered again:
wasmtime marks the instance unusable and every later call would answer
`cannot enter component instance`. That is a useless thing for an operator to
read and it would leave a transition silently broken for the rest of a show.

So a trap ends the instance. The worker drops the store and stops, the
instance reads `failed`, and the supervisor builds a fresh one from the same
file on its next pass, a quarter of a second later, under the same backoff a
crashing process gets. A call in between says so and says what happens next.

A plugin that answers with an `error` record has not trapped. That is an
ordinary value, the instance is perfectly well, and the next call works.

## Where a component runs

On a worker thread of its own, one per instance, owning one wasmtime `Store`.
Nothing is shared but the engine. A call is a message to that thread and a
reply on a channel, and the caller waits with its own deadline.

That matters most for `render`. A transition's answer is asked for while a take
is being set up, and if the component ran on the mixer's thread a slow one
would hold the command queue. It does not: the mixer waits on a channel, gives
up at its deadline, and takes the built in cut.

## Placements, and the media refusal

`placements = ["wasm"]` in the manifest, `[run] wasm = "plugin.wasm"` beside
it. A plugin that declares `wasm` alongside `sidecar` stays a process until an
operator writes `place = "wasm"` under `[plugins.<name>]`: a process is what
everything else in the core is built around, so it is the conservative default.

A `source`, `output`, `filter` or `encoder` asked to run at this placement is
refused with `-32005` before anything is built, and `data.placements` names the
three that do carry media:

```json
{"code": -32005,
 "message": "`demo/source` is a source and the `wasm` placement carries no media, so a frame would never reach it. Media crosses at these placements: in-process, sidecar, node. Tier W runs service, transition, panel logic only; docs/explanation/why-wasm-is-not-on-the-frame-path.md says why.",
 "data": {"placements": ["in-process", "sidecar", "node"], "type": "demo/source", "kind": "source", "retryable": false}}
```

The manifest validator catches the same mistake earlier: a plugin whose only
placement is `wasm` and which provides a media kind fails `gmx plugin test`
on check 1.

## The binary size, and why the feature is off

wasmtime is a compiler. Measured on `macos-aarch64`, release profile,
`lto = "fat"`, `codegen-units = 1`:

| Build | Bytes | |
|---|---|---|
| `cargo build --release` | 18,403,824 | 17.6 MiB |
| `cargo build --release --features wasm` | 30,383,712 | 29.0 MiB |
| the difference | 11,979,888 | 11.4 MiB |

09 section 4 item 4 gives a feature 4 MB before it has to be opt in, and the
whole binary 30 MB on `linux-aarch64`. This costs 11.4 MiB and takes the binary
to 30.4 MB, so `wasm` is **off by default**. A build that wants it says so:

```
cargo build --release --features wasm
```

`gmx doctor` reports which build you have:

```
ok    wasm host          wasmtime 36 (LTS), component model, fuel and epoch limits on
```

and, on a build without it:

```
warn  wasm host          this build carries none, so a plugin with placements = ["wasm"] will not start. Rebuild with `cargo build --release --features wasm` if you need one.
```

A warning rather than a failure, because a mixer with no WebAssembly host runs
a show perfectly well. It becomes a problem only when a `wasm` only plugin is
installed, and that refusal names the flag too.

wasmtime 36 is the LTS line. It asks for Rust 1.86 where the current release
asks for 1.95, and `crates/godwinmix-wasm` carries that `rust-version` on
itself: the rest of the workspace still builds on 1.82.

## What a component costs to run

The engine is built on first use, so a core with no tier W plugin installed
pays nothing for the feature beyond the bytes on disk. Loading one component
compiles it: about 1.7 to 2 seconds for the 100 KB examples in this repository,
once, at startup. After that a call is a channel round trip and the component's
own work.

`plugin.list` and `plugin.stats` carry a component beside every process
instance, with `pid: null` and `rss_bytes` read from its linear memory. There is
no other number a component has: it has no process for the sampler to find.

## Hot reload

`plugin.reload <name>` stops the component and builds a new one from the file
on disk. There is no process to outlive the swap and no handshake to roll back
to, so the whole of it is a stop and a start.

## The three worlds

| World | Imports | Exports | Used for |
|---|---|---|---|
| `service-plugin` | `host` | `service` | a `service` provide |
| `transition-plugin` | `host` | `transition` | a `transition` provide |
| `plugin` | `host` | `service`, `transition` | a component that is both, and what `godwinmix-sdk-wasm` generates |

The host instantiates a component under one of the two narrow worlds, decided
by the provide's `kind`. A component that exports more than the world names is
fine, which is why the SDK can generate one set of bindings and stub the half a
plugin did not implement.

## See also

* [Write a WASM plugin](../how-to/write-a-wasm-plugin.md), from nothing to a
  passing harness.
* [Why WASM is not on the frame path](../explanation/why-wasm-is-not-on-the-frame-path.md).
* [The plugin manifest](plugin-manifest.md) for `[run] wasm`, `wasi` and the
  `wasm` placement.
* [The plugin protocol](plugin-protocol.md) for the methods this world mirrors.
