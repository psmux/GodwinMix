//! `replay/source`: an event, or the last seconds of one angle, played back
//! from the replay rings at any speed down to a tenth.
//!
//! It is a clip as far as the mixer is concerned: it declares `seek`, it ends
//! with EOS and `params.at_end` decides what then (hold, the default, keeps
//! the last frame up), and the aligner places it on the programme's timeline.
//! Nothing is decoded until it is added; nothing is encoded at all.
//!
//! ```text
//!  appsrc (picture) -> decodebin -> normaliser
//!  appsrc (sound)   -> decodebin -> volume -> normaliser
//! ```
//!
//! `replay://` plays the first angle; `replay://cam1` plays cam1. Params:
//! `event`, `angle`, `seconds` (how far back, with no event) and `speed`.
//! `load` and `set` change what plays without a rebuild (`replay_call`).

use super::replay_feed::Feed;
use super::{assemble, BuildCtx, Ingest, KindParts, Wiring};
use crate::caps::CanvasCaps;
use crate::config::Params;
use crate::gstutil::make;
use crate::plugin::source::{Provide, Source, SourceRequest};
use crate::plugin::{
    Capability, CapabilitySet, Configure, Health, Hello, Manifest, MediaDecl, MediaEnds,
    PluginState, ProvideKind, Ready, StreamMode, Tier, API_LEVEL,
};
use crate::replay::{Shared, Window};
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use serde_json::Value;
use std::sync::Arc;

pub const MANIFEST: Manifest = Manifest {
    plugin: "replay",
    id: "source",
    kind: ProvideKind::Source,
    api: API_LEVEL,
    description: "An instant replay: an event or the last seconds of an angle, at any speed",
    uri_schemes: &["replay://"],
    rank: 256,
    media: MediaDecl { video: StreamMode::Container, audio: StreamMode::Container, alpha: false, thumb: true },
    capabilities: CapabilitySet::new().with(Capability::RestartInPlace).with(Capability::Seek).with(Capability::Health),
    latency_ms: 0,
    tier: Tier::Core,
};

pub const PROVIDE: Provide = Provide { manifest: MANIFEST, claims, make: new };

fn claims(uri: &str) -> Option<u16> {
    uri.trim().to_ascii_lowercase().starts_with("replay://").then_some(MANIFEST.rank)
}

fn new(req: SourceRequest<'_>) -> Result<Box<dyn Source>> {
    Ok(Box::new(ReplaySource { ctx: req.ctx(), params: Params::new(), playing: None }))
}

/// What a replay source is playing.
pub struct Playing {
    pub feed: Arc<Feed>,
    pub window: Window,
    pub angle: String,
    pub volume: gst::Element,
    pub pipeline: gst::Pipeline,
    pub sources: Vec<gst_app::AppSrc>,
}

pub struct ReplaySource {
    pub(super) ctx: BuildCtx,
    pub(super) params: Params,
    pub(super) playing: Option<Playing>,
}

/// The angle a `replay://cam1` address names.
fn angle_of_uri(uri: &str) -> Option<String> {
    let rest = uri.trim().get("replay://".len()..)?;
    let name = rest.split(['?', '#', '/']).next().unwrap_or("");
    (!name.is_empty()).then(|| name.to_string())
}

pub(super) fn text(p: &Params, key: &str) -> Option<String> {
    p.get(key).and_then(|v| v.as_str()).map(str::to_string)
}

pub(super) fn number(p: &Params, key: &str) -> Option<f64> {
    p.get(key).and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)))
}

impl ReplaySource {
    pub(super) fn shared(&self) -> Result<Arc<Shared>> {
        crate::replay::registry::find(self.ctx.origin)
            .context("replay is not running on this mixer. Turn it on with replay.start, then add the replay source again")
    }

    /// The window and clip these params ask for.
    pub(super) fn resolve(&self, p: &Params) -> Result<(Window, crate::replay::clip::Clip, f64)> {
        let shared = self.shared()?;
        let speed = crate::replay::shared::check_speed(number(p, "speed"))?;
        let angle = text(p, "angle").or_else(|| angle_of_uri(&self.ctx.cfg.uri));
        let window = shared.window(text(p, "event").as_deref(), number(p, "seconds"), angle.as_deref())?;
        let clip = shared.clip(&window, &window.angle)?;
        Ok((window, clip, speed))
    }
}

impl Source for ReplaySource {
    fn manifest(&self) -> &Manifest {
        &MANIFEST
    }

    fn initialize(&mut self, hello: Hello) -> Result<Ready> {
        crate::replay::shared::check_speed(number(&hello.params, "speed"))?;
        self.params = hello.params.clone();
        self.ctx.canvas = hello.canvas;
        Ok(Ready { manifest: MANIFEST, latency_ms: 0, capabilities: MANIFEST.capabilities })
    }

    fn start(&mut self, canvas: &CanvasCaps, thumb: bool) -> Result<MediaEnds> {
        self.ctx.canvas = canvas.clone();
        let (window, clip, speed) = self.resolve(&self.params.clone())?;
        let feed = Feed::new(clip, speed);
        let id = self.ctx.id.clone();
        let vsrc = appsrc(&format!("{id}-src-replay"), feed.first_caps(), feed.duration());
        let vdec = make("decodebin", &format!("{id}-replay-vdec"))?;
        let volume = make("volume", &format!("{id}-replay-mute"))?;
        volume.set_property("mute", feed.muted());
        let asrc = feed.audio_caps().map(|c| appsrc(&format!("{id}-src-replay-a"), Some(c), feed.duration()));
        let mut els: Vec<gst::Element> = vec![vsrc.clone().upcast(), vdec.clone(), volume.clone()];
        let adec = match &asrc {
            Some(a) => {
                let d = make("decodebin", &format!("{id}-replay-adec"))?;
                els.extend([a.clone().upcast(), d.clone()]);
                Some(d)
            }
            None => None,
        };
        feed.wire(&vsrc, asrc.as_ref());
        let ends = assemble(&self.ctx, thumb, Ingest::default().with(els).livesync(false), |w: &Wiring| {
            vsrc.link(&vdec).context("linking the replay picture")?;
            volume.link(&w.norm.audio_entry()).context("linking the replay sound")?;
            w.route(&vdec, w.norm.video_entry(), volume.clone());
            if let (Some(a), Some(d)) = (&asrc, &adec) {
                a.link(d).context("linking the replay sound")?;
                w.route(d, w.norm.video_entry(), volume.clone());
            }
            Ok(KindParts::default())
        })?;
        let angle = window.angle.clone();
        let sources = std::iter::once(vsrc).chain(asrc).collect();
        self.playing = Some(Playing { feed, window, angle, volume, pipeline: ends.pipeline.clone(), sources });
        Ok(ends)
    }

    fn stop(&mut self) -> Result<()> {
        self.playing = None;
        Ok(())
    }

    fn configure(&mut self, params: &Params) -> Result<Configure> {
        crate::replay::shared::check_speed(number(params, "speed"))?;
        let only_end = |p: &Params| {
            let mut p = p.clone();
            p.remove("at_end");
            p
        };
        if only_end(params) == only_end(&self.params) {
            self.params = params.clone();
            return Ok(Configure::Applied);
        }
        Ok(Configure::RestartRequired("a replay plays a new stretch by being loaded again".into()))
    }

    fn health(&self) -> Health {
        Health::of(if self.playing.is_some() { PluginState::Running } else { PluginState::Starting })
    }

    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        self.replay_call(method, params)
    }
}

/// One of the two appsrcs: clip times, seekable, never live.
fn appsrc(name: &str, caps: Option<gst::Caps>, duration: u64) -> gst_app::AppSrc {
    let src = gst_app::AppSrc::builder()
        .name(name)
        .format(gst::Format::Time)
        .stream_type(gst_app::AppStreamType::Seekable)
        .is_live(false)
        .build();
    src.set_caps(caps.as_ref());
    src.set_duration(gst::ClockTime::from_nseconds(duration));
    src
}

