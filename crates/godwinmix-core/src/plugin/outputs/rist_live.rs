//! Where a RIST output sends, and what it hears back.
//!
//! The receiver's RTCP reports carry the highest sequence number it has had
//! and the last sender report it saw. Both move with every report while the
//! receiver is getting the stream and stop when it is not, so their sum,
//! judged by `progress`, is the liveness. `ristsink`'s own statistics carry
//! only the round trip time, which keeps its last value after the receiver
//! is gone; it is the fallback for a build whose `rtpbin` cannot be reached.

use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;

/// What the receiver's reports say it has had, or `None` when the sink's
/// sessions cannot be read.
pub fn answered(sink: &gst::Element) -> Option<u64> {
    let rtpbin = sink.downcast_ref::<gst::Bin>()?.by_name("rist_send_rtpbin")?;
    let mut sum = 0u64;
    for id in 0u32..16 {
        let Some(session) = rtpbin.emit_by_name::<Option<glib::Object>>("get-internal-session", &[&id]) else { break };
        let stats = session.property::<gst::Structure>("stats");
        let Ok(sources) = stats.get::<glib::ValueArray>("source-stats") else { continue };
        sum += sources.iter().filter_map(|v| v.get::<gst::Structure>().ok()).map(|s| reported(&s)).sum::<u64>();
    }
    Some(sum)
}

/// What one source's receiver report says, nothing when it has none.
pub fn reported(s: &gst::StructureRef) -> u64 {
    if !s.get::<bool>("have-rb").unwrap_or(false) {
        return 0;
    }
    let field = |f: &str| s.get::<u32>(f).map(u64::from).unwrap_or(0);
    field("rb-exthighestseq") + field("rb-lsr")
}

/// The round trip time `ristsink` reports, above zero once any receiver
/// answered.
pub fn round_trip(sink: &gst::Element) -> u64 {
    let Some(stats) = sink.property::<Option<gst::Structure>>("stats") else { return 0 };
    let Ok(sessions) = stats.get::<glib::ValueArray>("session-stats") else { return 0 };
    let each = sessions.iter().filter_map(|v| v.get::<gst::Structure>().ok());
    each.map(|s| s.get::<u64>("round-trip-time").unwrap_or(0)).max().unwrap_or(0)
}

/// `rist://host:port`, with the port RTP goes to. RIST puts RTCP on the next
/// port up, so the RTP port has to be even.
pub fn address(uri: &str) -> Result<(String, u16)> {
    let rest = uri.trim().strip_prefix("rist://").or_else(|| uri.trim().strip_prefix("RIST://"));
    let hostport = rest.map(|r| r.split(['?', '/']).next().unwrap_or(r)).unwrap_or("");
    let (host, port) = hostport
        .rsplit_once(':')
        .and_then(|(h, p)| Some((h.trim_matches(['[', ']']).to_string(), p.parse::<u16>().ok()?)))
        .filter(|(h, _)| !h.is_empty())
        .with_context(|| format!("rist/output needs an address such as rist://192.168.1.50:5004, not `{uri}`"))?;
    anyhow::ensure!(
        port % 2 == 0,
        "rist/output port {port} is odd. RIST sends RTP to an even port and RTCP to the one above it; use {} or {}",
        port - 1,
        port + 1
    );
    Ok((host, port))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_report_counts_only_when_there_is_one() {
        let _ = gst::init();
        let none = gst::Structure::builder("s").field("have-rb", false).field("rb-exthighestseq", 9u32).build();
        assert_eq!(reported(&none), 0);
        let some = gst::Structure::builder("s").field("have-rb", true).field("rb-exthighestseq", 9u32).field("rb-lsr", 1u32).build();
        assert_eq!(reported(&some), 10);
    }
}
