//! The state machine of 03 section 7, and the restart backoff.
//!
//! ```text
//!   spawn -> starting -> ready -> running -> stalled -> (restart) -> running
//!              |            ^        |
//!              v            +--------+ stop
//!            failed              stopped
//! ```
//!
//! `configure` does not change state. `shutdown` is legal from any state and
//! leads to process exit. Three restarts are free, then 30 seconds doubling to
//! 300, cleared on the first frame and on removal, which is what the
//! supervisor in mixer.rs already does for the browser sidecar.

use godwinmix_protocol::plugin::wire::InstanceState;
use std::time::{Duration, Instant};

/// Restarts that cost nothing before the backoff starts counting.
pub const FREE_RESTARTS: u32 = 3;
/// The first wait after the free restarts are spent.
pub const FIRST_BACKOFF: Duration = Duration::from_secs(30);
/// The ceiling. A plugin that has been failing for this long is not going to
/// be fixed by trying again sooner.
pub const MAX_BACKOFF: Duration = Duration::from_secs(300);

/// How long to wait before the next attempt, given how many have been made.
#[derive(Debug, Clone, Default)]
pub struct Backoff {
    attempts: u32,
}

impl Backoff {
    pub fn new() -> Self {
        Self::default()
    }

    /// Count one attempt and say how long to wait before the next.
    pub fn next_wait(&mut self) -> Duration {
        self.attempts += 1;
        self.wait()
    }

    /// The wait as it stands, without counting an attempt.
    pub fn wait(&self) -> Duration {
        if self.attempts <= FREE_RESTARTS {
            return Duration::ZERO;
        }
        let steps = self.attempts - FREE_RESTARTS - 1;
        let secs = FIRST_BACKOFF.as_secs().saturating_mul(1u64 << steps.min(8));
        Duration::from_secs(secs.min(MAX_BACKOFF.as_secs()))
    }

    pub fn attempts(&self) -> u32 {
        self.attempts
    }

    /// The first frame arrived, or the source was removed. Either way the
    /// count goes back to nothing.
    pub fn clear(&mut self) {
        self.attempts = 0;
    }
}

/// One instance's state, and what it is allowed to do next.
#[derive(Debug, Clone)]
pub struct Lifecycle {
    state: InstanceState,
    detail: Option<String>,
    since: Instant,
    backoff: Backoff,
    restarts: u32,
}

impl Default for Lifecycle {
    fn default() -> Self {
        Self::new()
    }
}

impl Lifecycle {
    pub fn new() -> Self {
        Self {
            state: InstanceState::Starting,
            detail: None,
            since: Instant::now(),
            backoff: Backoff::new(),
            restarts: 0,
        }
    }

    pub fn state(&self) -> InstanceState {
        self.state
    }

    pub fn detail(&self) -> Option<&str> {
        self.detail.as_deref()
    }

    pub fn restarts(&self) -> u32 {
        self.restarts
    }

    pub fn in_state_for(&self) -> Duration {
        self.since.elapsed()
    }

    /// Move to a state, with the reason. Returns true when the state actually
    /// changed, so a caller emits `event/plugin.state` once rather than every
    /// tick.
    pub fn to(&mut self, state: InstanceState, detail: Option<String>) -> bool {
        let changed = self.state != state;
        if changed {
            self.since = Instant::now();
        }
        // The detail may change without the state changing: a degraded plugin
        // saying something new is worth an event.
        let said_something_new = self.detail != detail;
        self.state = state;
        self.detail = detail;
        changed || said_something_new
    }

    /// The instance answered `initialize`. Refuse anything else in `starting`.
    pub fn initialized(&mut self) -> bool {
        self.to(InstanceState::Ready, None)
    }

    /// A buffer arrived. The backoff is cleared on the first frame, as the
    /// browser supervisor already does.
    pub fn first_frame(&mut self) -> bool {
        self.backoff.clear();
        self.to(InstanceState::Running, None)
    }

    /// No buffers for `stall_timeout_secs`.
    pub fn stalled(&mut self, detail: impl Into<String>) -> bool {
        self.to(InstanceState::Stalled, Some(detail.into()))
    }

    /// The process is gone, or it refused the handshake.
    pub fn failed(&mut self, detail: impl Into<String>) -> bool {
        self.to(InstanceState::Failed, Some(detail.into()))
    }

    /// Ask for a restart. `None` means it is not time yet, and says when.
    pub fn may_restart(&mut self) -> Result<(), Duration> {
        let wait = self.backoff.wait();
        if wait > Duration::ZERO && self.since.elapsed() < wait {
            return Err(wait - self.since.elapsed());
        }
        Ok(())
    }

    /// A restart is being made. Counts it and moves back to `starting`.
    pub fn restarting(&mut self) {
        self.restarts += 1;
        self.backoff.next_wait();
        self.to(InstanceState::Starting, Some("restarting".into()));
    }

    /// Whether the core may call this method now (03 section 6).
    pub fn may_call(&self, method: &str) -> bool {
        self.state.may_call(method)
    }

    /// The refusal a call in the wrong state gets, naming the state and what
    /// to wait for.
    pub fn refusal(&self, method: &str) -> String {
        format!(
            "`{method}` is not legal while the plugin is {}. {} Wait for \
             event/plugin.state.",
            self.state.as_str(),
            match self.state {
                InstanceState::Starting => "It has not answered `initialize` yet.",
                InstanceState::Failed => "It is being restarted by the supervisor.",
                InstanceState::Ready | InstanceState::Stopped =>
                    "Start it first, or call configure, health, discover, tool.call or shutdown.",
                _ => "Stop it first.",
            }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_restarts_are_free_then_the_wait_doubles_to_a_ceiling() {
        let mut b = Backoff::new();
        for _ in 0..FREE_RESTARTS {
            assert_eq!(b.next_wait(), Duration::ZERO, "the first three cost nothing");
        }
        assert_eq!(b.next_wait(), FIRST_BACKOFF);
        assert_eq!(b.next_wait(), FIRST_BACKOFF * 2);
        assert_eq!(b.next_wait(), FIRST_BACKOFF * 4);
        for _ in 0..20 {
            b.next_wait();
        }
        assert_eq!(b.wait(), MAX_BACKOFF, "it stops at five minutes");
        b.clear();
        assert_eq!(b.wait(), Duration::ZERO, "a frame clears the count");
    }

    #[test]
    fn the_state_machine_walks_the_documented_path() {
        let mut life = Lifecycle::new();
        assert_eq!(life.state(), InstanceState::Starting);
        assert!(!life.may_call("start"), "nothing is legal before initialize");
        assert!(life.initialized());
        assert!(life.may_call("start"));
        assert!(life.first_frame());
        assert_eq!(life.state(), InstanceState::Running);
        assert!(life.may_call("stop"));
        assert!(!life.may_call("start"), "a running source is not started twice");
        assert!(life.stalled("no buffers for 2.0 s"));
        assert_eq!(life.detail(), Some("no buffers for 2.0 s"));
        life.restarting();
        assert_eq!(life.state(), InstanceState::Starting);
        assert_eq!(life.restarts(), 1);
    }

    #[test]
    fn a_state_that_does_not_change_does_not_raise_a_second_event() {
        let mut life = Lifecycle::new();
        life.initialized();
        assert!(!life.to(InstanceState::Ready, None), "the same state again says nothing");
        assert!(
            life.to(InstanceState::Ready, Some("waiting for the camera".into())),
            "a new detail in the same state is worth an event"
        );
    }

    #[test]
    fn a_refusal_names_the_state_and_the_event_to_wait_for() {
        let life = Lifecycle::new();
        let text = life.refusal("seek");
        assert!(text.contains("starting"), "{text}");
        assert!(text.contains("event/plugin.state"), "{text}");
    }
}
