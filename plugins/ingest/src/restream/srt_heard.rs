//! Whether an SRT receiver is still there.
//!
//! `srtsink` dials again by itself when its connection breaks and posts
//! nothing while it does, so a destination whose receiver vanished behind a
//! pulled cable read `live` for good with every byte dropped. The receiver
//! acknowledges what it gets every 10 ms; a count of those that has not moved
//! for `QUIET` is a receiver that is not there, and the link is given up and
//! dialled again like any other loss.

use std::time::{Duration, Instant};

use gstreamer as gst;
use gstreamer::prelude::*;

/// Long enough for a first connection on a slow network, and for a hiccup.
pub const QUIET: Duration = Duration::from_secs(8);

pub struct Heard {
    last: u64,
    at: Instant,
}

impl Heard {
    pub fn new(now: Instant) -> Heard {
        Heard { last: 0, at: now }
    }

    /// Whether the receiver has acknowledged anything on this link.
    pub fn ever(&self) -> bool {
        self.last > 0
    }

    /// `Err` with how long it has been quiet once nothing has come back for
    /// `QUIET`. A sink that reports no count is not judged.
    pub fn check(&mut self, count: Option<u64>, now: Instant) -> Result<(), Duration> {
        let Some(count) = count else { return Ok(()) };
        if count != self.last {
            (self.last, self.at) = (count, now);
            return Ok(());
        }
        let quiet = now.saturating_duration_since(self.at);
        if quiet >= QUIET { Err(quiet) } else { Ok(()) }
    }
}

/// The acknowledgements a caller's `srtsink` has had, zero while it is
/// still dialling and its statistics are empty. A listener answers `None`:
/// being without a caller is its normal state, so it is not judged.
pub fn acks(sink: &gst::Element, listener: bool) -> Option<u64> {
    if listener {
        return None;
    }
    let Some(stats) = sink.property::<Option<gst::Structure>>("stats") else { return Some(0) };
    let field = "packet-ack-received";
    let n = stats.get::<i32>(field).map(|v| v.max(0) as u64).or_else(|_| stats.get::<i64>(field).map(|v| v.max(0) as u64));
    Some(n.unwrap_or(0))
}

/// An address `srtsink` listens on: `mode=listener`, or no host.
pub fn listens(uri: &str) -> bool {
    let lower = uri.trim().to_lowercase();
    let rest = lower.strip_prefix("srt://").unwrap_or(&lower);
    let (hostport, query) = rest.split_once('?').unwrap_or((rest, ""));
    match query.split('&').find_map(|kv| kv.strip_prefix("mode=")) {
        Some(m) => m == "listener" || m == "server",
        None => hostport.starts_with(':') || hostport.is_empty(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_count_that_stops_is_noticed_after_the_quiet_time() {
        let t = Instant::now();
        let mut h = Heard::new(t);
        assert!(h.check(Some(0), t + Duration::from_secs(1)).is_ok(), "still dialling");
        assert!(h.check(Some(0), t + QUIET).is_err(), "nothing ever came back");
        let mut h = Heard::new(t);
        assert!(h.check(Some(50), t + Duration::from_secs(1)).is_ok());
        assert!(h.check(Some(50), t + Duration::from_secs(5)).is_ok());
        assert!(h.check(Some(50), t + Duration::from_secs(1) + QUIET).is_err(), "a receiver that went quiet");
        assert!(h.check(None, t + Duration::from_secs(60)).is_ok(), "a listener is not judged");
    }

    #[test]
    fn a_listener_is_named_or_has_no_host() {
        assert!(listens("srt://:9000"));
        assert!(listens("srt://127.0.0.1:9000?mode=listener"));
        assert!(!listens("srt://ingest.example:9000?latency=200"));
    }
}
