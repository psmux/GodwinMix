//! What the operator, the event stream and the hooks are told about a swap.

use super::super::Mixer;
use super::Seen;
use crate::failover::state::Swap;
use crate::failover::Liveness;
use crate::state::{Event, Severity};
use std::time::Instant;
use tracing::info;

impl Mixer {
    /// One stand in changed: an event for every client and a line a person
    /// reads in the alerts.
    pub(super) fn tell_swap(&self, swap: &Swap, reason: Option<&'static str>, transition: &str) {
        let id = &swap.source;
        let lost = self.backups.lost_for(id, Instant::now()).map(|d| d.as_millis() as u64).unwrap_or(0);
        let (event, severity, message) = match (&swap.from, &swap.to, reason) {
            (None, Some(to), _) => (
                Event::SourceBackupOn {
                    source: id.clone(),
                    backup: to.clone(),
                    transition: transition.to_string(),
                    lost_ms: lost,
                },
                Severity::Warning,
                format!("{} so {to} is on in its place.", self.no_picture(id, lost / 1000)),
            ),
            (Some(from), Some(to), _) => (
                Event::SourceBackupOn {
                    source: id.clone(),
                    backup: to.clone(),
                    transition: transition.to_string(),
                    lost_ms: lost,
                },
                Severity::Warning,
                format!("{from} has no picture either, so {to} is on in {id}'s place now."),
            ),
            (Some(from), None, reason) => {
                let reason = reason.unwrap_or("backup_down");
                let (severity, message) = match reason {
                    "backup_down" => (
                        Severity::Error,
                        format!(
                            "{from} went down too and nothing else can stand in for {id}, so its \
                             last frame or the slate is on air. Take another source."
                        ),
                    ),
                    "cleared" => (Severity::Info, format!("{id} has no backup now and is drawn as itself.")),
                    _ => (Severity::Info, format!("{id} is back on air in place of {from}.")),
                };
                let event =
                    Event::SourceBackupOff { source: id.clone(), backup: from.clone(), reason: reason.to_string() };
                (event, severity, message)
            }
            (None, None, _) => return,
        };
        info!(source = %id, from = ?swap.from, to = ?swap.to, transition, "{message}");
        let _ = self.events.send(event);
        let _ = self.events.send(Event::Alert { severity, message, action: None });
    }

    /// The backup went on and none of the chain has a picture: say so, once.
    pub(super) fn warn_if_uncovered(&self, id: &str, now: Instant) {
        let seen = Seen(&self.sources, &self.pending, &self.unstarted);
        if self.backups.chain(id).iter().any(|b| seen.live(b)) {
            return;
        }
        let backup = self.backups.rule(id).map(|r| r.backup.clone()).unwrap_or_default();
        let lost = self.backups.lost_for(id, now).map(|d| d.as_secs()).unwrap_or(0);
        info!(source = %id, backup = %backup, "a source went down and its backup has no picture either");
        let _ = self.events.send(Event::Alert {
            severity: Severity::Error,
            message: format!(
                "{} and its backup {backup} has no picture either, so {}. Take another source now.",
                self.no_picture(id, lost).trim_end_matches(','),
                match self.backups.never_live(id) {
                    true => "the slate is on air where it is placed".to_string(),
                    false => format!("{id}'s last frame stays up and then the slate"),
                }
            ),
            action: None,
        });
    }

    /// The start of every line about a source without a picture, ending in a
    /// comma: one that went down, or one that never came up.
    fn no_picture(&self, id: &str, secs: u64) -> String {
        if self.backups.never_live(id) {
            format!("{id} has not connected in {secs} s,")
        } else {
            format!("{id} has had no picture for {secs} s,")
        }
    }
}
