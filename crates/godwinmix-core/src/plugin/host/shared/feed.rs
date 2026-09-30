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
use super::reader::{Samples, Tracks};
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::Arc;

/// Readers the owner makes room for. Each show reading a device is one, and
/// a mixer reading its own camera through the bus is one more.
const MAX_READERS: u32 = 16;
/// Frames one reader may hold. A mixer's programme queue, its thumbnail and
/// `videorate` each keep a frame, so three was too few to never wait.
const LEASES: u32 = 8;
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
        tap(&ends, plan, &published)?;
        if plan.tracks.video {
            time_through(&ends, plan, &through)?;
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

/// Take the programme branches off the normaliser's tees and hang a bus sink
/// on each track the source has instead.
fn tap(ends: &MediaEnds, plan: &Plan, published: &Arc<AtomicU64>) -> Result<()> {
    let id = &plan.build.id;
    let pipeline = &ends.pipeline;
    if let Some(queue) = pipeline.by_name(&format!("{id}-vprog-q")) {
        unhook(&ends.vtee, &queue)?;
        pipeline.remove_many([&queue, &ends.video]).context("taking the programme branch off")?;
    }
    unhook(&ends.atee, &ends.audio)?;
    pipeline.remove(&ends.audio).context("taking the programme's sound off")?;
    if plan.tracks.video {
        publish(ends, plan, &ends.vtee, plan.name.clone(), published)?;
    }
    if plan.tracks.audio {
        publish(ends, plan, &ends.atee, plan.name.audio(), published)?;
    }
    Ok(())
}

/// Let go of the tee pad that feeds `el`.
fn unhook(tee: &gst::Element, el: &gst::Element) -> Result<()> {
    if let Some(peer) = el.static_pad("sink").and_then(|p| p.peer()) {
        peer.unlink(&el.static_pad("sink").context("no sink pad")?).ok();
        tee.release_request_pad(&peer);
    }
    Ok(())
}

/// A bus sink for one track, on `tee`.
fn publish(
    ends: &MediaEnds,
    plan: &Plan,
    tee: &gst::Element,
    name: BusName,
    published: &Arc<AtomicU64>,
) -> Result<()> {
    let id = &plan.build.id;
    let pipeline = &ends.pipeline;
    let tag = if matches!(name, BusName::Audio(_)) { "-audio" } else { "" };
    // The normaliser's `videorate` fills gaps by holding each frame until the
    // next one arrives, a frame of delay. Every reader runs its own normaliser
    // for its own canvas and fills its own gaps, so here it only drops, which
    // holds nothing, and the bus adds no frame of delay to anyone.
    if let Some(rate) = pipeline.by_name(&format!("{id}-vrate")) {
        rate.set_property("drop-only", true);
    }
    let sink = crate::gstutil::make("gmxbussink", &format!("{id}-bus-sink{tag}"))?;
    sink.set_property("bus-name", name.to_string());
    sink.set_property("bus-dir", plan.dir.display().to_string());
    sink.set_property("max-readers", MAX_READERS);
    sink.set_property("leases", LEASES);
    // Publish each frame the moment it is here. The readers stamp it on their
    // own clocks; a sink that waited for this pipeline's would only add delay.
    sink.set_property("sync", false);
    sink.set_property("async", false);
    pipeline.add(&sink).context("adding the bus sink")?;
    tee.link(&sink).context("linking the normaliser to the bus sink")?;
    let pad = sink.static_pad("sink").context("gmxbussink has no sink pad")?;
    let published = published.clone();
    pad.add_probe(gst::PadProbeType::BUFFER, move |_, _| {
        published.store(monotonic_ns(), Relaxed);
        gst::PadProbeReturn::Ok
    });
    Ok(())
}

/// Time each frame from the normaliser's entry to the bus sink, matched by
/// its timestamp. Both probes are on the one streaming thread, so the lock is
/// never contended; `try_lock` keeps it that way if that ever changes.
fn time_through(ends: &MediaEnds, plan: &Plan, through: &Arc<Samples>) -> Result<()> {
    let id = &plan.build.id;
    let pipeline = &ends.pipeline;
    let entry = pipeline.by_name(&format!("{id}-vrate")).context("the feed has no normaliser")?;
    let sink = pipeline.by_name(&format!("{id}-bus-sink")).context("the feed has no bus sink")?;
    let seen: Arc<Mutex<VecDeque<(u64, u64)>>> = Arc::default();
    let into = seen.clone();
    entry.static_pad("sink").context("videorate has no sink pad")?.add_probe(
        gst::PadProbeType::BUFFER,
        move |_, info| {
            if let (Some(pts), Some(mut held)) = (info.buffer().and_then(|b| b.pts()), into.try_lock()) {
                if held.len() >= 16 {
                    held.pop_front();
                }
                held.push_back((pts.nseconds(), monotonic_ns()));
            }
            gst::PadProbeReturn::Ok
        },
    );
    let through = through.clone();
    sink.static_pad("sink").context("gmxbussink has no sink pad")?.add_probe(
        gst::PadProbeType::BUFFER,
        move |_, info| {
            if let (Some(pts), Some(held)) = (info.buffer().and_then(|b| b.pts()), seen.try_lock()) {
                if let Some((_, at)) = held.iter().find(|(p, _)| *p == pts.nseconds()) {
                    through.add_ns(monotonic_ns().saturating_sub(*at));
                }
            }
            gst::PadProbeReturn::Ok
        },
    );
    Ok(())
}
