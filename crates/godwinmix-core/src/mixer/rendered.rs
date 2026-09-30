//! Where an output is attached: onto the programme encoder, as every output
//! always was, or onto the tees of the rendition it asked for. The mixer's
//! half of `render/`; everything that decides lives there.

use super::Mixer;
use crate::config::OutputConfig;
use crate::output::OutputSlot;
use crate::render::{Renditions, Station, Tap, PROGRAMME};
use crate::state::{Event, Severity};
use anyhow::Result;
use godwinmix_protocol::rendition::{AudioCodec, AudioShape, StreamInfo, VideoCodec, VideoShape};
use std::sync::Arc;
use tracing::info;

impl Mixer {
    /// Attach one output where it belongs. The rendition is planned and
    /// admitted first; a refusal leaves nothing behind.
    pub(super) fn attach_output(&mut self, cfg: &OutputConfig) -> Result<Arc<OutputSlot>> {
        if let Some(choice) = &cfg.rendition {
            match self.renditions.add(&cfg.id, choice) {
                Ok(Some(taps)) => return self.attach_to_rendition(cfg, taps),
                Ok(None) => {}
                Err(refusal) => return Err(anyhow::Error::new(refusal)),
            }
        }
        let slot = OutputSlot::attach(&self.program, &self.venc_tee, &self.aenc_tee, cfg, self.bus_tx.clone())?;
        self.hold_encoder_for(&cfg.id);
        Ok(slot)
    }

    fn attach_to_rendition(&mut self, cfg: &OutputConfig, taps: Vec<Tap>) -> Result<Arc<OutputSlot>> {
        let top = taps.first().cloned();
        let tees = top.as_ref().and_then(|t| t.video_tee.clone().zip(t.audio_tee.clone()));
        let Some((video, audio)) = tees else {
            self.drop_rendition(&cfg.id);
            anyhow::bail!(
                "output {} asked for a rendition without both picture and sound, which only an \
                 HLS output can take so far. Pick a rendition with both, or an HLS output",
                cfg.id
            );
        };
        let slot = OutputSlot::attach_to(&self.program, &video, &audio, cfg, self.bus_tx.clone(), taps);
        if slot.is_err() {
            self.drop_rendition(&cfg.id);
        } else {
            self.publish_plan();
        }
        slot
    }

    /// Take an output's rendition out of the plan, if it had one.
    pub(super) fn drop_rendition(&mut self, id: &str) {
        if self.renditions.has(id) {
            self.renditions.remove(id);
            self.publish_plan();
        }
    }

    fn publish_plan(&self) {
        let plan = self.renditions.shared_view().read().clone();
        let _ = self.events.send(Event::RenditionPlan { scope: PROGRAMME.into(), plan });
    }

    /// Whether anything goes out, for the governor: a calibration never
    /// starts while something is on air.
    pub(super) fn note_on_air(&self) {
        self.renditions.station().set_on_air(!self.outputs.is_empty());
    }

    /// Once a watchdog tick: what the governor shed, and what came back.
    pub(super) fn rendition_tick(&mut self) {
        let tick = self.renditions.tick();
        for note in &tick.shed {
            let _ = self.events.send(Event::Alert { severity: Severity::Warning, message: note.why.clone(), action: None });
            let _ = self.events.send(Event::GovernorShed { what: note.what.clone(), why: note.why.clone() });
        }
        for what in &tick.restored {
            info!(what = %what, "rendition back on air");
            let _ = self.events.send(Event::Alert { severity: Severity::Info, message: format!("The {what} is running again: there is room for it now."), action: None });
        }
        if !tick.shed.is_empty() || !tick.restored.is_empty() {
            self.publish_plan();
            self.broadcast_status();
        }
    }

    /// The rungs an output reads, top first. Empty for an output on the
    /// programme encoder. See `render/mod.rs`.
    pub fn rendition_taps(&self, output: &str) -> Vec<Tap> {
        self.renditions.taps(output)
    }

    pub fn renditions(&self) -> &Renditions {
        &self.renditions
    }

    /// The station's governor, in place of the one `build` made. Before
    /// `start`, so configured outputs are admitted by it.
    pub fn set_station(&mut self, station: Station) {
        self.renditions.set_station(station);
    }

    /// Why this output's rendition is stopped, while the governor has it so.
    pub(super) fn shed_reason(&self, output: &str) -> Option<String> {
        self.renditions.shed_reason(output)
    }
}

/// The programme as the planner sees it: raw frames at the canvas size and
/// raw samples, which must be encoded for any output.
pub(super) fn programme_info(cfg: &crate::config::Config) -> StreamInfo {
    StreamInfo {
        video: Some(VideoShape {
            codec: VideoCodec::Other,
            width: cfg.canvas.width.max(2) as u32,
            height: cfg.canvas.height.max(2) as u32,
            fps: godwinmix_protocol::rendition::Fps::whole(cfg.canvas.fps.max(1) as u32),
            bitrate_kbps: 0,
            keyframe_ms: 0,
        }),
        audio: Some(AudioShape {
            codec: AudioCodec::Pcm,
            channels: cfg.canvas.channels.clamp(1, 8) as u8,
            sample_rate: cfg.canvas.sample_rate.max(8000) as u32,
            bitrate_kbps: 0,
        }),
        encoded: false,
    }
}

/// The station a mixer starts with: the config's governor, nothing measured
/// yet, and no sampler. The binary replaces it with a running one.
pub(super) fn default_station(cfg: &crate::config::Config) -> Station {
    let governor = godwinmix_govern::Governor::new(cfg.governor.clone(), godwinmix_govern::Profile::uncalibrated());
    Station::with_governor(governor, &crate::catalogue::global(), cfg.hardware.encode)
}

#[cfg(test)]
mod tests;
