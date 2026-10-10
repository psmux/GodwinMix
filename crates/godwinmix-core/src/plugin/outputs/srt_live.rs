//! What `srtsink`'s statistics say about the receiver.
//!
//! The count read is the acknowledgements the receiver sent back, which SRT
//! sends every 10 ms while data arrives and which stop the moment the far end
//! is gone. Caller mode has them at the top level; listener mode has one
//! structure per caller in `callers`, a value array, and they are added up.
//! A build whose statistics carry no acknowledgement count falls back to the
//! packets sent, which a dead receiver stops too once SRT gives up on it,
//! and statistics with neither are a socket that has heard nothing yet.

use gstreamer as gst;

/// Fields that count what came back, or failing that what went out, tried in
/// order.
const FIELDS: &[&str] = &["packet-ack-received", "packets-sent", "bytes-sent"];

/// The count for one socket's statistics, whatever integer width the version
/// chose for it.
fn count(s: &gst::StructureRef) -> Option<u64> {
    FIELDS.iter().copied().find_map(|f| {
        s.get::<i32>(f)
            .map(|v| v.max(0) as u64)
            .or_else(|_| s.get::<i64>(f).map(|v| v.max(0) as u64))
            .or_else(|_| s.get::<u64>(f))
            .or_else(|_| s.get::<u32>(f).map(u64::from))
            .ok()
    })
}

/// The receiver's count across the statistics. A socket still dialling may
/// report none of the fields, and that is nothing heard yet, not a sink that
/// cannot say: it used to read as connected for as long as the dial lasted.
pub fn answered(stats: &gst::StructureRef) -> u64 {
    if let Ok(callers) = stats.get::<glib::ValueArray>("callers") {
        let each: Vec<gst::Structure> = callers.iter().filter_map(|v| v.get().ok()).collect();
        return of_callers(&each);
    }
    count(stats).unwrap_or(0)
}

/// The listener's count: every caller's, added up.
fn of_callers(callers: &[gst::Structure]) -> u64 {
    callers.iter().filter_map(|c| count(c)).sum()
}

/// A listener waits for callers and takes them as they come, so being without
/// one is not a fault to rebuild over. `mode=listener` says so, and so does an
/// address with no host, which is how `srtsink` reads `srt://:9000`.
pub fn is_listener(uri: &str) -> bool {
    let lower = uri.trim().to_lowercase();
    let rest = lower.strip_prefix("srt://").unwrap_or(&lower);
    let (hostport, query) = rest.split_once('?').unwrap_or((rest, ""));
    let mode = query.split('&').find_map(|kv| kv.strip_prefix("mode="));
    match mode {
        Some(m) => m == "listener" || m == "server",
        None => hostport.starts_with(':') || hostport.is_empty(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acknowledgements_are_read_first_and_callers_are_added_up() {
        let _ = gst::init();
        let caller = gst::Structure::builder("s").field("packet-ack-received", 7i32).field("packets-sent", 900i64).build();
        assert_eq!(answered(&caller), 7);
        let old = gst::Structure::builder("s").field("packets-sent", 900i64).build();
        assert_eq!(answered(&old), 900);
        let callers = [3i32, 4].map(|n| gst::Structure::builder("c").field("packet-ack-received", n).build());
        assert_eq!(of_callers(&callers), 7);
        // The listener's own running total never goes down, and is never
        // what is read.
        let listener = gst::Structure::builder("s").field("bytes-sent-total", 5_000u64).build();
        assert_eq!(answered(&listener), 0);
        assert_eq!(answered(&gst::Structure::new_empty("s")), 0);
    }

    #[test]
    fn a_listener_is_named_or_has_no_host() {
        assert!(is_listener("srt://:9000"));
        assert!(is_listener("srt://0.0.0.0:9000?mode=listener"));
        assert!(!is_listener("srt://ingest.example:9000"));
        assert!(!is_listener("srt://ingest.example:9000?mode=caller&latency=200"));
    }

}
