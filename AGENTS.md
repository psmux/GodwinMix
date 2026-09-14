# AGENTS.md

For a coding agent working in this repository. Short on purpose; every section
names the file that has the detail.

## What this is

GodwinMix is a live video mixer. Rust on GStreamer 1.28, axum and tokio. It runs
headless behind a web UI, ships as a Tauri desktop app, and every component is
meant to be replaceable by a plugin. It has to stay light enough for a Raspberry
Pi and a five year old laptop with no GPU.

The rule underneath everything: **the programme output never stops.** Nothing
you add may block, stall or slow the encoder, and nothing may do blocking work
on a GStreamer streaming thread or the bus handler. A plugin that dies costs one
source and nothing else.

## Build, test, smoke test

```sh
cargo build                     # the workspace
cargo test                      # the whole suite, real GStreamer elements, no mocks
cargo test -p godwinmix-core    # one crate
cargo clippy --all-targets      # fix what you introduced
dev/smoke.sh                    # one run of everything a person does, against a real core
```

You need a Rust toolchain at the `rust-version` in the root `Cargo.toml` and
GStreamer with its development headers. `CONTRIBUTING.md` has the per platform
install lines.

Plugin work has its own loop, which needs neither a core nor GStreamer:

```sh
cargo test -p godwinmix-sdk                  # the SDK, no GStreamer
cargo test -p godwinmix-sdk --features gst   # and the GStreamer backed half
./examples/test-zero-dep.sh                  # the no dependency Python plugin
./templates/test-templates.sh                # every template this machine can run
```

## Where things are

| Path | What is in it |
|---|---|
| `crates/godwinmix-protocol/` | the wire contract: types, methods, errors, scopes, the OpenAPI and MCP tables. No media stack |
| `crates/godwinmix-core/` | the mixing engine as a library: pipeline, sources, outputs, filters, scenes, caps, the codec catalogue |
| `crates/godwinmix-host/` | the tier 2 sidecar host: spawning plugins, the core side of the handshake. Mostly empty today |
| `crates/godwinmix-sdk/` | what a Rust plugin author depends on: the manifest, the JSON lines loop, pacing, the media writers |
| `crates/godwinmix/` | the binaries (`godwinmix` and `gmx`), the control server, the CLI, the MCP server |
| `templates/` | five plugin templates, one per language, each a working source plugin |
| `examples/` | `zero-dep-source.py`, a whole plugin in Python with no imports outside the standard library |
| `skills/` | the `godwinmix-operate` and `godwinmix-develop` skills, installed by `gmx skill install` |
| `docs/` | tutorials, how to guides, reference, explanation. Four kinds, kept apart |
| `ui/`, `browser/`, `tauri-app/` | the web UI, the browser source sidecar, the desktop shell. Their own build, excluded from the workspace |
| `dev/` | smoke tests and harnesses a person runs by hand |
| `codecs.toml` | the codec catalogue, data rather than code |

`docs/explanation/architecture.md` says what belongs in which crate and which
way the dependencies point. Read it before adding a module.

## The rules a change is judged against

1. **Nothing runs unless asked.** Multiview, thumbnails, snapshots, meters,
   telemetry and push streams exist only while a client is subscribed. Adding
   work that happens whether or not anyone wants it is the thing to avoid.
2. **One protocol.** The first party UI, CLI, MCP server and plugins use the same
   public contract as a third party. If the reference implementation cheats, the
   ecosystem never forms.
3. **Every crate earns its place.** Prefer the standard library and what
   `Cargo.toml` already has. `godwinmix-sdk` depends on serde, serde_json and
   toml, and nothing else unless its `gst` feature is on; keep it that way.
4. **Ids are slugs and errors name the next step.** `cam-wide`, never a UUID.
   Every error message says the current state and what to do about it, with a
   `data` object a caller can act on.
5. **Cross platform.** Windows, macOS and Linux are all first class. A platform
   specific path is `cfg` gated with a documented fallback: `unixfd` is Linux
   and macOS, and a container on a pipe works everywhere.
6. **Docs land with the code.** A feature ships with a how to page under
   `docs/how-to/` and, where it has an interface, a reference page under
   `docs/reference/`, in the same branch. Never write output a command does not
   produce; where something does not exist yet, say so in one sentence.
7. **Small functions, small modules.** Nothing over 150 lines. A growing file is
   a new module.

## Working on a plugin

Start at `skills/godwinmix-develop/SKILL.md`, then the template for your
language and its own `AGENTS.md`. The protocol is
`docs/reference/plugin-protocol.md` and the manifest is
`docs/reference/plugin-manifest.md`.

The one thing that catches everyone: in container mode **stdout is media**, so a
`print` anywhere in a plugin corrupts the video stream. Log on stderr.

## Commits

One plain sentence saying what changed and why, then an optional body. Match the
style already in `git log`. No em dashes, no en dashes, no double hyphens as
punctuation anywhere in this repository, including commit messages.
