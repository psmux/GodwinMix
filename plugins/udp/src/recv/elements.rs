//! The three elements of the receive pipeline, and the probe on the first.

use std::sync::Mutex;

use gstreamer as gst;
use gstreamer::prelude::*;

use super::probe::{Probe, Verdict};
use super::settings::Settings;
use super::Sink;

pub fn udpsrc(s: &Settings) -> Result<gst::Element, String> {
    let e = s.endpoint()?;
    let src = make("udpsrc", "src")?;
    src.set_property("address", &e.host);
    src.set_property("port", i32::from(e.port));
    // Two receivers of one group on one machine is normal; two sources fighting
    // over one unicast port is a mistake, and refusing the second says so.
    src.set_property("reuse", e.multicast());
    src.set_property("auto-multicast", true);
    src.set_property("buffer-size", (s.receive_buffer_kb * 1024) as i32);
    src.set_property("retrieve-sender-address", false);
    if !s.interface.is_empty() {
        src.set_property("multicast-iface", crate::iface::name_of(&s.interface));
    }
    if let Some(source) = &e.source {
        src.set_property("multicast-source", format!("+{source}"));
    }
    let caps = gst::Caps::builder("video/mpegts").field("systemstream", true).field("packetsize", 188).build();
    src.set_property("caps", &caps);
    Ok(src)
}

pub fn attach_probe(src: &gst::Element, probe: Probe) -> Result<(), String> {
    let pad = src.static_pad("src").ok_or("udpsrc has no src pad")?;
    let probe = Mutex::new(probe);
    pad.add_probe(gst::PadProbeType::BUFFER, move |_, info| {
        let Some(gst::PadProbeData::Buffer(buffer)) = info.data.as_mut() else {
            return gst::PadProbeReturn::Ok;
        };
        let verdict = match buffer.map_readable() {
            Ok(map) => probe.lock().unwrap_or_else(|e| e.into_inner()).datagram(map.as_slice()),
            Err(_) => return gst::PadProbeReturn::Ok,
        };
        match verdict {
            Verdict::Keep => gst::PadProbeReturn::Ok,
            Verdict::Drop => gst::PadProbeReturn::Drop,
            Verdict::Replace(bytes) => {
                let mut fresh = gst::Buffer::from_mut_slice(bytes);
                if let Some(b) = fresh.get_mut() {
                    b.set_pts(buffer.pts());
                    b.set_dts(buffer.dts());
                }
                *buffer = fresh;
                gst::PadProbeReturn::Ok
            }
        }
    });
    Ok(())
}

pub fn sink_for(sink: &Sink) -> Result<gst::Element, String> {
    let out = match sink {
        Sink::Stdout => {
            let out = make("fdsink", "out")?;
            out.set_property("fd", 1i32);
            out
        }
        Sink::File(path) => {
            let out = make("filesink", "out")?;
            out.set_property("location", path.to_string_lossy().to_string());
            // Unbuffered, so a test reading the file sees each datagram as it lands.
            out.set_property_from_str("buffer-mode", "unbuffered");
            out
        }
    };
    out.set_property("sync", false);
    out.set_property("async", false);
    Ok(out)
}

pub fn with_interface(address: &str, iface: &str) -> String {
    if iface.is_empty() { address.to_string() } else { format!("{address} on {iface}") }
}

pub fn make(factory: &str, name: &str) -> Result<gst::Element, String> {
    let b = gst::ElementFactory::make(factory);
    let b = if name.is_empty() { b } else { b.name(name) };
    b.build().map_err(|e| format!("could not make '{factory}': {e}"))
}
