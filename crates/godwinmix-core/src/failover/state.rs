//! Which backup should be drawn, and what the status says about it.

use super::{Board, Liveness, BACKUP_GRACE};
use godwinmix_protocol::backup::{BackupPhase, BackupState};
use std::time::{Duration, Instant};

/// One source whose stand in should change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Swap {
    pub source: String,
    /// What is drawn in its place now, `None` for the source itself.
    pub from: Option<String>,
    /// What should be.
    pub to: Option<String>,
}

impl Board {
    /// The backups of `id`, in the order they are tried: its own, then that
    /// one's, and so on, stopping before anything already in the list.
    pub fn chain(&self, id: &str) -> Vec<String> {
        let mut chain: Vec<String> = Vec::new();
        let mut at = id.to_string();
        while let Some(next) = self.rule(&at).map(|r| r.backup.trim().to_string()) {
            if next == id || chain.contains(&next) {
                break;
            }
            chain.push(next.clone());
            at = next;
        }
        chain
    }

    /// Who should be drawn in `id`'s place now. The backup on screen stays
    /// while it is live or only just quiet, so a second backup coming back
    /// does not swap pictures for nothing; otherwise the first live one.
    fn target(&self, id: &str, live: &impl Liveness) -> Option<String> {
        let entry = self.entries.get(id)?;
        if entry.rule.is_none() || !entry.watch.engaged() {
            return None;
        }
        let chain = self.chain(id);
        if let Some(now) = entry.showing.as_ref().filter(|s| chain.contains(s)) {
            if live.live_within(now, BACKUP_GRACE) {
                return Some(now.clone());
            }
        }
        chain.into_iter().find(|b| live.live(b))
    }

    /// The first live backup in the chain whether or not the source is down,
    /// to warn before it matters.
    fn cover(&self, id: &str, live: &impl Liveness) -> Option<String> {
        self.chain(id).into_iter().find(|b| live.live(b))
    }

    /// Every source whose stand in should change.
    pub fn changes(&self, live: &impl Liveness) -> Vec<Swap> {
        self.entries
            .iter()
            .filter_map(|(id, e)| {
                let to = self.target(id, live);
                (to != e.showing).then(|| Swap { source: id.clone(), from: e.showing.clone(), to })
            })
            .collect()
    }

    /// The mixer is drawing `to` in `id`'s place now. Answers why the backup
    /// came off, when it did.
    pub fn shown(&mut self, id: &str, to: Option<String>) -> Option<&'static str> {
        let entry = self.entries.get_mut(id)?;
        let off = entry.showing.is_some() && to.is_none();
        entry.showing = to;
        if !off {
            return None;
        }
        Some(entry.leaving.take().unwrap_or("backup_down"))
    }

    /// How long `id` has been without a picture, while it has a rule.
    pub fn lost_for(&self, id: &str, now: Instant) -> Option<Duration> {
        self.entries.get(id)?.watch.lost_for(now)
    }

    /// `backup` on the source's status.
    pub fn state(&self, id: &str, now: Instant, live: &impl Liveness) -> Option<BackupState> {
        let entry = self.entries.get(id)?;
        let rule = entry.rule.as_ref()?;
        let phase = entry.watch.phase(rule);
        let ms = |d: Duration| d.as_millis() as u64;
        Some(BackupState {
            phase,
            showing: entry.showing.clone(),
            lost_ms: entry.watch.lost_for(now).map(ms),
            switch_in_ms: entry.watch.switch_in(rule, now).map(ms),
            return_in_ms: entry.watch.return_in(rule, now).map(ms),
            warning: self.warning(id, phase, entry.showing.is_some(), live),
            never_live: phase != BackupPhase::Ready && self.never_live(id),
        })
    }

    fn warning(&self, id: &str, phase: BackupPhase, showing: bool, live: &impl Liveness) -> Option<String> {
        let backup = self.rule(id)?.backup.clone();
        let down_too = || self.cover(id, live).is_none();
        match phase {
            BackupPhase::OnBackup | BackupPhase::Returning | BackupPhase::MainBack if !showing => Some(format!(
                "{backup}, the backup, has no picture either, so {id}'s last frame or the slate is \
                 on air. Take another source, or pick a backup that is live."
            )),
            BackupPhase::Ready | BackupPhase::Lost if down_too() => Some(format!(
                "{backup}, the backup, has no picture, so it could not stand in for {id} now. \
                 Pick a backup that is live."
            )),
            _ => None,
        }
    }
}
