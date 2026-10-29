//! A bus played on a sound card here: headphones, a monitor speaker, a line
//! out to another room.
//!
//! The device is found through GStreamer's `DeviceMonitor`, the same way the
//! `audio-device` plugin finds microphones, so the platform's own element is
//! chosen and pointed at the device without this file knowing whether that is
//! `osxaudiosink`, `wasapi2sink`, `pulsesink` or `alsasink`. `default` skips
//! the monitor and takes `autoaudiosink`, which is the system's choice.
//!
//! The sink runs in a pipeline of its own, fed across a proxy from a leaky
//! queue on the bus's tee:
//!
//! ```text
//!  programme:  bus-{id}-tee -> bus-{id}-dev-q -> bus-{id}-dev-proxy
//!  its own:    proxysrc -> audioconvert -> audioresample -> the device's sink
//! ```
//!
//! An audio sink can offer a clock and has to preroll, and neither of those
//! may touch the programme: a sound card unplugged, or one that takes a second
//! to open, costs the headphones and nothing else. It does not sync to the
//! clock: the bus already delivers in real time, and the card plays it as it
//! comes.

use crate::gstutil::{self, make};
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use parking_lot::Mutex;
use std::sync::Arc;

mod find;
pub use find::{list, sink_for};

/// A bus playing on a device.
pub struct DevicePlayer {
    tee: gst::Element,
    pad: gst::Pad,
    near: Vec<gst::Element>,
    far: gst::Pipeline,
    programme: gst::Pipeline,
    /// The last error the device's pipeline posted, for the status.
    pub fault: Arc<Mutex<Option<String>>>,
}

impl DevicePlayer {
    pub fn start(programme: &gst::Pipeline, tee: &gst::Element, bus: &str, sink: gst::Element) -> Result<Self> {
        let q = gstutil::queue_preview_holding(&format!("bus-{bus}-dev-q"), 0.5)?;
        let proxy = make("proxysink", &format!("bus-{bus}-dev-proxy"))?;
        let far = gst::Pipeline::with_name(&format!("bus-{bus}-device"));
        let src = make("proxysrc", "bus-device-src")?;
        src.set_property("proxysink", &proxy);
        let chain = [src, make("audioconvert", "bus-device-conv")?, make("audioresample", "bus-device-res")?, sink.clone()];
        if sink.find_property("sync").is_some() {
            sink.set_property("sync", false);
        }
        far.add_many(&chain).context("building a device pipeline")?;
        gst::Element::link_many(&chain).context("linking a device pipeline")?;
        let fault = Arc::new(Mutex::new(None));
        watch_faults(&far, fault.clone());
        far.set_state(gst::State::Playing).context("starting the sound output")?;

        programme.add_many([&q, &proxy]).context("adding a device feed")?;
        q.link(&proxy).context("linking a device feed")?;
        proxy.sync_state_with_parent().ok();
        q.sync_state_with_parent().ok();
        let pad = tee.request_pad_simple("src_%u").context("the bus tee refused a pad")?;
        pad.link(&q.static_pad("sink").context("a queue with no sink")?).context("linking a device feed")?;
        Ok(Self { tee: tee.clone(), pad, near: vec![q, proxy], far, programme: programme.clone(), fault })
    }

    pub fn stop(self) {
        if let Some(peer) = self.pad.peer() {
            let _ = self.pad.unlink(&peer);
        }
        self.tee.release_request_pad(&self.pad);
        for el in &self.near {
            el.set_locked_state(true);
            let _ = el.set_state(gst::State::Null);
            let _ = self.programme.remove(el);
        }
        let _ = self.far.set_state(gst::State::Null);
    }
}

/// Keep the device pipeline's last error, from its own bus, on the thread that
/// posts it. A string copied into a mutex and nothing else.
fn watch_faults(pipeline: &gst::Pipeline, fault: Arc<Mutex<Option<String>>>) {
    let Some(bus) = pipeline.bus() else { return };
    bus.set_sync_handler(move |_, msg| {
        if let gst::MessageView::Error(e) = msg.view() {
            *fault.lock() = Some(e.error().to_string());
        }
        gst::BusSyncReply::Drop
    });
}
