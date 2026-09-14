---
name: godwinmix-develop
description: Build on GodwinMix. Use when writing a plugin, a preset, a theme, a panel or a client against the GodwinMix control protocol, when adding a method to the core, or when a plugin fails the conformance harness. Covers the manifest, the JSON-RPC contract, the media transports and the test loop.
---

# Building on GodwinMix

Everything is a plugin, and the first party parts use the same public contract
as everybody else. If the reference implementation cheats, the ecosystem never
forms, so it does not cheat.

## Where the contract is

* `protocol.json` and `protocol.md` at the repository root are generated from
  the method table. Never edit them by hand; regenerate with
  `godwinmix --api-info > protocol.json`.
* `godwinmix --api-info --markdown` is the same thing to read.
* `core.api` over JSON-RPC answers the same document at runtime, so a client
  can discover the surface rather than being written against a copy of it.

## A plugin in one paragraph

A plugin is a manifest plus a program that speaks JSON-RPC 2.0. It sends
`initialize` first, the core answers with the canvas and the media transport
to use, the plugin sends `initialized`, and after that the core calls
`configure`, `start`, `stop`, `health` and `shutdown` on it. Media leaves by a
unix file descriptor where the platform has one and by a container on a pipe
where it does not, so the same plugin runs in process, beside the core, or on
another machine without being rewritten.

`gmx plugin new` writes a template: a manifest, a stub emitting colour bars, a
failing test, a check script that finishes in under thirty seconds, an
`AGENTS.md` and a `SKILL.md`. Start there rather than from an empty directory.

## Adding a method to the core

One place: `crates/godwinmix/src/control/methods/`. A `MethodDef` carries the
name, the scope, the summary, the params and result schemas, whether it is
mutating or destructive, and its MCP binding. Everything else is generated
from it:

* the `/rpc` dispatcher,
* the `/api/v1` route, by the transform rule in `protocol.md`,
* `protocol.json` and `openapi.json`,
* the MCP tool list.

A method cannot exist on one surface and not another, and a test fails the
build if a route appears that the transform rule did not produce.

Rules the tests enforce:

* No method blocks for more than five seconds. Work that would goes on the
  task table: `spawn_task` in `control/methods/tasks.rs`.
* Every mutating method returns the full resulting object, never an empty
  200, and accepts `idempotency_key`.
* Every destructive method accepts `dry_run: true` and answers with the diff
  it would make against live state.
* Every error names the current state and the next step. A test walks the
  refusals an agent can provoke and fails on a message that does not carry
  either a value that would have worked or a concrete instruction.
* The MCP hot tool list is budgeted in bytes. `gmx agent cost` prints it and
  CI fails on a five percent growth against `bench/agent-cost.json`.

## Never block the streaming thread

The programme output never stops. Nothing may block, stall or slow the
encoder. A pad probe runs on a streaming thread: it reads atomics and
returns. File reads, network calls and locks that another thread can hold go
on a blocking worker (`tokio::task::spawn_blocking`) or on the command queue.

## Nothing runs unless asked

No multiview, meters, thumbnails, snapshots, telemetry or push stream exists
until a client subscribes to it. Look at `godwinmix_core::telemetry` for the
pattern: a lease, an atomic the probe reads once per frame, and state that is
forgotten when the last holder goes.

## The test loop

```
cargo test --workspace     # real GStreamer elements, no mocks
cargo clippy --all-targets
dev/smoke.sh               # a whole core, every door, on a free port
```

Tests use real elements because a mock pipeline proves nothing about a
pipeline. `test/source` is colour bars and tone and needs no network, no file
and no browser.

## Cross platform

Windows, macOS and Linux are all first class. Every path compiles on all
three. Platform specific parts are `cfg` gated with a documented fallback: the
unix file descriptor transport is Linux and macOS only, and a container on a
pipe works everywhere.

## House style

Ids are legible slugs, never UUIDs. Errors name the state and the next step.
No function over 150 lines. Prefer small modules to growing large files. New
dependencies have to earn their place: prefer the standard library and what
`Cargo.toml` already has.
