# The crate map

GodwinMix is one repository, four crates and one binary. This page says what
belongs in each crate, what does not, and where a new module goes.

The rule underneath all of it: a dependency points one way only, from the thing
that serves towards the thing that mixes, and from both towards the contract.

```
  godwinmix            the binaries, the control server, the CLI, the web UI
      |                  |
      |                  v
      |             godwinmix-core     the mixing engine, embeddable
      |                  |
      v                  v
         godwinmix-protocol            the wire contract, no media stack
```

`godwinmix-host` sits beside the engine and depends only on the protocol. It is
empty today.

## godwinmix-protocol

`crates/godwinmix-protocol/`

Everything a client has to agree with the mixer about, and nothing else. The
request and response bodies, the status and event records, the error code
table, the scopes and the token rules, the method table's descriptions, the
trace id, and the generators that turn all of it into `protocol.json`,
`protocol.md`, `openapi.json` and the MCP tool list. `API_LEVEL` and
`API_COMPATIBLE` are here, which makes this the crate the compatibility promise
is written against.

Its dependencies are serde, serde_json and schemars, plus tracing for one
warning line and parking_lot for two mutexes. Nothing here knows about
GStreamer, about axum, or about a running mixer, and a test in CI fails if that
changes. That is what lets a client library, an SDK or a conformance harness
depend on it without installing a media stack first.

What does not belong here: anything that reads the state of a real mixer,
anything that builds a pipeline, and any handler. The method table holds the
*description* of a method (its name, scope, schemas and REST path); the code
that carries it out lives in `godwinmix`.

## godwinmix-core

`crates/godwinmix-core/`

The mixing engine as a library. The mixer thread and its command queue, the
source and output lifecycles, the plugin traits and the built in kinds, the
codec catalogue, the config, scenes and layouts, the OBS importer, multiview,
snapshots, the media library, the file converter, the probe, and the parts of
`observe` that instrument the engine: the metrics registry, the session log,
the log layer, the trace id task local and the pipeline introspection.

`cargo add godwinmix-core` gives someone the engine inside their own program.
`examples/embed.rs` is that program, `tests/embed.rs` runs it, and CI builds
both, so the claim is checked rather than asserted. See
[Embed the engine](../how-to/embed-the-engine.md).

What does not belong here: an HTTP server, a JSON-RPC dispatcher, an MCP
server, a command line parser, anything that serves a file over HTTP. If a
module needs axum, clap or reqwest, it is in the wrong crate. The
`--log-format` flag is the smallest example of the rule: the three choices are
`observe::logs::Format` here, and the `clap::ValueEnum` over the same three
names is in the binary.

## godwinmix-host

`crates/godwinmix-host/`

The tier 2 plugin host: the manifest reader, the handshake that picks a
transport, the transports themselves and the loader that starts, supervises,
restarts and kills a sidecar. None of it is written yet; the crate exists so
the layout is settled before the code arrives, and so the engine can be
embedded without a plugin loader linked in. See its `README.md`.

## godwinmix

`crates/godwinmix/`

Everything that serves the engine. The axum control plane and its three doors
(`/rpc`, `/api/v1` and the legacy REST routes), the WebSocket layer, every
method handler, the `gmx ctl` client, the MCP server, the web UI and panel
serving, the bench command, the scene, codec and observability subcommands, the
`/metrics` route, the clap definition and `run()`.

It builds two binaries from one library: `godwinmix`, which is the name a
package manager and a service unit use, and `gmx`, which is the name an
operator types. They are the same code.

## Where the data files live

`codecs.toml`, `layouts/`, `presets/`, `schemas/` and `ui/` stay at the
repository root, because that is where every document says they are and where
an operator expects them. The crates that embed them reach out with a relative
path (`include_str!("../../../../layouts/full.json")` from the engine,
`include_str!("../../../ui/index.html")` from the binary). Adding a file to one
of those directories means adding a line to the table in the module that
embeds it; there is no build script and no glob.

## Adding a module

Ask what the module needs and the crate follows:

* It describes something that goes over the wire, and needs neither GStreamer
  nor a server. Put it in `godwinmix-protocol`, add `pub mod` to `lib.rs`, and
  if it is a new method, register it in the binary's `control::methods`.
* It builds or drives a pipeline. Put it in `godwinmix-core`. Add `pub mod` to
  `lib.rs`. If it publishes a status or an event, the type goes in the protocol
  crate and the engine imports it.
* It answers a request, parses a command line, or serves a file. Put it in
  `godwinmix`. A subcommand goes in its `src/cli/`, a method handler in
  `src/control/methods/`, a route in `src/control/rest.rs` or in the module
  that owns it.

Write the `use` line to the crate that owns the item:
`use godwinmix_core::mixer::MixerHandle`, not a re-export through a nearer
crate. `godwinmix_core::prelude` is the one exception, and it is deliberately
five names long.

A crate under `crates/` joins the workspace by existing: the members list is a
glob. Shared dependency versions live in `[workspace.dependencies]` in the root
`Cargo.toml`, and a crate opts in with `dep = { workspace = true }` plus only
the features it needs.
