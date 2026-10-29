//! The mixer's half of `crate::replay`: the calls off the queue, the sources
//! joining and leaving, the programme encoder held while the programme is an
//! angle, and saving what replay is set to.

use super::{Command, Mixer, MixerHandle};
use crate::replay::{Refusal, ReplayCall, Shared, SourceTaps, Taps};
use gstreamer::prelude::*;
use godwinmix_protocol::replay::{ReplayEvent, ReplayStatus};
use std::sync::Arc;
use tokio::sync::oneshot;
use tracing::warn;

/// The answer to a `replay.*` call: everything, the event a mark made, or why
/// nothing moved.
pub type ReplayAnswer = Result<(ReplayStatus, Option<ReplayEvent>), Refusal>;
pub type ReplayReply = oneshot::Sender<ReplayAnswer>;

impl MixerHandle {
    /// One `replay.*` call on the mixer thread.
    pub async fn replay(&self, call: ReplayCall) -> anyhow::Result<ReplayAnswer> {
        let (tx, rx) = oneshot::channel();
        self.ask(Command::Replay(call, Some(tx)), rx).await
    }

    /// The pipeline behind a source, for calling its kind off the mixer
    /// thread. `None` when there is no such source.
    pub async fn source_input(&self, source: &str) -> anyhow::Result<Option<Arc<crate::input::InputPipeline>>> {
        let (tx, rx) = oneshot::channel();
        self.ask(Command::SourceInput { source: source.into(), reply: tx }, rx).await
    }

    /// The rings and events, for reading a clip off the mixer thread: a
    /// save writes from these without the mixer waiting on the disk.
    pub fn replay_shared(&self) -> Arc<Shared> {
        self.replay.clone()
    }
}

impl Mixer {
    pub(super) fn replay_call(&mut self, call: ReplayCall, reply: Option<ReplayReply>) {
        let saves = matches!(call, ReplayCall::Start(_) | ReplayCall::Stop);
        let answer = self.with_replay_taps(|desk, taps| desk.call(call, taps));
        if answer.is_ok() && saves {
            self.cfg.replay = self.replay.config.clone();
            self.persist_runtime();
        }
        self.sync_replay_lease();
        if let Some(reply) = reply {
            let _ = reply.send(answer);
        }
    }

    /// Replay as the config says, at start. A refusal is logged; the
    /// programme starts either way.
    pub(super) fn start_configured_replay(&mut self) {
        self.replay.shared().set_clock(self.program.clock());
        if !self.cfg.replay.is_on() {
            return;
        }
        let cfg = self.cfg.replay.clone();
        if let Err(e) = self.with_replay_taps(|desk, taps| desk.start(cfg, taps)) {
            warn!(%e, "replay in the config could not start");
        }
        self.sync_replay_lease();
    }

    /// A source that has just been added starts recording if it is an angle.
    pub(super) fn replay_join(&mut self, id: &str) {
        if self.replay.config.is_on() {
            self.with_replay_taps(|desk, taps| desk.join(id, taps));
        }
    }

    pub(super) fn replay_leave(&mut self, id: &str) {
        self.replay.leave(id);
    }

    /// The programme encoder runs while the programme is an angle.
    fn sync_replay_lease(&mut self) {
        let wanted = self.replay.records_programme();
        if wanted == self.replay_lease.is_some() {
            return;
        }
        self.replay_lease = wanted.then(|| self.enc.lease("replay"));
        self.sync_encoder();
        if wanted {
            // The encoder may have just started: the angle's tee pads exist
            // already, so the first keyframe lands in the ring.
            self.with_replay_taps(|desk, taps| desk.join(godwinmix_protocol::replay::PROGRAMME, taps));
        }
    }

    fn with_replay_taps<T>(&mut self, f: impl FnOnce(&mut crate::replay::Desk, &Taps) -> T) -> T {
        let Self { replay, sources, program, venc_tee, aenc_tee, .. } = self;
        let source = |id: &str| -> Option<SourceTaps> {
            let slot = sources.iter().find(|s| s.input.id == id)?;
            let (pipeline, vtee, atee) = slot.input.taps();
            // The sound tee is there whether or not sound has arrived yet; an
            // encoder on it that never gets any costs nothing.
            Some((pipeline, vtee, Some(atee)))
        };
        let taps = Taps { program, venc_tee, aenc_tee, source: &source };
        f(replay, &taps)
    }
}
