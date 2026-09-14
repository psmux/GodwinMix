# godwinmix-core

The GodwinMix mixing engine, as a library.

Multiple sources come in, one of them is on programme at a time, and the
programme feed goes out to one or more destinations without ever stopping.
Switching source is instant and does not disturb the outgoing stream, because
the output encoder is started once and runs for the life of the broadcast;
everything that changes happens upstream of it in raw video.

```sh
cargo add godwinmix-core
```

You need GStreamer with its development headers, the same as building
GodwinMix itself. Nothing else: no HTTP server, no argument parser, no MCP
server and no web UI come with this crate, and `cargo tree` will show you that.

```rust
use godwinmix_core::prelude::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    gstreamer::init()?;
    let cfg: Config = toml::from_str("")?;       // every field has a default
    let (mut mix, handle, cmd_rx, _bus_rx) = Mixer::build(cfg)?;
    mix.start()?;
    let thread = mixer::spawn(mix, cmd_rx, handle.clone());

    // ... send commands on `handle` ...

    handle.send(Command::Shutdown).ok();
    let _ = thread.join();
    Ok(())
}
```

`examples/embed.rs` is the same thing with a source added, a take, two seconds
on air and a clean shutdown. `tests/embed.rs` runs it and CI builds it, so the
example cannot rot.

```sh
cargo run --example embed -p godwinmix-core
```

## What is in here

The mixer thread and its command queue, the source and output lifecycles, the
plugin traits and the built in kinds, the codec catalogue, the configuration,
scenes and layouts, the OBS importer, multiview, snapshots, the media library,
the file converter, the codec probe, and the instrumentation: metrics, the
session log, the log layer, the trace id and pipeline introspection.

The types that go over the wire come from `godwinmix-protocol`, which this
crate depends on and re-exports through `state`, so a status this engine
produces is the same record the HTTP API returns.

## What is not

No HTTP server, no JSON-RPC dispatcher, no MCP server, no command line, no
plugin loader. Those are the `godwinmix` binary and, when it is written, the
`godwinmix-host` crate. If a module here needs axum, clap or reqwest, it is in
the wrong crate.

## Documentation

* [Embed the engine in your own program](../../docs/how-to/embed-the-engine.md)
* [The crate map](../../docs/explanation/architecture.md)
* [Why the programme never stops](../../docs/explanation/why-the-programme-never-stops.md)

Licensed under Apache-2.0.
