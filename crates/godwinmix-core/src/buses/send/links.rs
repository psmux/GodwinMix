//! One source on one bus: a pad on the source's fan and one on the bus's
//! mixer, kept at the source's weight.

use super::{Link, SendPoint, BLOCK_TIMEOUT};
use crate::buses::weight::{self, CueMarks, Programme, Shared};
use crate::gstutil;
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;

impl SendPoint {
    /// Put this source on `bus`.
    pub fn join(&mut self, bus: &str, mix: &gst::Element, shared: Shared, cue: CueMarks) -> Result<()> {
        if self.links.iter().any(|l| l.bus == bus) {
            return Ok(());
        }
        let sink = mix.request_pad_simple("sink_%u").context("the bus mixer refused a pad")?;
        sink.set_property("volume", 0.0f64);
        sink.set_offset(self.apad.offset());
        let pad = self.fan.request_pad_simple("src_%u").context("the send fan refused a pad")?;
        let programme = Programme { source: self.source.clone(), amute: self.amute.clone(), apad: self.apad.clone() };
        weight::follow(&pad, &sink, shared, cue, programme);
        if let Err(e) = pad.link(&sink) {
            self.fan.release_request_pad(&pad);
            mix.release_request_pad(&sink);
            return Err(anyhow::Error::from(e).context(format!("linking {} to bus {bus}", self.source)));
        }
        self.links.push(Link { bus: bus.to_string(), pad, sink, mix: mix.clone() });
        Ok(())
    }

    /// Take this source off `bus`. Unlinked at an idle point before either pad
    /// is released: a mixer pad released under a buffer answers FLUSHING, and
    /// that must never reach the queue, whose upstream is the programme.
    pub fn leave(&mut self, bus: &str) {
        let Some(pos) = self.links.iter().position(|l| l.bus == bus) else { return };
        let link = self.links.remove(pos);
        let (pad, sink) = (link.pad.clone(), link.sink.clone());
        if let Err(e) = gstutil::with_pad_blocked(&link.pad, BLOCK_TIMEOUT, move || {
            let _ = pad.unlink(&sink);
        }) {
            tracing::warn!(source = %self.source, bus, ?e, "a bus link would not go idle; unlinking it anyway");
            let _ = link.pad.unlink(&link.sink);
        }
        self.fan.release_request_pad(&link.pad);
        link.mix.release_request_pad(&link.sink);
    }

    pub fn buses(&self) -> impl Iterator<Item = &str> {
        self.links.iter().map(|l| l.bus.as_str())
    }
}
