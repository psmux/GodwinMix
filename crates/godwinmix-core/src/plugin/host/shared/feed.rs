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
use crate::plugin::{Hello, MediaEnds, Tier};
use anyhow::{Context, Result};
use godwinmix_framebus::{monotonic_ns, BusName, Claim};
use godwinmix_protocol::plugin::wire::InstanceState;
use gstreamer as gst;
use gstreamer::prelude::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::Arc;

/// Readers the owner makes room for. Each show reading a device is one, and
/// a mixer reading its own camera through the bus is one more.
const MAX_READERS: u32 = 16;
/// Frames one reader may hold. A mixer's programme queue, its thumbnail and
/// `videorate` each keep a frame, so three was too few to never wait.
const LEASES: u32 = 8;
/// A device that has said nothing for this long is reopened.
const SILENT_NS: u64 = 3_000_000_000;

/// What a feed needs to start, kept by the source across handovers.
#[derive(Clone)]
pub struct Plan {
    pub type_id: String,
    pub name: BusName,
    pub dir: PathBuf,
    pub build: BuildCtx,
}

pub struct Feed {
    id: String,
    sidecar: SidecarSource,
    pipeline: gst::Pipeline,
    published: Arc<AtomicU64>,
    started_ns: u64,
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
        tap(&ends, plan, &published)?;
        let pipeline = ends.pipeline.clone();
        if let Err(e) = pipeline.set_state(gst::State::Playing) {
            let _ = pipeline.set_state(gst::State::Null);
            let _ = sidecar.stop();
            return Err(anyhow::anyhow!("the feed for {} would not play: {e}", plan.name));
        }
        Ok(Feed { id: plan.build.id.clone(), sidecar, pipeline, published, started_ns: monotonic_ns(), _claim: claim })
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
        let quiet_since = self.published.load(Relaxed).max(self.started_ns);
        (monotonic_ns().saturating_sub(quiet_since) > SILENT_NS)
            .then(|| "the device sent no picture for 3 s".to_string())
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
        let _ = self.pipeline.set_state(gst::State::Null);
        let _ = self.sidecar.stop();
    }
}

/// Take the programme branch off the normaliser's tee and hang the bus sink
/// there instead.
fn tap(ends: &MediaEnds, plan: &Plan, published: &Arc<AtomicU64>) -> Result<()> {
    let id = &plan.build.id;
    let pipeline = &ends.pipeline;
    if let Some(queue) = pipeline.by_name(&format!("{id}-vprog-q")) {
        if let Some(peer) = queue.static_pad("sink").and_then(|p| p.peer()) {
            ends.vtee.release_request_pad(&peer);
        }
        pipeline.remove_many([&queue, &ends.video]).context("taking the programme branch off")?;
    }
    let sink = crate::gstutil::make("gmxbussink", &format!("{id}-bus-sink"))?;
    sink.set_property("bus-name", plan.name.to_string());
    sink.set_property("bus-dir", plan.dir.display().to_string());
    sink.set_property("max-readers", MAX_READERS);
    sink.set_property("leases", LEASES);
    // Publish each frame the moment it is here. The readers stamp it on their
    // own clocks; a sink that waited for this pipeline's would only add delay.
    sink.set_property("sync", false);
    sink.set_property("async", false);
    pipeline.add(&sink).context("adding the bus sink")?;
    ends.vtee.link(&sink).context("linking the normaliser to the bus sink")?;
    let pad = sink.static_pad("sink").context("gmxbussink has no sink pad")?;
    let published = published.clone();
    pad.add_probe(gst::PadProbeType::BUFFER, move |_, _| {
        published.store(monotonic_ns(), Relaxed);
        gst::PadProbeReturn::Ok
    });
    Ok(())
}
