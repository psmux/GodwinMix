//! `image/source`: a still picture held on screen, or a numbered sequence of
//! pictures played as video.
//!
//! A still through `file/source` decoded one frame and ended, so the source
//! went `stalled` and was restarted over and over. Here the one frame is
//! decoded once and `imagefreeze` repeats it as a live stream, which costs a
//! copy per frame and no decode. A sequence (`frames/%04d.png`) is read by
//! `multifilesrc` at the frame rate asked for, looping, and decoded like a
//! clip. Both carry silence on the audio side, so the mixer sees a source
//! with sound like any other.

use super::{assemble, BuildCtx, Ingest, KindParts, Wiring};
use crate::caps::CanvasCaps;
use crate::config::Params;
use crate::gstutil::make;
use crate::input::route_pads;
use crate::plugin::source::{unknown_method, Provide, Source, SourceRequest};
use crate::plugin::{
    Capability, CapabilitySet, Configure, Health, Hello, Manifest, MediaDecl, MediaEnds, PluginState, ProvideKind,
    Ready, StreamMode, Tier, API_LEVEL,
};
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use serde_json::Value;

#[path = "image_chain.rs"]
mod image_chain;
pub use image_chain::{fps, image_type, is_sequence, printf_d};
use image_chain::{sequence, still};

pub const MANIFEST: Manifest = Manifest {
    plugin: "image",
    id: "source",
    kind: ProvideKind::Source,
    api: API_LEVEL,
    description: "A still picture held on screen, or a numbered sequence of pictures played as video",
    uri_schemes: &[],
    rank: 210,
    media: MediaDecl { video: StreamMode::Container, audio: StreamMode::Raw, alpha: false, thumb: true },
    capabilities: CapabilitySet::new().with(Capability::RestartInPlace).with(Capability::Health),
    latency_ms: 0,
    tier: Tier::Core,
};

pub const PROVIDE: Provide = Provide { manifest: MANIFEST, claims, make: new };

fn claims(uri: &str) -> Option<u16> {
    image_type(uri).map(|_| MANIFEST.rank)
}

/// A picture with transparency goes to the overlay board; every other
/// picture, and every numbered sequence, through the compositor as before.
fn new(req: SourceRequest<'_>) -> Result<Box<dyn Source>> {
    let uri = req.cfg.uri.clone();
    if !is_sequence(&uri) && super::image_alpha::wanted(&uri, &req.cfg.effective_params())? {
        return super::rendered::make::<super::image_alpha::AlphaStill>(req);
    }
    Ok(Box::new(ImageSource { ctx: req.ctx(), running: false }))
}

pub struct ImageSource {
    ctx: BuildCtx,
    running: bool,
}

impl Source for ImageSource {
    fn manifest(&self) -> &Manifest {
        &MANIFEST
    }

    fn initialize(&mut self, hello: Hello) -> Result<Ready> {
        fps(&hello.params)?;
        self.ctx.canvas = hello.canvas;
        // A still's frames come from `imagefreeze` and its silence from a live
        // `audiotestsrc`, both stamped with the running time of the clock and
        // base time the mixer gave this pipeline, which is the programme's.
        // Shifted again by the aligner, a picture added a minute into a show
        // sat a minute in the future, the compositor held it until its queue
        // filled, and it was judged stalled with nothing drawn. A sequence's
        // frames start at zero and still want the shift.
        let capabilities = if is_sequence(&self.ctx.cfg.uri) {
            MANIFEST.capabilities
        } else {
            MANIFEST.capabilities.with(Capability::ProgrammeTimeline)
        };
        Ok(Ready { manifest: MANIFEST, latency_ms: 0, capabilities })
    }

    fn start(&mut self, canvas: &CanvasCaps, thumb: bool) -> Result<MediaEnds> {
        self.ctx.canvas = canvas.clone();
        let uri = self.ctx.cfg.uri.clone();
        let p = if is_sequence(&uri) { sequence(&self.ctx, &uri)? } else { still(&self.ctx, &uri)? };
        let silence = make("audiotestsrc", &format!("{}-silence", self.ctx.id))?;
        crate::probe::set_enum(&silence, "wave", "silence");
        silence.set_property("is-live", true);
        crate::probe::set_int(&silence, "samplesperbuffer", i64::from(canvas.sample_rate / 100));
        let id = self.ctx.id.clone();
        let mut els: Vec<gst::Element> = p.before.iter().cloned().chain([p.dynamic.clone()]).chain(p.after.iter().cloned()).collect();
        els.push(silence.clone());
        let ends = assemble(&self.ctx, thumb, Ingest::default().with(els).livesync(false), |w: &Wiring| {
            let mut head = p.before.clone();
            head.push(p.dynamic.clone());
            gst::Element::link_many(&head).context("linking the pictures to their decoder")?;
            gst::Element::link_many(&p.after).context("linking the picture chain")?;
            route_pads(&p.dynamic, &id, p.after.first().cloned(), None, w.has_video, w.has_audio);
            let last = p.after.last().context("no picture element")?;
            last.link(&w.norm.video_entry()).context("linking the picture to the canvas")?;
            silence.link(&w.norm.audio_entry()).context("linking the silence")?;
            w.has_audio.store(true, std::sync::atomic::Ordering::Relaxed);
            Ok(KindParts::default())
        })?;
        self.running = true;
        Ok(ends)
    }

    fn stop(&mut self) -> Result<()> {
        self.running = false;
        Ok(())
    }

    fn configure(&mut self, params: &Params) -> Result<Configure> {
        fps(params)?;
        Ok(Configure::RestartRequired("a picture takes a new file or rate by being opened again".into()))
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

#[cfg(test)]
#[path = "image_tests.rs"]
mod tests;
