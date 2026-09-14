# godwinmix

The GodwinMix binaries, and everything that serves the engine.

Two binaries from one library. `godwinmix` is the name a package manager and a
service unit use; `gmx` is the name an operator types. They are the same code.

```sh
cargo run -- --example-config > godwinmix.toml
cargo run -- --config godwinmix.toml
cargo run -- --probe          # what the codec catalogue picks on this machine
cargo run -- --api-info       # the whole control protocol as JSON Schema
```

## What is in here

The axum control plane and its three doors (`/rpc`, `/api/v1` and the legacy
REST routes), the WebSocket layer, every method handler, the `/metrics` route,
the `gmx ctl` client, the MCP server over stdio, the web UI and panel serving,
the bench command, the scene, codec and observability subcommands, and the
clap definition behind `run()`.

| Directory | What is in it |
|---|---|
| `src/control/` | the server, the method table's handlers, REST, WebSocket |
| `src/cli/` | the subcommands that are not `ctl`: scene, codec, observe |
| `src/observe/` | the `/metrics` route and the `log.*` and `pipeline.*` methods |
| `src/ctl.rs` | `gmx ctl`, which talks to a running mixer over HTTP |
| `src/mcp.rs` | the Model Context Protocol server |
| `src/ui.rs` | the embedded web UI and the plugin panel directory |
| `src/bench.rs` | `gmx bench`, which is where every published number comes from |

The mixing is `godwinmix-core` and the wire contract is `godwinmix-protocol`.
This crate depends on both and adds nothing to either: a handler here reaches
into a running mixer, and the description of the method it implements lives in
the protocol crate.

## Documentation

* [The crate map](../../docs/explanation/architecture.md)
* [The command line](../../docs/reference/cli.md)
* [The HTTP API](../../docs/reference/http-api.md)

Licensed under Apache-2.0.
