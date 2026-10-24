//! Branches on and off a tee while the pipeline plays.

use std::time::Duration;

use gstreamer as gst;
use gstreamer::prelude::*;

/// Hang a branch off a tee: a new pad of the tee, linked to the branch's head.
pub fn hang(tee: &gst::Element, head: &gst::Element) -> Result<gst::Pad, String> {
    let pad = tee.request_pad_simple("src_%u").ok_or("the tee gave no pad")?;
    let sink = head.static_pad("sink").ok_or("the branch has no sink pad")?;
    if let Err(e) = pad.link(&sink) {
        tee.release_request_pad(&pad);
        return Err(format!("{e:?}"));
    }
    Ok(pad)
}

/// Unlink a branch from its tee once no buffer is on the way through, and
/// wait a moment for that; a tee with nothing flowing is idle at once.
pub fn unlink_when_idle(pad: &gst::Pad, head: &gst::Element) {
    let Some(sink) = head.static_pad("sink") else { return };
    let (tx, rx) = std::sync::mpsc::sync_channel::<()>(1);
    pad.add_probe(gst::PadProbeType::IDLE, move |pad, _| {
        let _ = pad.unlink(&sink);
        let _ = tx.try_send(());
        gst::PadProbeReturn::Remove
    });
    let _ = rx.recv_timeout(Duration::from_millis(500));
}
