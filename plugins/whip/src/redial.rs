//! When a WHIP output that has not posted an error should dial again anyway.
//!
//! The supervisor used to redial only when the pipeline failed. A pulled
//! cable does not fail the pipeline: ICE goes to `disconnected`, then perhaps
//! `failed`, and nothing reaches the bus, so the output sat degraded for the
//! rest of the show. This is the judgement the supervisor now asks for on
//! every tick, kept apart from the threads so it can be tested on its own.

use std::time::{Duration, Instant};

/// ICE `disconnected` is allowed to recover by itself for this long, which
/// covers a Wi-Fi roam or a router that dropped a few packets.
pub const DISCONNECTED_FOR: Duration = Duration::from_secs(10);

/// An attempt that has not come up in this long is not going to: the offer,
/// the answer and ICE all fit in a few seconds on any real network.
pub const UP_WITHIN: Duration = Duration::from_secs(30);

#[derive(Default)]
pub struct Watch {
    /// When the current attempt stopped being up, or was built.
    down_since: Option<Instant>,
    /// When ICE went to `disconnected` and stayed there.
    disconnected_since: Option<Instant>,
}

impl Watch {
    /// A fresh attempt: both clocks start again.
    pub fn built(&mut self, now: Instant) {
        self.down_since = Some(now);
        self.disconnected_since = None;
    }

    /// Why this attempt should be dropped and dialled again, or `None` while
    /// it is up or still has time. `broken` is a pipeline error, `up` is the
    /// health being ok, `ice` the ICE state when the sink reports one.
    pub fn redial(&mut self, broken: bool, up: bool, ice: Option<&str>, now: Instant) -> Option<String> {
        if up {
            self.down_since = None;
            self.disconnected_since = None;
            return None;
        }
        let down_since = *self.down_since.get_or_insert(now);
        if broken {
            return Some("the pipeline failed".into());
        }
        let ice = ice.unwrap_or_default();
        if ice.contains("failed") || ice.contains("closed") {
            return Some(format!("ICE went to {ice}"));
        }
        if ice.contains("disconnected") {
            let since = *self.disconnected_since.get_or_insert(now);
            if now.saturating_duration_since(since) >= DISCONNECTED_FOR {
                return Some(format!("ICE has been disconnected for {} s", DISCONNECTED_FOR.as_secs()));
            }
        } else {
            self.disconnected_since = None;
        }
        (now.saturating_duration_since(down_since) >= UP_WITHIN)
            .then(|| format!("the connection has not come up in {} s", UP_WITHIN.as_secs()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failed_or_lingering_disconnect_is_redialled_and_a_short_one_is_not() {
        let t = Instant::now();
        let mut w = Watch::default();
        w.built(t);
        assert!(w.redial(false, true, Some("connected"), t).is_none());
        assert!(w.redial(false, false, Some("disconnected"), t + Duration::from_secs(1)).is_none(), "a hiccup");
        assert!(w.redial(false, true, Some("connected"), t + Duration::from_secs(3)).is_none(), "and it recovered");
        let cut = t + Duration::from_secs(60);
        assert!(w.redial(false, false, Some("disconnected"), cut).is_none());
        assert!(w.redial(false, false, Some("disconnected"), cut + DISCONNECTED_FOR).is_some(), "a pulled cable");
        w.built(cut);
        assert!(w.redial(false, false, Some("failed"), cut + Duration::from_secs(1)).is_some());
    }

    #[test]
    fn an_attempt_that_never_comes_up_is_redialled_after_the_limit() {
        let t = Instant::now();
        let mut w = Watch::default();
        w.built(t);
        assert!(w.redial(false, false, Some("checking"), t + UP_WITHIN - Duration::from_secs(1)).is_none());
        assert!(w.redial(false, false, Some("checking"), t + UP_WITHIN).is_some());
        w.built(t + UP_WITHIN);
        assert!(w.redial(false, false, None, t + UP_WITHIN + Duration::from_secs(1)).is_none());
        assert!(w.redial(true, false, None, t + UP_WITHIN + Duration::from_secs(1)).is_some(), "an error at once");
    }
}
