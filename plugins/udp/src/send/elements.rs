//! The muxer, the socket, and what the output's health says.

use godwinmix_sdk::wire::Health;
use gstreamer as gst;
use gstreamer::prelude::*;

use super::settings::Settings;
use crate::recv::make;

pub fn judge(what: &str, failure: Option<String>, ended: bool, bytes: u64) -> Health {
    if let Some(f) = failure {
        return Health::failing(format!("sending to {what} failed: {f}"));
    }
    if ended {
        return Health::degraded("the core stopped handing over the programme. It starts again when the output is reconnected.");
    }
    if bytes == 0 {
        return Health::degraded(format!(
            "nothing sent to {what} in the last second: the programme has not arrived from the core yet."
        ));
    }
    let mut h = Health::ok();
    h.detail = Some(format!("{:.1} Mbit/s to {what}", bytes as f64 * 8.0 / 1e6));
    h
}

pub fn muxer(s: &Settings) -> Result<gst::Element, String> {
    let mux = make("mpegtsmux", "mux")?;
    mux.set_property("alignment", s.packets_per_datagram as i32);
    if s.cbr_kbps > 0 {
        mux.set_property("bitrate", u64::from(s.cbr_kbps) * 1000);
    }
    Ok(mux)
}

/// The payloader when the address is rtp://, and the socket.
pub fn tail(s: &Settings) -> Result<Vec<gst::Element>, String> {
    let e = s.endpoint()?;
    let mut out = Vec::new();
    if s.rtp() {
        let pay = make("rtpmp2tpay", "pay")?;
        pay.set_property("mtu", 12 + 188 * s.packets_per_datagram);
        pay.set_property("pt", 33u32);
        out.push(pay);
    }
    let sink = make("udpsink", "out")?;
    sink.set_property("host", &e.host);
    sink.set_property("port", i32::from(e.port));
    sink.set_property("auto-multicast", false);
    sink.set_property("ttl", s.ttl as i32);
    sink.set_property("ttl-mc", s.ttl as i32);
    sink.set_property("sync", false);
    sink.set_property("async", false);
    if !s.interface.is_empty() {
        sink.set_property("multicast-iface", &s.interface);
    }
    if s.dscp >= 0 {
        sink.set_property("qos-dscp", s.dscp);
    }
    if s.cbr_kbps > 0 {
        // A ceiling a hair over the constant rate, so a frame leaves spread
        // over its interval rather than in one burst. The margin covers the RTP
        // header and keeps the sink from ever falling behind the programme.
        sink.set_property("max-bitrate", u64::from(s.cbr_kbps) * 1030);
    }
    out.push(sink);
    Ok(out)
}
