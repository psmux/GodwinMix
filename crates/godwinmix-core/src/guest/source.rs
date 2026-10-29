//! `guest/source`: what a guest's session sends, as a mixer source.
//!
//! `guest://<id>` reads the inter channel the guest's session writes (see
//! `super::branch`). It is up from the moment it is added, whether or not
//! the guest has connected yet: black and silence until the first frame,
//! the last picture held for a few seconds when the guest's connection
//! drops, and black again after that. A guest who comes back writes to the
//! same channel and the source picks it up with nothing rebuilt.

use crate::caps::CanvasCaps;
use crate::config::Params;
use crate::gstutil::make;
use crate::plugin::kinds::{assemble, BuildCtx, Ingest, KindParts, Wiring};
use crate::plugin::source::{unknown_method, Provide, Source, SourceRequest};
use crate::plugin::{
    Capability, CapabilitySet, Configure, Health, Hello, Manifest, MediaDecl, MediaEnds, PluginState, ProvideKind,
    Ready, StreamMode, Tier, API_LEVEL,
};
use anyhow::{Context, Result};
use gstreamer::prelude::*;
use serde_json::Value;
use std::sync::atomic::Ordering;

/// The address scheme.
pub const SCHEME: &str = "guest://";

/// How long the last picture is held after the guest's frames stop.
const HOLD_NS: u64 = 5_000_000_000;

pub const MANIFEST: Manifest = Manifest {
    plugin: "guest",
    id: "source",
    kind: ProvideKind::Source,
    api: API_LEVEL,
    description: "A guest's camera and microphone, taken in over WHIP from an invite link",
    uri_schemes: &[SCHEME],
    rank: 256,
    media: MediaDecl { video: StreamMode::Raw, audio: StreamMode::Raw, alpha: false, thumb: true },
    capabilities: CapabilitySet::new().with(Capability::RestartInPlace).with(Capability::Health),
    latency_ms: 0,
    tier: Tier::Core,
};

pub const PROVIDE: Provide = Provide { manifest: MANIFEST, claims, make: new };

fn claims(uri: &str) -> Option<u16> {
    uri.trim().to_ascii_lowercase().starts_with(SCHEME).then_some(MANIFEST.rank)
}

/// The guest a `guest://<id>` address names.
pub fn guest_of(uri: &str) -> Option<&str> {
    let rest = uri.trim().strip_prefix(SCHEME)?;
    let id = rest.split(['/', '?', '#']).next().unwrap_or("");
    (!id.is_empty()).then_some(id)
}

/// The address of a guest's source.
pub fn uri_of(guest: &str) -> String {
    format!("{SCHEME}{guest}")
}

fn new(req: SourceRequest<'_>) -> Result<Box<dyn Source>> {
    let guest = guest_of(&req.cfg.uri)
        .with_context(|| format!("a guest source's address is guest://<invite id>, not `{}`", req.cfg.uri))?
        .to_string();
    Ok(Box::new(GuestSource { guest, ctx: req.ctx(), running: false }))
}

pub struct GuestSource {
    guest: String,
    ctx: BuildCtx,
    running: bool,
}

impl Source for GuestSource {
    fn manifest(&self) -> &Manifest {
        &MANIFEST
    }

    fn initialize(&mut self, hello: Hello) -> Result<Ready> {
        self.ctx.canvas = hello.canvas;
        Ok(Ready { manifest: MANIFEST, latency_ms: MANIFEST.latency_ms, capabilities: MANIFEST.capabilities })
    }

    fn start(&mut self, canvas: &CanvasCaps, thumb: bool) -> Result<MediaEnds> {
        self.ctx.canvas = canvas.clone();
        let (id, channel) = (&self.ctx.id, super::channel(&self.guest));
        let vsrc = make("intervideosrc", &format!("{id}-guest-v"))?;
        vsrc.set_property("channel", &channel);
        crate::probe::set_int(&vsrc, "timeout", HOLD_NS as i64);
        let asrc = make("interaudiosrc", &format!("{id}-guest-a"))?;
        asrc.set_property("channel", &channel);
        let ends = assemble(&self.ctx, thumb, Ingest::default().with([vsrc.clone(), asrc.clone()]), |w: &Wiring| {
            vsrc.link(&w.norm.video_entry()).context("linking a guest's picture")?;
            asrc.link(&w.norm.audio_entry()).context("linking a guest's sound")?;
            w.has_video.store(true, Ordering::Relaxed);
            w.has_audio.store(true, Ordering::Relaxed);
            Ok(KindParts::default())
        })?;
        self.running = true;
        Ok(ends)
    }

    fn stop(&mut self) -> Result<()> {
        self.running = false;
        Ok(())
    }

    fn configure(&mut self, _params: &Params) -> Result<Configure> {
        Ok(Configure::Applied)
    }

    fn health(&self) -> Health {
        Health::of(if self.running { PluginState::Running } else { PluginState::Starting })
    }

    fn call(&mut self, method: &str, _params: Value) -> Result<Value> {
        match method {
            "restart" => Ok(Value::Null),
            other => Err(unknown_method(&MANIFEST, other, &["restart"])),
        }
    }
}
