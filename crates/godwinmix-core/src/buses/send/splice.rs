//! Moving a send point's tee into a branch and out of it again, at an idle
//! point of the pad above, and taking its own elements down.

use super::{SendPoint, BLOCK_TIMEOUT};
use crate::gstutil;
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;

impl SendPoint {
    /// Move the link above the mute onto the tee, at an idle point.
    pub(super) fn splice_in(&self) -> Result<()> {
        let mute_in = self.amute.static_pad("sink").context("the mute has no sink pad")?;
        let above = mute_in.peer().context("nothing is linked above the mute")?;
        let tee_in = self.tee.static_pad("sink").context("a tee with no sink")?;
        let (a, to_p) = (above.clone(), self.to_programme.clone());
        gstutil::with_pad_blocked(&above, BLOCK_TIMEOUT, move || {
            let _ = a.unlink(&mute_in);
            let linked = a.link(&tee_in).map_err(|e| e.to_string()).and_then(|_| to_p.link(&mute_in).map_err(|e| e.to_string()));
            if let Err(e) = linked {
                tracing::error!(%e, "could not put a send point in; putting the mute back");
                let _ = a.unlink(&tee_in);
                let _ = to_p.unlink(&mute_in);
                let _ = a.link(&mute_in);
            }
        })
        .context("putting a send point in while blocked")?;
        anyhow::ensure!(self.to_programme.is_linked(), "the send point did not go in; the source is as it was");
        Ok(())
    }

    /// Put the mute back under the meter, at an idle point.
    pub(super) fn splice_out(&self) -> Result<()> {
        let tee_in = self.tee.static_pad("sink").context("a tee with no sink")?;
        let mute_in = self.amute.static_pad("sink").context("the mute has no sink pad")?;
        let Some(above) = tee_in.peer() else { return Ok(()) };
        let (a, to_p) = (above.clone(), self.to_programme.clone());
        gstutil::with_pad_blocked(&above, BLOCK_TIMEOUT, move || {
            let _ = a.unlink(&tee_in);
            let _ = to_p.unlink(&mute_in);
            if let Err(e) = a.link(&mute_in) {
                tracing::error!(?e, "could not relink a source's mute after its send point");
            }
        })
        .context("taking a send point out while blocked")
    }

    /// The tee, queue and fan, unlinked from everything, down and out.
    pub(super) fn drop_own(&self) {
        if let Some(q) = self.queue.static_pad("sink") {
            let _ = self.to_buses.unlink(&q);
        }
        self.tee.release_request_pad(&self.to_buses);
        self.tee.release_request_pad(&self.to_programme);
        drop_elements(&self.pipeline, &[&self.tee, &self.queue, &self.fan]);
    }
}

pub(super) fn drop_elements(pipeline: &gst::Pipeline, els: &[&gst::Element]) {
    for el in els {
        el.set_locked_state(true);
        let _ = el.set_state(gst::State::Null);
        let _ = pipeline.remove(*el);
    }
}
