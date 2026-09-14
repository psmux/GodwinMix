# godwinmix-host

The tier 2 plugin host: everything about running a plugin beside the core that
is not a pipeline.

A tier 2 plugin is a separate process. It speaks JSON-RPC 2.0 over stdin and
stderr, one object per line, and it hands media across the process boundary
through a transport the two sides agree on at the handshake. A crash costs one
source and never the programme output.

| Module | What it owns |
|---|---|
| `launch` | `[run]` and `[build]` turned into argv, an environment and a cwd; the `GMX_*` variables |
| `channel` | JSON lines, the 4 MiB limit, several requests in flight per direction |
| `handshake` | the api range check and the transport negotiation |
| `lifecycle` | the state machine of 03 section 7 and the restart backoff |
| `budget` | `[plugins.<name>]` limits and the `on_over_budget` policy |
| `sampler` | cpu and rss per process, read once a second |
| `offline` | `gmx plugin test --offline`: a transcript, a binary, no core |

## What it does not own

The child process itself. `godwinmix-core` already carries the process group
teardown, the PID 1 orphan reaper and the Windows stdout reader thread that a
sidecar needs, and a second copy of those would be a second set of bugs. The
core's `plugin::host` module spawns and wires; this crate decides what to say
and what a line means.

No GStreamer, no axum, no tokio. Everything here unit tests on a machine with
no media stack installed.

## Where the rules come from

`docs/reference/plugin-lifecycle.md` and `docs/reference/plugin-manifest.md`.
The manifest types themselves live in `godwinmix-protocol::plugin`, because
both the core and a plugin's SDK read them and one description of a protocol
cannot drift.
