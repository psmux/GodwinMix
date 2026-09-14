# Embed the engine in your own program

You want the mixing, not the mixer. No HTTP server, no web UI, no command line:
a pipeline that takes sources in, puts one of them on programme, and pushes the
result out, driven by your own code.

That is `godwinmix-core`. It is the same engine the `godwinmix` binary runs, as
a library.

## What you need

GStreamer with its development headers, the same as building GodwinMix itself:

```sh
brew install gstreamer                  # macOS
# Linux: your distro's gstreamer plus plugins base, good, bad, ugly, libav, rs
# Windows: the MSVC runtime and development MSIs from gstreamer.freedesktop.org
```

Then, in your own crate:

```sh
cargo add godwinmix-core
cargo add anyhow gstreamer toml
cargo add tokio --features rt-multi-thread,macros,sync,time
```

`gstreamer` and `toml` are there because you call `gstreamer::init()` yourself
and because a `Config` is a TOML document. Nothing else about GodwinMix leaks
into your dependency list: the engine carries no HTTP server and no argument
parser, and `cargo tree` will show you that.

## The shortest thing that works

```rust
use godwinmix_core::prelude::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Once per process, before anything is built. The engine does not do this
    // for you, because your program may have done it already.
    gstreamer::init()?;

    // Every field has a default, so an empty document is a working mixer.
    let mut cfg: Config = toml::from_str("")?;
    cfg.canvas.width = 1280;
    cfg.canvas.height = 720;
    cfg.canvas.fps = 30;

    let (mut mix, handle, cmd_rx, _bus_rx) = Mixer::build(cfg)?;
    mix.start()?;
    let thread = mixer::spawn(mix, cmd_rx, handle.clone());

    handle.send(Command::Shutdown).ok();
    let _ = thread.join();
    Ok(())
}
```

`Mixer::build` hands back four things: the mixer itself, a `MixerHandle` you
clone anywhere you want to send it a command, the receiving end of the command
queue, and a stream of pipeline bus events. One OS thread owns the pipelines
for the life of the process and everything else talks to it through the handle.

## The example in the repository

`crates/godwinmix-core/examples/embed.rs` is the same thing with a source
added, a take, two seconds on air and a clean shutdown. Run it from a checkout:

```sh
cargo run --example embed -p godwinmix-core
```

It prints the programme source and the source count it read while live. The
parts worth copying:

**Bus messages go through the same queue as commands.** A camera dying and an
operator's take are then handled on one serialised path and cannot race.

```rust
let handle = handle.clone();
tokio::spawn(async move {
    while let Some(ev) = bus_rx.recv().await {
        if handle.send(Command::Bus(ev)).is_err() {
            return;
        }
    }
});
```

**A source is a `SourceConfig`, which is a TOML document.** `test/source` is
the built in colour bars and tone: it needs no network, no file and no browser,
which makes it the right thing to start with.

```rust
let source: SourceConfig = toml::from_str(r#"
    id = "bars"
    type = "test/source"
    uri = "test://smpte"
"#)?;
let (tx, rx) = tokio::sync::oneshot::channel();
handle.send(Command::AddSource(Box::new(source), Some(tx))).ok();
rx.await?.map_err(anyhow::Error::msg)?;
```

**Commands that can fail take an acknowledgement channel.** The mixer thread
answers on it when it has done the work, or with the reason it did not. Send
without one when you do not want to wait.

```rust
let (tx, rx) = tokio::sync::oneshot::channel();
handle.send(Command::Take {
    source: Some("bars".into()),
    at_running_time_ms: None,
    ack: Some(tx),
}).ok();
rx.await?.map_err(anyhow::Error::msg)?;
```

**`Command::Status` is how you read state.** It answers a `MixerStatus`, which
is the same record `GET /api/v1/status` returns, because it is the same type.

```rust
let (tx, rx) = tokio::sync::oneshot::channel();
handle.send(Command::Status(tx)).ok();
let status = rx.await?;
println!("on air: {:?}", status.program);
```

`tests/embed.rs` runs this example as a test, and CI builds it, so the page you
are reading cannot drift from code that works.

## What else is in the crate

`godwinmix_core::prelude` is five names: `Config`, `Mixer`, `MixerHandle`,
`Command`, and the `mixer` module. Everything else you reach for by its full
path, which keeps the layering readable:

| You want | Where it is |
|---|---|
| Sources and outputs as configuration | `godwinmix_core::config` |
| The status and event records | `godwinmix_core::state` (re-exported from `godwinmix-protocol`) |
| Codec selection and the catalogue | `godwinmix_core::catalogue` |
| Scenes, layouts and the OBS importer | `godwinmix_core::scene` |
| The plugin traits and built in kinds | `godwinmix_core::plugin` |
| Metrics, the session log, log levels | `godwinmix_core::observe` |
| The mosaic | `godwinmix_core::multiview` |
| Snapshots | `godwinmix_core::snapshot` |

## What you do not get

No HTTP server, no JSON-RPC dispatcher, no MCP server, no web UI and no command
line. Those are the `godwinmix` crate, which is the binary. If you want them,
run the binary and talk to it over the protocol: that is what every first party
client does, and the contract is `godwinmix-protocol`, which you can also
depend on alone.

No plugin loader either. A tier 2 plugin is a separate process, and the host
that starts and supervises one will be `godwinmix-host`. An embedded engine
runs the built in kinds and whatever you register yourself.

## See also

* [The crate map](../explanation/architecture.md), for what belongs where.
* [How a source works](../explanation/how-a-source-works.md).
* [Why the programme never stops](../explanation/why-the-programme-never-stops.md),
  which is the constraint every command in the queue is written against.
