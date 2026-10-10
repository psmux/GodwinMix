//! How long the supervisor waits before it restarts a source that has stopped
//! delivering, and when it forgives one that came back.
//!
//! On 2026-10-05 the desktop app ran 16 hours without a fault, then the
//! machine filled up with other work and its sources began to miss their
//! frames. Each one was judged stalled, restarted, delivered a frame or two
//! and stalled again: 290 stalls in under two hours, 246 of them in the last
//! hour, a page rebuilt 43 times. Two things let that run. The backoff was
//! cleared by the first frame after a restart, so a source that came back for
//! a second was back on the fast path, and the stall limit was the same ten
//! seconds on the fortieth restart as on the first. A restart on a starved
//! machine costs more CPU than it saves, so the storm fed itself.
//!
//! So three rules, each cheap and each read on the tick the supervisor
//! already runs:
//!
//! * A source is forgiven only after it has stayed live for [`HEALTHY_FOR`].
//!   Until then every further stall restart is a strike against it.
//! * Each strike doubles how long it may stay stalled before the next
//!   restart, up to [`MAX_DOUBLINGS`] doublings. Ten seconds becomes twenty,
//!   forty, eighty and then 160, where it stays. A source that is really dead
//!   is still restarted, a few times an hour rather than a few times a minute,
//!   and it reads `stalled` the whole time.
//! * A tick on which the programme itself made fewer than half its frames
//!   counts a quarter towards that limit. The machine is short of CPU, and a
//!   source missing frames then is more likely starved than dead.

use std::time::{Duration, Instant};

/// How long a source must stay live after a restart before its strikes are
/// cleared. A minute: longer than any flicker seen on 2026-10-05, where a
/// page came back for under two seconds and a camera for under ten, and short
/// enough that a source which really recovered is on the fast path again
/// before anyone has to think about it.
pub const HEALTHY_FOR: Duration = Duration::from_secs(60);

/// How many times the stall limit may double. Four takes the default ten
/// seconds to 160.
pub const MAX_DOUBLINGS: u32 = 4;

/// One source's standing with the supervisor.
#[derive(Debug, Default, Clone)]
pub struct Patience {
    /// Stall restarts since this source last stayed live for `HEALTHY_FOR`.
    strikes: u32,
    /// When the source was last seen to turn live, while it still is.
    live_since: Option<Instant>,
    /// Time spent stalled since it last delivered, weighted by the load.
    stalled: Duration,
}

impl Patience {
    /// Strikes against this source, for the log and the tests.
    pub fn strikes(&self) -> u32 {
        self.strikes
    }

    /// How long this source may stay stalled before it is restarted, given
    /// the configured limit for a source with no strikes.
    pub fn stall_limit(&self, base: Duration) -> Duration {
        base.saturating_mul(1 << self.strikes.min(MAX_DOUBLINGS))
    }

    /// One tick of a stalled source. `loaded` says the programme itself fell
    /// short of its frame rate on this tick. Returns true once the time spent
    /// stalled has reached the limit, and keeps returning true until the
    /// source is restarted or delivers again.
    pub fn stalled_tick(&mut self, tick: Duration, loaded: bool, base: Duration) -> bool {
        self.live_since = None;
        self.stalled += if loaded { tick / 4 } else { tick };
        self.stalled >= self.stall_limit(base)
    }

    /// One tick of a source that is not stalled. Returns true on the tick a
    /// source with strikes has been live for `HEALTHY_FOR`, which is when the
    /// caller clears its own backoff counters too.
    pub fn healthy_tick(&mut self, live: bool, now: Instant) -> bool {
        self.stalled = Duration::ZERO;
        if !live {
            self.live_since = None;
            return false;
        }
        let since = *self.live_since.get_or_insert(now);
        if self.strikes > 0 && now.duration_since(since) >= HEALTHY_FOR {
            self.strikes = 0;
            return true;
        }
        false
    }

    /// A stall restart has been armed for this source.
    pub fn struck(&mut self) {
        self.strikes = self.strikes.saturating_add(1);
        self.stalled = Duration::ZERO;
        self.live_since = None;
    }

    /// Whether a source with this record may have its backoff counters
    /// cleared: it has no strikes, so nothing is waiting on it to prove
    /// itself.
    pub fn forgiven(&self) -> bool {
        self.strikes == 0
    }
}

impl super::Mixer {
    /// Restart a source the stall sweep has given up on, and count it as a
    /// strike if a restart was armed. Only a stall is a strike. A clip that
    /// loops restarts at every end of stream and a server that refuses us
    /// errors; both keep the backoff they always had, cleared by the next
    /// frame, so a ten second clip does not wait ten seconds between loops.
    pub(super) fn arm_stall_restart(&mut self, id: crate::state::SourceId) {
        self.arm_strike(id, "it has delivered nothing for too long");
    }

    /// Restart a source for a fault that counts against it, `why` being the
    /// fault: a stall, or a plugin that says it is failing.
    pub(super) fn arm_strike(&mut self, id: crate::state::SourceId, why: &'static str) {
        if !self.arm_source_restart(id.clone(), why) {
            return;
        }
        let patience = self.patience.entry(id.clone()).or_default();
        patience.struck();
        let base = Duration::from_secs(self.cfg.stall.restart_after_secs);
        tracing::info!(
            source = %id,
            strikes = patience.strikes(),
            next_stall_limit_secs = patience.stall_limit(base).as_secs(),
            "a stall restart is a strike until the source stays live for a minute"
        );
    }

    /// Whether the programme made under half its frames since the last tick.
    /// One read of the frame counter the programme probe keeps anyway.
    pub(super) fn programme_starved(&mut self) -> bool {
        let frames = crate::observe::metrics::counter("gmx_programme_frames_total", &[]).get();
        let (then, before) = std::mem::replace(&mut self.programme_frames, (Instant::now(), frames));
        let fps = self.canvas.fps.numer() as f64 / self.canvas.fps.denom().max(1) as f64;
        programme_starved(frames.saturating_sub(before), then.elapsed(), fps)
    }
}

/// Whether the programme fell short of its frame rate over one tick, from the
/// programme frame counter before and after it. Short means under half the
/// frames the canvas rate asks for, which no healthy machine misses and a
/// starved one does.
pub fn programme_starved(frames: u64, elapsed: Duration, fps: f64) -> bool {
    let expected = elapsed.as_secs_f64() * fps;
    // Under two frames expected says nothing either way: a tick that came
    // round early, or a canvas so slow that one frame is a long time.
    expected >= 2.0 && (frames as f64) < expected / 2.0
}

#[cfg(test)]
mod tests;
