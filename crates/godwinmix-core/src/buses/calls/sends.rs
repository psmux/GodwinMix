//! The calls that change what is on a bus rather than the bus: sends and
//! cue marks.

use super::Around;
use crate::buses::check::{self, Refusal};
use crate::buses::desk::Desk;
use godwinmix_protocol::buses::{BusConfig, CueSetRequest, SendSetRequest};

impl Desk {
    pub(super) fn send(&mut self, req: SendSetRequest, around: &Around<'_>) -> Result<(), Refusal> {
        let Some(mut cfg) = self.config.buses.iter().find(|b| b.id == req.bus).cloned() else {
            return Err(self.no_bus(&req.bus));
        };
        if !around.has_source(&req.source) {
            return Err(around.no_source(&req.source));
        }
        let level = check::level("level", req.level)?;
        if cfg.kind() != "sends" {
            return Err(Refusal::Invalid {
                field: "bus",
                message: format!(
                    "bus {} is a {} bus and follows the programme, so it has no sends. Send to a bus \
                     with neither `minus` nor `cue`.",
                    cfg.id,
                    cfg.kind()
                ),
            });
        }
        if level == 0.0 {
            cfg.sends.remove(&req.source);
        } else {
            cfg.sends.insert(req.source.clone(), level);
        }
        self.live(&cfg.id)?.shared.lock().sends = cfg.sends.clone();
        self.store(cfg);
        Ok(())
    }

    pub(super) fn mark_cue(&mut self, req: CueSetRequest, around: &Around<'_>) -> Result<(), Refusal> {
        if !around.has_source(&req.source) {
            return Err(around.no_source(&req.source));
        }
        let mut marks = self.cue.lock();
        if req.solo {
            marks.clear();
        }
        marks.retain(|m| m != &req.source);
        if req.cue {
            marks.push(req.source);
        }
        Ok(())
    }

    pub(super) fn store(&mut self, cfg: BusConfig) {
        if let Some(slot) = self.config.buses.iter_mut().find(|b| b.id == cfg.id) {
            *slot = cfg;
        }
    }
}
