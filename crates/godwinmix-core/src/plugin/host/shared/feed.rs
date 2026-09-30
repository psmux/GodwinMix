//! The owner's half: the plugin process that opens the device, and a small
//! pipeline of its own that publishes what it sends on the bus.
//!
//! The pipeline is the one every sidecar source builds (the plugin's socket,
//! then the normaliser), with the branch to the programme taken off and
//! `gmxbussink` put on the tee in its place. Nothing here is in a mixer's
//! pipeline, so a feed that stalls or fails is one reader's gap, never the
//! programme's, and its failure is noticed by polling, not on a bus handler.

use super::super::source::SidecarSource;
use crate::plugin::kinds::BuildCtx;
use crate::plugin::source::Source;
use crate::plugin::{Hello, Tier};
use anyhow::Result;
use godwinmix_framebus::{monotonic_ns, BusName, Claim};
use godwinmix_protocol::plugin::wire::InstanceState;
use gstreamer as gst;
use gstreamer::prelude::*;
use std::path::PathBuf;
use super::reader::{Samples, Tracks};
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::Arc;

/// A device that sent pictures and then nothing for this long is reopened.
const SILENT_NS: u64 = 3_000_000_000;

/// What a feed needs to start, kept by the source across handovers.
#[derive(Clone)]
pub struct Plan {
    pub type_id: String,
    pub name: BusName,
    pub dir: PathBuf,
    pub build: BuildCtx,
    pub tracks: Tracks,
}

pub struct Feed {
    id: String,
    /// Whether silence means the device failed. A camera that stops is
    /// broken; a channel with nobody publishing to it is only waiting.
    silence_is_failure: bool,
    sidecar: SidecarSource,
    pipeline: gst::Pipeline,
    published: Arc<AtomicU64>,
    /// From a frame reaching this process from the plugin to its publish.
    pub through: Arc<Samples>,
    /// Held for as long as the feed lives, and dropped after everything else.
    _claim: Claim,
}

impl Feed {
    /// Open the device and start publishing. Blocks for as long as the plugin
    /// takes to say hello and start, which is why only the owner thread calls it.
    pub fn start(claim: Claim, plan: &Plan) -> Result<Feed> {
        super::register_elements()?;
        let mut sidecar = super::super::sidecar_for(&plan.type_id, plan.build.clone())?;
        sidecar.initialize(Hello {
            instance: plan.build.id.clone(),
            canvas: plan.build.canvas.clone(),
            api_level: crate::plugin::API_LEVEL,
            params: plan.build.cfg.effective_params(),
            tier: Tier::Sidecar,
        })?;
        let ends = sidecar.start(&plan.build.canvas, false)?;
        let published = Arc::new(AtomicU64::new(0));
        let through = Arc::new(Samples::default());
        super::tap::tap(&ends, plan, &published)?;
        if plan.tracks.video {
            super::tap::time_through(&ends, plan, &through)?;
        }
        let pipeline = ends.pipeline.clone();
        if let Err(e) = pipeline.set_state(gst::State::Playing) {
            let _ = pipeline.set_state(gst::State::Null);
            let _ = sidecar.stop();
            return Err(anyhow::anyhow!("the feed for {} would not play: {e}", plan.name));
        }
        Ok(Feed {
            id: plan.build.id.clone(),
            silence_is_failure: matches!(plan.name, BusName::Camera(_)),
            sidecar,
            pipeline,
            published,
            through,
            _claim: claim,
        })
    }

    /// Why this feed should be given up, if it should. Polled by the owner
    /// thread; reads the pipeline's bus without waiting.
    pub fn failure(&mut self) -> Option<String> {
        for notice in self.sidecar.notices() {
            match notice {
                super::super::Notice::Broken(why) => {
                    return Some(format!("the plugin's channel broke: {why}"));
                }
                super::super::Notice::Log { message, .. } => {
                    tracing::info!(source = %self.id, "{message}");
                }
                _ => {}
            }
        }
        if matches!(self.sidecar.instance_state(), InstanceState::Failed | InstanceState::Stopped) {
            return Some("the plugin process stopped".into());
        }
        if let Some(bus) = self.pipeline.bus() {
            if let Some(msg) = bus.pop_filtered(&[gst::MessageType::Error]) {
                if let gst::MessageView::Error(e) = msg.view() {
                    return Some(format!("the feed pipeline failed: {}", e.error()));
                }
            }
        }
        // Only once it has sent something: a camera that takes its time to wake
        // is starting, not failing, and the plugin's own handshake and health
        // say when it will not start at all.
        let last = self.published.load(Relaxed);
        let silent = last != 0 && monotonic_ns().saturating_sub(last) > SILENT_NS;
        (silent && self.silence_is_failure).then(|| "the device sent nothing for 3 s".to_string())
    }

    pub fn sidecar(&mut self) -> &mut SidecarSource {
        &mut self.sidecar
    }

    pub fn pid(&self) -> Option<u32> {
        self.sidecar.pid()
    }
}

impl Drop for Feed {
    fn drop(&mut self) {
        tracing::info!(source = %self.id, publish_ms = %self.through.report(), "closing the shared device");
        let _ = self.pipeline.set_state(gst::State::Null);
        let _ = self.sidecar.stop();
    }
}
