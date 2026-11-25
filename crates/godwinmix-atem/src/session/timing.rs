//! How long the session waits for each thing. The defaults suit a switcher
//! on a local network; a test shortens them so a reconnect takes
//! milliseconds rather than seconds.

use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timing {
    /// The wait between hellos while nothing answers, at first.
    pub hello_every: Duration,
    /// The wait doubles once the switcher has been silent for
    /// `silent_after`, up to this.
    pub hello_max: Duration,
    /// Silence for this long ends a session, and is when an unanswered hello
    /// starts to say so.
    pub silent_after: Duration,
    /// How long to wait for an acknowledgement before sending again.
    pub resend_after: Duration,
    /// How many times to send again before giving up on the session.
    pub max_resends: u32,
    /// How long to wait after the switcher said every slot is taken.
    pub full_backoff: Duration,
}

impl Default for Timing {
    fn default() -> Timing {
        Timing {
            hello_every: Duration::from_secs(1),
            hello_max: Duration::from_secs(8),
            silent_after: Duration::from_secs(5),
            resend_after: Duration::from_millis(200),
            max_resends: 10,
            full_backoff: Duration::from_secs(5),
        }
    }
}

impl Timing {
    /// Every wait divided by `factor`, for tests on loopback.
    pub fn faster(self, factor: u32) -> Timing {
        Timing {
            hello_every: self.hello_every / factor,
            hello_max: self.hello_max / factor,
            silent_after: self.silent_after / factor,
            resend_after: self.resend_after / factor,
            max_resends: self.max_resends,
            full_backoff: self.full_backoff / factor,
        }
    }
}
