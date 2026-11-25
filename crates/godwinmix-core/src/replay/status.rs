//! What `replay.get` says, and what `replay.start` accepts.

use super::angle::AngleBranches;
use super::{Desk, Refusal};
use godwinmix_protocol::replay::{AngleStatus, ReplayConfig, ReplayStatus, MAX_SECONDS};

impl Desk {
    pub fn status(&self) -> ReplayStatus {
        let angles = self.config.angles.iter().map(|id| self.angle_status(id)).collect();
        let events = self.shared.marks.lock().events.clone();
        ReplayStatus { on: self.config.is_on(), config: self.config.clone(), angles, events, now_ms: self.shared.now() / 1_000_000 }
    }

    fn angle_status(&self, id: &str) -> AngleStatus {
        let (held, bytes, gops) = self.shared.ring(id).map(|r| {
            let r = r.lock();
            (r.held(), r.bytes(), r.gop_count() as u32)
        }).unwrap_or_default();
        let branch = self.branches.get(id);
        let waiting = match (branch, gops) {
            (None, _) => Some(format!("source {id} is not here; it is recorded from when it is added")),
            (Some(_), 0) => Some("no picture has arrived yet".to_string()),
            _ => None,
        };
        AngleStatus {
            id: id.to_string(),
            held_secs: held as f64 / 1e9,
            bytes,
            gops,
            encoder: branch.map(|b| b.encoder.clone()).unwrap_or_default(),
            dropped: branch.map(AngleBranches::dropped).unwrap_or(0),
            waiting,
        }
    }
}

pub(super) fn check(cfg: &ReplayConfig) -> Result<(), Refusal> {
    if !(1..=MAX_SECONDS).contains(&cfg.seconds) {
        return Err(Refusal::invalid("seconds", format!("seconds must be 1 to {MAX_SECONDS}, not {}", cfg.seconds)));
    }
    if cfg.angles.is_empty() {
        return Err(Refusal::invalid("angles", "name at least one angle: a source id, or \"programme\". replay.stop turns replay off".into()));
    }
    for (i, a) in cfg.angles.iter().enumerate() {
        if a.is_empty() || a.len() > 64 || a.chars().any(char::is_whitespace) || cfg.angles[..i].contains(a) {
            return Err(Refusal::invalid("angles", format!("'{a}' is not an angle: give each source id once, without spaces")));
        }
    }
    Ok(())
}
