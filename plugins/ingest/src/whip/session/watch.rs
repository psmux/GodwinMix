//! When a WHIP session's peer has gone for good.
//!
//! WebRTC's Disconnected is not an ending. It is what a peer connection
//! reads during a Wi-Fi blip, a phone walking between access points, or a
//! second of loss on a busy link, and it comes back to Connected by itself
//! when the path does. Ending the session there threw away a publisher
//! that would have recovered, so the session is kept through Disconnected
//! for [`GRACE`] and ends only on Failed, on Closed, or on Disconnected
//! that has lasted longer than that.
//!
//! The publisher page (`ui/join/session.js`) gives Disconnected five
//! seconds before it offers again from scratch. [`GRACE`] is longer, so
//! the mixer never drops a session the page still expects to come back;
//! and a page that does offer again takes over the old session at once,
//! which by then has sent nothing for longer than `hub::STALE`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_webrtc::WebRTCPeerConnectionState as State;

/// How long Disconnected is given to come back by itself.
pub const GRACE: Duration = Duration::from_secs(15);

/// How long a new session may take to connect at all.
pub const FIRST: Duration = Duration::from_secs(20);

/// What the peer connection read, and since when it has been away.
pub struct Grace {
    started: Instant,
    connected: bool,
    lost: Option<Instant>,
}

impl Grace {
    pub fn new(now: Instant) -> Grace {
        Grace { started: now, connected: false, lost: None }
    }

    /// Whether the session is over, given what the connection reads at `now`.
    pub fn over(&mut self, state: State, now: Instant) -> bool {
        match state {
            State::Connected => {
                self.connected = true;
                self.lost = None;
                false
            }
            State::Failed | State::Closed => true,
            State::New | State::Connecting if !self.connected => now.saturating_duration_since(self.started) > FIRST,
            // Disconnected, or connecting again after it was connected.
            _ => now.saturating_duration_since(*self.lost.get_or_insert(now)) > GRACE,
        }
    }
}

/// A thread that reads the connection once a second, and ends the session
/// when the peer has gone for good or the gate has cut it off.
pub fn watch(bin: gst::Element, stop: Arc<AtomicBool>, ended: Box<dyn Fn() + Send>) {
    let _ = std::thread::Builder::new().name("gmx-whip-watch".into()).spawn(move || {
        let mut grace = Grace::new(Instant::now());
        loop {
            std::thread::sleep(Duration::from_secs(1));
            if stop.load(Ordering::Relaxed) {
                break;
            }
            if grace.over(bin.property::<State>("connection-state"), Instant::now()) {
                break;
            }
        }
        drop(bin);
        ended();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(start: Instant, secs: u64) -> Instant {
        start + Duration::from_secs(secs)
    }

    #[test]
    fn a_blip_of_disconnected_is_ridden_out_and_connected_again_resets_it() {
        let t = Instant::now();
        let mut g = Grace::new(t);
        assert!(!g.over(State::Connected, at(t, 1)));
        assert!(!g.over(State::Disconnected, at(t, 30)), "a session long connected is not ended by its first Disconnected");
        assert!(!g.over(State::Disconnected, at(t, 40)), "ten seconds of Disconnected is inside the grace");
        assert!(!g.over(State::Connected, at(t, 41)));
        assert!(!g.over(State::Disconnected, at(t, 50)), "the grace starts again after Connected");
        assert!(!g.over(State::Disconnected, at(t, 64)));
    }

    #[test]
    fn disconnected_past_the_grace_ends_it() {
        let t = Instant::now();
        let mut g = Grace::new(t);
        g.over(State::Connected, at(t, 1));
        assert!(!g.over(State::Disconnected, at(t, 2)));
        assert!(g.over(State::Disconnected, at(t, 2) + GRACE + Duration::from_secs(1)));
    }

    #[test]
    fn failed_and_closed_end_it_at_once() {
        let t = Instant::now();
        for end in [State::Failed, State::Closed] {
            let mut g = Grace::new(t);
            g.over(State::Connected, at(t, 1));
            assert!(g.over(end, at(t, 2)), "{end:?}");
        }
    }

    #[test]
    fn a_session_that_never_connects_ends_after_the_first_wait() {
        let t = Instant::now();
        let mut g = Grace::new(t);
        assert!(!g.over(State::Connecting, at(t, 10)));
        assert!(g.over(State::Connecting, at(t, 21)));
    }
}
