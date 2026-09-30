//! The pipeline surgery that turns a sidecar source's pipeline into a feed:
//! the branches to the programme come off the normaliser's tees, a bus sink
//! goes on each, and two probes time what publishing costs.

use super::feed::Plan;
use super::reader::Samples;
use crate::plugin::MediaEnds;
use anyhow::{Context, Result};
use godwinmix_framebus::{monotonic_ns, BusName};
use gstreamer as gst;
use gstreamer::prelude::*;
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

/// Take the programme branches off the normaliser's tees and hang a bus sink
/// on each track the source has instead.
pub(super) fn tap(ends: &MediaEnds, plan: &Plan, published: &Arc<AtomicU64>) -> Result<()> {
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
pub(super) fn time_through(ends: &MediaEnds, plan: &Plan, through: &Arc<Samples>) -> Result<()> {
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
