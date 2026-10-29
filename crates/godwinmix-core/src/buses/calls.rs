//! The `audio.*` calls, applied to the desk. Each one is checked whole before
//! anything moves, so a refused call changes nothing.

use super::check::{self, Refusal};
use super::desk::{checked, mode_of, Desk};
use super::AudioCall;
use crate::plugin::branch::ProgrammeBranch;
use godwinmix_protocol::buses::{AudioStatus, BusConfig, BusSetRequest, CueSetRequest, SendSetRequest};

/// What the mixer knows that the desk does not.
pub struct Around<'a> {
    pub branches: &'a [&'a ProgrammeBranch],
    /// The outputs whose sound is set to each bus, attached or not.
    pub outputs_on: &'a dyn Fn(&str) -> Vec<String>,
}

impl Around<'_> {
    fn has_source(&self, id: &str) -> bool {
        self.branches.iter().any(|b| b.id == id)
    }

    fn no_source(&self, id: &str) -> Refusal {
        Refusal::NotFound { kind: "source", id: id.to_string(), known: self.branches.iter().map(|b| b.id.clone()).collect() }
    }
}

impl Desk {
    /// Apply one call and answer with the whole desk.
    pub fn call(&mut self, call: AudioCall, around: &Around<'_>) -> Result<AudioStatus, Refusal> {
        match call {
            AudioCall::Master(req) => {
                let mut m = self.config.master;
                if let Some(g) = req.gain {
                    m.gain = check::level("gain", g)?;
                }
                m.muted = req.muted.unwrap_or(m.muted);
                self.config.master = m;
                self.apply_master();
            }
            AudioCall::Add(cfg) => self.add(cfg, around)?,
            AudioCall::Set(req) => self.set(req, around)?,
            AudioCall::Remove(id) => self.remove(&id, around)?,
            AudioCall::List => {}
            AudioCall::Send(req) => self.send(req, around)?,
            AudioCall::Cue(req) => self.mark_cue(req, around)?,
            AudioCall::DeviceFound(found) => self.device_found(found),
        }
        Ok(self.status(around.outputs_on))
    }

    fn add(&mut self, cfg: BusConfig, around: &Around<'_>) -> Result<(), Refusal> {
        checked(&cfg)?;
        if self.config.buses.iter().any(|b| b.id == cfg.id) {
            return Err(Refusal::Invalid {
                field: "id",
                message: format!("there is already a bus called {}. Change it with audio.bus.set, or pick another id.", cfg.id),
            });
        }
        for id in cfg.sends.keys().chain(cfg.minus.iter()) {
            if !around.has_source(id) {
                return Err(around.no_source(id));
            }
        }
        self.build(&cfg, around.branches)?;
        self.config.buses.push(cfg);
        Ok(())
    }

    fn set(&mut self, req: BusSetRequest, around: &Around<'_>) -> Result<(), Refusal> {
        let Some(mut cfg) = self.config.buses.iter().find(|b| b.id == req.id).cloned() else {
            return Err(self.no_bus(&req.id));
        };
        if let Some(g) = req.gain {
            cfg.gain = check::level("gain", g)?;
        }
        cfg.muted = req.muted.unwrap_or(cfg.muted);
        if let Some(name) = req.name {
            cfg.name = Some(name).filter(|n| !n.trim().is_empty());
        }
        if let Some(minus) = req.minus {
            if !minus.is_empty() && !around.has_source(&minus) {
                return Err(around.no_source(&minus));
            }
            cfg.minus = Some(minus).filter(|m| !m.is_empty());
        }
        checked(&cfg)?;
        let device = req.device.filter(|d| cfg.device.as_deref().unwrap_or("") != d);
        if let Some(d) = &device {
            cfg.device = Some(d.clone()).filter(|d| !d.is_empty());
        }
        let live = self.live(&cfg.id)?;
        live.graph.set_master(cfg.gain, cfg.muted);
        live.shared.lock().mode = mode_of(&cfg);
        if let Some(d) = device {
            self.open_device(&cfg.id, &d);
        }
        self.store(cfg);
        Ok(())
    }

    fn remove(&mut self, id: &str, around: &Around<'_>) -> Result<(), Refusal> {
        if !self.config.buses.iter().any(|b| b.id == id) {
            return Err(self.no_bus(id));
        }
        let users = (around.outputs_on)(id);
        if !users.is_empty() {
            return Err(Refusal::InUse {
                message: format!(
                    "bus {id} is the sound of {}. Point {} back at the programme with output.set \
                     (audio left out), or remove {}, then remove the bus.",
                    users.join(", "),
                    if users.len() == 1 { "it" } else { "them" },
                    if users.len() == 1 { "it" } else { "them" },
                ),
                by: users,
            });
        }
        self.tear_down(id);
        self.config.buses.retain(|b| b.id != id);
        Ok(())
    }

    fn send(&mut self, req: SendSetRequest, around: &Around<'_>) -> Result<(), Refusal> {
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

    fn mark_cue(&mut self, req: CueSetRequest, around: &Around<'_>) -> Result<(), Refusal> {
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

    fn store(&mut self, cfg: BusConfig) {
        if let Some(slot) = self.config.buses.iter_mut().find(|b| b.id == cfg.id) {
            *slot = cfg;
        }
    }
}
