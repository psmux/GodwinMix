//! Where the programme hands its encoded stream to a destination, and what is
//! not allowed across.
//!
//! An output's feed is a leaky queue in the programme pipeline, so a slow
//! destination loses buffers and never holds the encoder up. That covers
//! buffers. It does not cover serialized queries: a queue holds one of those
//! until everything queued ahead of it has been pushed, however leaky it is.
//! Its thread pushes into `proxysink`, and on the far side is the output's own
//! pipeline, which can sit still for as long as its sink likes. `rtmp2sink`
//! waiting for a server that accepted the connection and never answered is one.
//!
//! Caught on 2026-10-01 running the example config with another core already
//! on port 1935. The primary output's sink waited on a handshake that never
//! came, its queues filled, and an allocation query from the video encoder's
//! renegotiation went into the feed queue and stayed there. The encoder waits
//! for that answer holding its stream lock, the compositor waits for the
//! encoder, and the programme stopped. Every source then read as stalled, and
//! the stall restart of `cam-pulpit` sat for minutes inside a flush that the
//! stopped compositor could not let through, holding the mixer's command loop
//! the whole time.
//!
//! Nothing downstream of the feed has a buffer pool worth offering an encoder:
//! it is a muxer and a network or file sink. So the allocation query, and the
//! drain query an encoder sends when it is flushed, are answered here, before
//! they reach the queue, with nothing on offer. An encoder copes with that
//! exactly as it copes with a peer that has no opinion.

use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;

/// Answer the serialized queries that would otherwise wait on the destination,
/// at the sink pad of the queue that feeds it.
pub fn answer_serialized_queries(feed: &gst::Element) -> Result<()> {
    let pad = feed
        .static_pad("sink")
        .with_context(|| format!("{} has no sink pad", feed.name()))?;
    pad.add_probe(gst::PadProbeType::QUERY_DOWNSTREAM, |_pad, info| {
        let Some(query) = info.query_mut() else {
            return gst::PadProbeReturn::Ok;
        };
        match query.view_mut() {
            gst::QueryViewMut::Allocation(_) | gst::QueryViewMut::Drain(_) => {
                gst::PadProbeReturn::Handled
            }
            _ => gst::PadProbeReturn::Ok,
        }
    })
    .context("installing the query answer on an output feed")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// A feed queue whose far end never reads, with a buffer stuck in its
    /// thread, and an allocation query sent into it. Answered at once with the
    /// probe; without it the query would wait for as long as the far end does.
    #[test]
    fn a_stuck_destination_cannot_hold_an_allocation_query() {
        let _ = gst::init();
        let pipeline = gst::Pipeline::new();
        let src = gst::ElementFactory::make("videotestsrc").property("is-live", true).build().unwrap();
        let feed = crate::gstutil::queue_time("feed", 1.0, true).unwrap();
        let sink = gst::ElementFactory::make("fakesink").property("async", false).build().unwrap();
        pipeline.add_many([&src, &feed, &sink]).unwrap();
        gst::Element::link_many([&src, &feed, &sink]).unwrap();
        // The destination that never reads: every buffer parks in the probe.
        let (parked_tx, parked_rx) = std::sync::mpsc::channel::<()>();
        let parked_tx = std::sync::Mutex::new(parked_tx);
        sink.static_pad("sink").unwrap().add_probe(gst::PadProbeType::BUFFER, move |_, _| {
            let _ = parked_tx.lock().unwrap().send(());
            std::thread::sleep(Duration::from_secs(3600));
            gst::PadProbeReturn::Ok
        });
        answer_serialized_queries(&feed).unwrap();
        pipeline.set_state(gst::State::Playing).unwrap();
        parked_rx.recv_timeout(Duration::from_secs(5)).expect("a buffer reached the stuck sink");

        let pad = feed.static_pad("sink").unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let caps = gst::Caps::builder("video/x-raw").build();
            let mut q = gst::query::Allocation::new(Some(&caps), true);
            let _ = tx.send(pad.query(&mut q));
        });
        let answered = rx.recv_timeout(Duration::from_millis(500));
        // The parked streaming thread never comes back, so the pipeline is
        // left as it is rather than taken to NULL, which would join it.
        std::mem::forget(pipeline);
        assert!(answered.is_ok(), "the allocation query waited on the stuck destination");
    }
}
