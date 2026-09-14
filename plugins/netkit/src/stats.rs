//! Reading numbers out of a `stats` structure.
//!
//! `srtsrc`, `srtsink` and the webrtc elements all publish a `stats` property
//! holding a `GstStructure`. The field names differ between caller and listener
//! mode, between GStreamer versions and between the C and Rust implementations,
//! and the integer width differs with them. Every reader here tries a list of
//! spellings and takes the first that is present, which is the same trick the
//! core's built in `srt/output` uses.

use gstreamer::glib;
use gstreamer::prelude::*;
use gstreamer::Structure;

/// Read one field as an integer, whatever width it was stored at.
pub fn number(s: &gstreamer::StructureRef, field: &str) -> Option<i64> {
    if let Ok(v) = s.get::<i64>(field) {
        return Some(v);
    }
    if let Ok(v) = s.get::<u64>(field) {
        return Some(v as i64);
    }
    if let Ok(v) = s.get::<i32>(field) {
        return Some(v as i64);
    }
    if let Ok(v) = s.get::<u32>(field) {
        return Some(v as i64);
    }
    None
}

/// Read one field as a float, accepting an integer where a float was expected.
pub fn float(s: &gstreamer::StructureRef, field: &str) -> Option<f64> {
    if let Ok(v) = s.get::<f64>(field) {
        return Some(v);
    }
    if let Ok(v) = s.get::<f32>(field) {
        return Some(v as f64);
    }
    number(s, field).map(|v| v as f64)
}

/// The first of `fields` that is present, as an integer.
pub fn first_number(s: &gstreamer::StructureRef, fields: &[&str]) -> Option<i64> {
    fields.iter().find_map(|f| number(s, f))
}

/// The first of `fields` that is present, as a float.
pub fn first_float(s: &gstreamer::StructureRef, fields: &[&str]) -> Option<f64> {
    fields.iter().find_map(|f| float(s, f))
}

/// The `stats` property of an element, if it has one and it is set.
pub fn of(element: &gstreamer::Element) -> Option<Structure> {
    element.find_property("stats")?;
    element.property::<Option<Structure>>("stats")
}

/// SRT puts a listener's numbers one level down, one structure per caller.
///
/// In caller mode the numbers are at the top level; in listener mode they are
/// in a `callers` list. This returns the structure the numbers are actually in,
/// which is the top level one unless a caller has them.
pub fn srt_level(top: &Structure, probe: &[&str]) -> Option<Structure> {
    if first_number(top.as_ref(), probe).is_some() {
        return Some(top.clone());
    }
    let callers = top.get::<glib::ValueArray>("callers").ok();
    if let Some(list) = callers {
        for value in list.iter() {
            if let Ok(inner) = value.get::<Structure>() {
                if first_number(inner.as_ref(), probe).is_some() {
                    return Some(inner);
                }
            }
        }
    }
    if let Ok(list) = top.get::<gstreamer::List>("callers") {
        for value in list.iter() {
            if let Ok(inner) = value.get::<Structure>() {
                if first_number(inner.as_ref(), probe).is_some() {
                    return Some(inner);
                }
            }
        }
    }
    None
}

/// The field spellings for the four SRT numbers a person actually wants.
pub mod srt {
    /// Round trip time in milliseconds.
    pub const RTT_MS: &[&str] = &["rtt-ms", "rtt_ms", "msRTT"];
    /// Packets lost on the link.
    pub const LOST: &[&str] = &[
        "packets-received-lost",
        "packets-sent-lost",
        "pktRcvLoss",
        "pktSndLoss",
        "packet-loss",
    ];
    /// Packets the sender had to send again.
    pub const RETRANSMITTED: &[&str] = &[
        "packets-retransmitted",
        "packets-received-retransmitted",
        "pktRetrans",
        "pktRcvRetrans",
    ];
    /// Anything that proves bytes are moving, for a liveness answer.
    pub const MOVING: &[&str] = &[
        "packets-received",
        "bytes-received",
        "bytes-received-total",
        "packets-sent",
        "bytes-sent",
        "bytes-sent-total",
    ];
    /// The negotiated latency, which is the larger of the two peers' settings.
    pub const NEGOTIATED_LATENCY_MS: &[&str] = &["negotiated-latency-ms", "msRcvBuf"];
    /// Estimated link bandwidth, megabits per second.
    pub const BANDWIDTH_MBPS: &[&str] = &["bandwidth-mbps", "mbpsBandwidth"];
}

/// The SRT numbers a `health` answer carries, as JSON, with the fields that
/// this build does not publish left out rather than reported as zero.
pub fn srt_health_detail(element: &gstreamer::Element) -> Option<serde_json::Value> {
    let top = of(element)?;
    let level = srt_level(&top, srt::MOVING).unwrap_or(top);
    let mut out = serde_json::Map::new();
    if let Some(v) = first_float(level.as_ref(), srt::RTT_MS) {
        out.insert("rtt_ms".into(), round1(v).into());
    }
    if let Some(v) = first_number(level.as_ref(), srt::LOST) {
        out.insert("packets_lost".into(), v.into());
    }
    if let Some(v) = first_number(level.as_ref(), srt::RETRANSMITTED) {
        out.insert("packets_retransmitted".into(), v.into());
    }
    if let Some(v) = first_float(level.as_ref(), srt::BANDWIDTH_MBPS) {
        out.insert("bandwidth_mbps".into(), round1(v).into());
    }
    if let Some(v) = first_number(level.as_ref(), srt::NEGOTIATED_LATENCY_MS) {
        out.insert("negotiated_latency_ms".into(), v.into());
    }
    if out.is_empty() {
        return None;
    }
    Some(serde_json::Value::Object(out))
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn structure() -> Structure {
        crate::init().expect("gstreamer");
        Structure::builder("application/x-srt-statistics")
            .field("rtt-ms", 12.5f64)
            .field("packets-received", 900i64)
            .field("packets-received-lost", 3i32)
            .build()
    }

    #[test]
    fn a_number_is_read_at_any_width() {
        let s = structure();
        assert_eq!(number(s.as_ref(), "packets-received"), Some(900));
        assert_eq!(number(s.as_ref(), "packets-received-lost"), Some(3));
        assert_eq!(number(s.as_ref(), "nothing"), None);
    }

    #[test]
    fn a_float_field_reads_as_a_float_and_an_int_field_does_too() {
        let s = structure();
        assert_eq!(float(s.as_ref(), "rtt-ms"), Some(12.5));
        assert_eq!(float(s.as_ref(), "packets-received"), Some(900.0));
    }

    #[test]
    fn the_first_present_spelling_wins() {
        let s = structure();
        assert_eq!(first_float(s.as_ref(), srt::RTT_MS), Some(12.5));
        assert_eq!(first_number(s.as_ref(), srt::MOVING), Some(900));
        assert_eq!(first_number(s.as_ref(), &["nope", "also-nope"]), None);
    }

    #[test]
    fn a_listener_structure_is_followed_into_its_callers() {
        crate::init().expect("gstreamer");
        let caller = structure();
        let top = Structure::builder("application/x-srt-statistics")
            .field("callers", gstreamer::List::new([caller.to_send_value()]))
            .build();
        let found = srt_level(&top, srt::MOVING).expect("the caller carries the numbers");
        assert_eq!(first_number(found.as_ref(), srt::MOVING), Some(900));
    }

    #[test]
    fn a_structure_with_nothing_known_yields_no_detail() {
        crate::init().expect("gstreamer");
        let top = Structure::builder("application/x-srt-statistics").build();
        assert!(srt_level(&top, srt::MOVING).is_none());
    }
}
