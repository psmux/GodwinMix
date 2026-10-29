//! The replay desk on the mixer thread: which angles are recorded, the
//! branches that record them, and the calls that change either.

use super::angle::{self, AngleBranches, EncodeWith};
use super::{Refusal, ReplayCall, Shared};
use godwinmix_protocol::replay::{ReplayConfig, ReplayEvent, ReplayStatus, PROGRAMME};
use gstreamer as gst;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;
use tracing::{info, warn};

/// A source's pipeline and its picture and sound tees.
pub type SourceTaps = (gst::Pipeline, gst::Element, Option<gst::Element>);

/// Where the desk hangs its branches, lent by the mixer for one call.
pub struct Taps<'a> {
    pub program: &'a gst::Pipeline,
    pub venc_tee: &'a gst::Element,
    pub aenc_tee: &'a gst::Element,
    pub source: &'a dyn Fn(&str) -> Option<SourceTaps>,
}

pub struct Desk {
    pub config: ReplayConfig,
    pub(super) shared: Arc<Shared>,
    pub(super) branches: BTreeMap<String, AngleBranches>,
    with: EncodeWith,
}

impl Desk {
    pub fn new(origin: Instant, with: EncodeWith) -> Self {
        let shared = Arc::new(Shared::default());
        super::registry::register(origin, &shared);
        Self { config: ReplayConfig::default(), shared, branches: BTreeMap::new(), with }
    }

    pub fn shared(&self) -> &Arc<Shared> {
        &self.shared
    }

    /// Whether the programme encoder has to run for replay.
    pub fn records_programme(&self) -> bool {
        self.config.angles.iter().any(|a| a == PROGRAMME)
    }

    pub fn call(&mut self, call: ReplayCall, taps: &Taps) -> Result<(ReplayStatus, Option<ReplayEvent>), Refusal> {
        let event = match call {
            ReplayCall::Start(cfg) => {
                self.start(cfg, taps)?;
                None
            }
            ReplayCall::Stop => {
                self.stop();
                None
            }
            ReplayCall::Get => None,
            ReplayCall::Mark(req) => {
                if !self.config.is_on() {
                    return Err(Refusal::NotInState("replay is off, so there is nothing to mark. Turn it on with replay.start".into()));
                }
                let now_ms = self.shared.now() / 1_000_000;
                Some(self.shared.marks.lock().mark(&req, now_ms)?)
            }
            ReplayCall::Rename(req) => Some(self.shared.marks.lock().rename(&req.id, &req.name)?),
            ReplayCall::Delete(id) => {
                self.shared.marks.lock().delete(&id)?;
                None
            }
        };
        Ok((self.status(), event))
    }

    /// Record these angles. Angles already recorded keep their rings and
    /// branches; a source that is not here yet is joined when it comes.
    pub fn start(&mut self, cfg: ReplayConfig, taps: &Taps) -> Result<(), Refusal> {
        super::status::check(&cfg)?;
        self.branches.retain(|id, _| cfg.angles.contains(id));
        self.shared.set_angles(&cfg.angles, cfg.seconds);
        let rebuild = cfg.bitrate_kbps != self.config.bitrate_kbps;
        self.config = cfg;
        if rebuild {
            self.branches.retain(|id, _| id == PROGRAMME);
        }
        for id in self.config.angles.clone() {
            self.join(&id, taps);
        }
        info!(angles = ?self.config.angles, seconds = self.config.seconds, "replay is recording");
        Ok(())
    }

    /// Take every branch out, free every ring and forget the events.
    pub fn stop(&mut self) {
        self.branches.clear();
        self.shared.set_angles(&[], 1);
        *self.shared.marks.lock() = Default::default();
        if self.config.is_on() {
            info!("replay is off: every angle's branch is out and its memory freed");
        }
        self.config = ReplayConfig::default();
    }

    /// A source has joined, or replay has asked for it: start its branch.
    pub fn join(&mut self, id: &str, taps: &Taps) {
        if !self.config.angles.iter().any(|a| a == id) || self.branches.contains_key(id) {
            return;
        }
        let Some(ring) = self.shared.ring(id) else { return };
        let tag = format!("replay-{id}");
        let built = if id == PROGRAMME {
            angle::programme(taps.program, taps.venc_tee, taps.aenc_tee, &ring, &tag)
        } else if let Some((pipeline, vtee, atee)) = (taps.source)(id) {
            let with = self.with.clone().for_replay(self.config.bitrate_kbps);
            angle::source(&pipeline, &vtee, atee.as_ref(), &ring, &with, &tag)
        } else {
            return;
        };
        match built {
            Ok(b) => {
                info!(angle = %id, encoder = %b.encoder, "replay angle recording");
                self.branches.insert(id.to_string(), b);
            }
            Err(e) => warn!(angle = %id, ?e, "a replay angle could not be built; it is retried when the source comes back"),
        }
    }

    /// The source is leaving: its branch goes with its pipeline, its ring stays.
    pub fn leave(&mut self, id: &str) {
        self.branches.remove(id);
    }

    pub fn shutdown(&mut self) {
        self.branches.clear();
    }
}
