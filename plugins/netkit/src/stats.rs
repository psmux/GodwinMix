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

/// The first element inside `bin`, at any depth, that has this property.
///
/// The webrtc elements are bins, and the thing that knows the ICE state is the
/// `webrtcbin` several levels down. Walking for the property rather than for a
/// factory name means a renamed or re-wrapped element still answers.
pub fn find_with_property(
    bin: &gstreamer::Element,
    property: &str,
) -> Option<gstreamer::Element> {
    if bin.find_property(property).is_some() {
        return Some(bin.clone());
    }
    let bin = bin.clone().downcast::<gstreamer::Bin>().ok()?;
    let mut iter = bin.iterate_recurse();
    while let Ok(Some(element)) = iter.next() {
        if element.find_property(property).is_some() {
            return Some(element);
        }
    }
    None
}

/// An enum property as the name GStreamer prints for it, for a health detail.
pub fn enum_name(element: &gstreamer::Element, property: &str) -> Option<String> {
    element.find_property(property)?;
    let value = element.property_value(property);
    value.serialize().ok().map(|s| s.to_string())
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

/// The SRT numbers a person actually wants, with the fields this build does
/// not publish left absent rather than reported as zero.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SrtNumbers {
    pub rtt_ms: Option<f64>,
    pub packets_lost: Option<i64>,
    pub packets_retransmitted: Option<i64>,
    pub bandwidth_mbps: Option<f64>,
    pub negotiated_latency_ms: Option<i64>,
    /// Anything that proves bytes are moving. Zero means nobody is there yet.
    pub moving: i64,
}

impl SrtNumbers {
    /// Read them off an `srtsrc` or `srtsink`, following a listener's `callers`
    /// list down to the peer that has the numbers.
    pub fn read(element: &gstreamer::Element) -> Option<SrtNumbers> {
        let top = of(element)?;
        let level = srt_level(&top, srt::MOVING).unwrap_or(top);
        let s = level.as_ref();
        Some(SrtNumbers {
            rtt_ms: first_float(s, srt::RTT_MS).map(round1),
            packets_lost: first_number(s, srt::LOST),
            packets_retransmitted: first_number(s, srt::RETRANSMITTED),
            bandwidth_mbps: first_float(s, srt::BANDWIDTH_MBPS).map(round1),
            negotiated_latency_ms: first_number(s, srt::NEGOTIATED_LATENCY_MS),
            moving: first_number(s, srt::MOVING).unwrap_or(0),
        })
    }

    /// One line for a `health` detail. Only the numbers this build publishes.
    pub fn phrase(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if let Some(v) = self.rtt_ms {
            parts.push(format!("rtt {v} ms"));
        }
        if let Some(v) = self.packets_lost {
            parts.push(format!("{v} lost"));
        }
        if let Some(v) = self.packets_retransmitted {
            parts.push(format!("{v} retransmitted"));
        }
        if let Some(v) = self.bandwidth_mbps {
            parts.push(format!("{v} Mbit/s link"));
        }
        if let Some(v) = self.negotiated_latency_ms {
            parts.push(format!("{v} ms negotiated"));
        }
        if parts.is_empty() {
            return "this build of srtsrc publishes no statistics".into();
        }
        parts.join(", ")
    }

    /// The same numbers as JSON, for a `stats` call.
    pub fn json(&self) -> serde_json::Value {
        let mut out = serde_json::Map::new();
        if let Some(v) = self.rtt_ms {
            out.insert("rtt_ms".into(), v.into());
        }
        if let Some(v) = self.packets_lost {
            out.insert("packets_lost".into(), v.into());
        }
        if let Some(v) = self.packets_retransmitted {
            out.insert("packets_retransmitted".into(), v.into());
        }
        if let Some(v) = self.bandwidth_mbps {
            out.insert("bandwidth_mbps".into(), v.into());
        }
        if let Some(v) = self.negotiated_latency_ms {
            out.insert("negotiated_latency_ms".into(), v.into());
        }
        out.insert("moving".into(), self.moving.into());
        serde_json::Value::Object(out)
    }
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

    #[test]
    fn an_element_is_found_inside_a_bin_by_the_property_it_has() {
        crate::init().expect("gstreamer");
        let bin = gstreamer::parse::bin_from_description("identity ! fakesink", true)
            .expect("the description parses");
        let element: gstreamer::Element = bin.upcast();
        // `qos` is a GstBaseSink property, so the fakesink inside answers.
        assert!(find_with_property(&element, "qos").is_some());
        assert!(find_with_property(&element, "not-a-property-anything-has").is_none());
    }

    #[test]
    fn an_enum_property_reads_back_as_its_printed_name() {
        crate::init().expect("gstreamer");
        let sink = gstreamer::ElementFactory::make("fakesink").build().expect("fakesink");
        let state = enum_name(&sink, "state").or_else(|| enum_name(&sink, "sync-mode"));
        // Whichever enum this build has, it must come back as a name and not a
        // number; a build with neither is allowed to answer nothing.
        if let Some(state) = state {
            assert!(!state.is_empty());
        }
        assert!(enum_name(&sink, "not-a-property").is_none());
    }

    #[test]
    fn an_element_with_no_stats_property_yields_nothing() {
        crate::init().expect("gstreamer");
        let filter = gstreamer::ElementFactory::make("capsfilter").build().expect("capsfilter");
        assert!(of(&filter).is_none());
        assert!(SrtNumbers::read(&filter).is_none());
    }

    #[test]
    fn an_element_whose_stats_are_not_srt_reports_no_srt_numbers() {
        crate::init().expect("gstreamer");
        // GstBaseSink publishes its own `stats` structure, which carries none
        // of the SRT field names. Reading it must yield an empty answer rather
        // than a wrong one.
        let sink = gstreamer::ElementFactory::make("fakesink").build().expect("fakesink");
        let numbers = SrtNumbers::read(&sink).expect("fakesink has a stats property");
        assert_eq!(numbers.moving, 0);
        assert_eq!(numbers.rtt_ms, None);
        assert!(numbers.phrase().contains("no statistics"));
    }

    #[test]
    fn a_phrase_names_only_the_numbers_that_are_there() {
        let n = SrtNumbers { rtt_ms: Some(12.5), packets_lost: Some(3), ..Default::default() };
        let phrase = n.phrase();
        assert!(phrase.contains("rtt 12.5 ms"), "{phrase}");
        assert!(phrase.contains("3 lost"), "{phrase}");
        assert!(!phrase.contains("retransmitted"), "{phrase}");
        assert_eq!(n.json()["rtt_ms"], 12.5);
        assert_eq!(n.json()["moving"], 0);
    }

    #[test]
    fn a_build_with_no_statistics_at_all_says_so_rather_than_reporting_zeros() {
        assert!(SrtNumbers::default().phrase().contains("no statistics"));
    }
}
