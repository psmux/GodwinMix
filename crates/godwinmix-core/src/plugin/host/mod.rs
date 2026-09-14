//! Tier 2 in the engine: the five trait implementations that make a process
//! look like a built in kind.
//!
//! ```text
//!   process.rs    spawn, handshake, talk, poll, stop; the control channel
//!   transport.rs  the media end per transport, and where its sockets live
//!   source.rs     SidecarSource:  the same MediaEnds every other source makes
//!   output.rs     SidecarOutput:  the programme, muxed, down a FIFO
//!   filter.rs     SidecarFilter:  raw out and raw back on two sockets
//!   service.rs    SidecarService and SidecarDevice: no media at all
//! ```
//!
//! The split with `godwinmix-host` is the split between a pipeline and a
//! protocol. That crate knows what to say, what a line means, what state an
//! instance is in and what it costs; this module knows how to spawn with the
//! core's own process group teardown and how to turn the result into
//! GStreamer. Neither knows about the other's half.
//!
//! Nothing in here is a privilege a third party does not have. A sidecar
//! reaches the mixer through `Source`, `Output` and `Filter`, which is the
//! same door `rtmp/source` uses.

pub mod filter;
pub mod output;
pub mod process;
pub mod service;
pub mod source;
pub mod transport;

pub use filter::SidecarFilter;
pub use output::SidecarOutput;
pub use process::{Notice, Sidecar};
pub use service::{SidecarDevice, SidecarService};
pub use source::{SidecarSource, SidecarSpec};
pub use transport::MediaDir;

use crate::plugin::source::{Source, SourceRequest};
use anyhow::{Context, Result};

/// Make a sidecar source for whatever `type` the config named.
///
/// The `make` half of every loaded provide's registry entry. It is a plain
/// function pointer, the same shape a built in kind's is, because the thing it
/// needs to tell one plugin from another is already on the request: the config
/// carries the `type`, and the loader knows the rest.
pub fn make_source(req: SourceRequest<'_>) -> Result<Box<dyn Source>> {
    let type_id = match req.cfg.type_id.as_deref().filter(|t| !t.trim().is_empty()) {
        Some(t) => t.trim().to_string(),
        None => crate::plugin::loader::source_for_uri(&req.cfg.uri)
            .map(|p| p.manifest.provide_id())
            .with_context(|| {
                format!(
                    "nothing installed opens `{}`; write `type` to say what it is",
                    req.cfg.uri
                )
            })?,
    };
    let instance = req.cfg.id.clone();
    let launched = crate::plugin::loader::launch_for(
        &type_id,
        &instance,
        crate::plugin::loader::mint_token(&type_id, &instance),
        crate::plugin::loader::rpc_url(),
    )?;
    let manifest = crate::plugin::loader::source_provide(&type_id)
        .map(|p| p.manifest)
        .with_context(|| format!("`{type_id}` is not a loaded source provide"))?;
    let spec = SidecarSpec {
        plugin: launched.plugin,
        provide: launched.provide,
        manifest,
        launch: launched.launch,
        ctx: launched.ctx,
        canvas: req.canvas.clone(),
        runtime: crate::plugin::loader::runtime_dir(),
    };
    let mut build = req.ctx();
    build.tier = crate::plugin::Tier::Sidecar;
    Ok(Box::new(SidecarSource::new(spec, build)))
}
