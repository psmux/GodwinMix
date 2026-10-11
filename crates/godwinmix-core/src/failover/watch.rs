//! One source's standing with its backup, as a clock and three flags.
//!
//! Plain data and arithmetic on `Instant`s: nothing here touches a pipeline,
//! so every rule about when to switch and when to come back is tested
//! without one.

use godwinmix_protocol::backup::{BackupPhase, OnLoss, ReturnMode};
use std::time::{Duration, Instant};

/// What a step decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// The source has been down for `after_secs`: its backup goes on.
    Engage,
    /// It has been live for `return_after_secs` and returns by itself.
    Return,
}

#[derive(Debug, Default, Clone)]
pub(super) struct Watch {
    /// When the current outage began. `None` while the source is live.
    pub down_since: Option<Instant>,
    /// When the source came back, while it is live and on its backup.
    pub up_since: Option<Instant>,
    /// When the backup went on, while it is on.
    pub engaged: Option<Instant>,
    /// The operator said to stay on the backup until they return it.
    pub stay: bool,
    /// Seen live at least once since the rule was set.
    pub ever_live: bool,
    /// The source has not been live since it was added: this outage is a
    /// first connection that has not come.
    pub never_live: bool,
}

/// One look at a source.
#[derive(Debug, Clone, Copy, Default)]
pub struct Look {
    pub live: bool,
    pub down: bool,
    /// How long it has been making its first connection, while it has never
    /// been live. See [`super::Liveness::unconnected_for`].
    pub unconnected: Option<Duration>,
    /// How long a first connection is given before it counts as down:
    /// `stall.connect_timeout_secs`.
    pub connect_wait: Duration,
}

fn secs(s: f64) -> Duration {
    Duration::from_secs_f64(s.clamp(0.0, godwinmix_protocol::backup::MAX_WAIT_SECS))
}

impl Watch {
    /// Take one look at the source and say whether that changes anything.
    ///
    /// A source still making its first connection is given the connect
    /// timeout before it counts as down, so a camera a second slow to start
    /// does not swap the picture there and back. Past that, its outage is
    /// dated from when it was added, and the backup goes on once `after_secs`
    /// of it have passed: at the connect timeout or `after_secs`, whichever
    /// is later.
    pub fn step(&mut self, rule: &OnLoss, look: Look, now: Instant) -> Option<Step> {
        let first = look.unconnected.filter(|_| !look.live);
        if !look.live && !look.down && first.is_none_or(|waited| waited < look.connect_wait) {
            return None;
        }
        self.ever_live |= look.live;
        self.never_live = first.is_some() && !self.ever_live;
        if look.live {
            self.down_since = None;
            self.engaged?;
            let up = *self.up_since.get_or_insert(now);
            let auto = rule.returns == ReturnMode::Auto && !self.stay;
            if auto && now.saturating_duration_since(up) >= secs(rule.return_after_secs) {
                self.disengage();
                return Some(Step::Return);
            }
            return None;
        }
        self.up_since = None;
        let began = first.and_then(|waited| now.checked_sub(waited)).unwrap_or(now);
        let down = *self.down_since.get_or_insert(began);
        if self.engaged.is_none() && now.saturating_duration_since(down) >= secs(rule.after_secs) {
            self.engaged = Some(now);
            return Some(Step::Engage);
        }
        None
    }

    /// Back to the source itself, whoever asked.
    pub fn disengage(&mut self) {
        self.engaged = None;
        self.up_since = None;
        self.stay = false;
    }

    pub fn engaged(&self) -> bool {
        self.engaged.is_some()
    }

    /// Where it stands, from the clock alone.
    pub fn phase(&self, rule: &OnLoss) -> BackupPhase {
        match (self.engaged(), self.down_since.is_some()) {
            (false, false) => BackupPhase::Ready,
            (false, true) => BackupPhase::Lost,
            (true, true) => BackupPhase::OnBackup,
            // Engaged and live, but not yet seen live by a step: still on it.
            (true, false) if self.up_since.is_none() => BackupPhase::OnBackup,
            (true, false) if rule.returns == ReturnMode::Auto && !self.stay => BackupPhase::Returning,
            (true, false) => BackupPhase::MainBack,
        }
    }

    /// How long until the backup goes on, while the source is lost.
    pub fn switch_in(&self, rule: &OnLoss, now: Instant) -> Option<Duration> {
        let down = self.down_since.filter(|_| !self.engaged())?;
        Some(secs(rule.after_secs).saturating_sub(now.saturating_duration_since(down)))
    }

    /// How long until the source goes back on, while it is returning.
    pub fn return_in(&self, rule: &OnLoss, now: Instant) -> Option<Duration> {
        if self.phase(rule) != BackupPhase::Returning {
            return None;
        }
        let up = self.up_since?;
        Some(secs(rule.return_after_secs).saturating_sub(now.saturating_duration_since(up)))
    }

    /// How long the source has been without a picture.
    pub fn lost_for(&self, now: Instant) -> Option<Duration> {
        self.down_since.map(|d| now.saturating_duration_since(d))
    }
}
