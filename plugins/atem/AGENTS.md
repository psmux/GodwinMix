# AGENTS.md

For a coding agent changing this plugin. Read this before touching anything.

## What this is

A GodwinMix `service` plugin in Rust: no media of its own. It drives
Blackmagic ATEM switchers with the client in `crates/godwinmix-atem` and,
in `follow` and `lead` modes, ties them to GodwinMix's programme over the
public protocol (`event/program.took` in, `program.take` out). Agents and
the page reach every tool through the core's `tool.call` as
`atem/<tool>`.

## Build and test

```sh
cargo test -p gmx-atem -p godwinmix-atem
cargo clippy --all-targets -p gmx-atem -p godwinmix-atem
```

No GStreamer, no switcher and no core are needed. The tests use the fake
switcher in `crates/godwinmix-atem/tests/fake/`, included by path.

## Where things are

| Path | What it is |
|---|---|
| `src/settings.rs` | `schemas/atem.json` and the mode |
| `src/desk.rs` | every switcher, with its name, mode, map and last macro |
| `src/view.rs` | what `list`, `state` and `tally` answer |
| `src/tools/` | `setup.rs` (list, state, tally, connect, disconnect, map, mode), `control.rs` (cut, auto, ftb, preview, program, transition), `keys.rs` (dsk, usk, aux, macro) |
| `src/sync/` | follow and lead as pure functions, the loop guard, and `run.rs`, the only part that talks to the core |
| `src/service.rs` | the SDK `Service`, and the line by line mode for a terminal |
| `gmx-plugin.toml`, `schemas/` | the manifest and every tool's schema |
| `tests/` | tools against fake switchers, refusals, and the transcript |

## Rules that are easy to break

* **Tool names and arguments are a contract.** The page calls
  them (`ui/panels/camera/api.js`). Add tools and arguments; do not rename
  them. A change to an answer's shape goes with a change to `atem()` in
  `ui/panels/camera/shapes.js` and to `toPlugin` in
  `ui/panels/camera/demo/atem.js`.
* **`tools::NAMES` and the manifest list the same tools in the same order.**
  `tests/offline.rs` checks it.
* **The `aux` tool's schemas are `aux_source.json`.** AUX is a reserved file
  name on Windows with any extension.
* **The protocol stays sans IO.** Rules about packets belong in
  `godwinmix-atem`'s `session`, tested without a socket; this plugin only
  calls `Atem::send_now` and reads snapshots.
* **No move that changes nothing**, and every automatic move goes through
  the `Guard`. See the head of `src/sync/mod.rs`.
* **Every refusal says what is wrong and what to do**, with the client's
  error code in `data.code`.
* Modules stay under 150 lines.
