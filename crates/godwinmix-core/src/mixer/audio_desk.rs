//! The mixer's half of `crate::buses`: the calls off the queue, the sources
//! joining and leaving, the outputs and taps a bus feeds, and saving it all.

use super::{Mixer, SourceId};
use crate::buses::{AudioCall, Around, Refusal};
use crate::config::OutputConfig;
use crate::plugin::branch::ProgrammeBranch;
use crate::state::Event;
use godwinmix_protocol::buses::AudioStatus;
use gstreamer as gst;
use tokio::sync::oneshot;

/// The answer to an `audio.*` call: the whole desk, or why nothing moved.
pub type AudioReply = oneshot::Sender<Result<AudioStatus, Refusal>>;

impl Mixer {
    /// One `audio.*` call. Saved and broadcast when it changed something.
    pub(super) fn audio_call(&mut self, call: AudioCall, reply: Option<AudioReply>) {
        let changes = !matches!(call, AudioCall::List);
        let saves = !matches!(
            call,
            AudioCall::List | AudioCall::Cue(_) | AudioCall::DeviceFound(_) | AudioCall::Talk(_) | AudioCall::Talker { .. }
        );
        let silence = match &call {
            AudioCall::Talker { source, on: true } => Some(source.clone()),
            _ => None,
        };
        // A bus nothing else reads loses its listeners before its tee goes.
        if let AudioCall::Remove(id) = &call {
            if !self.outputs_by_bus().iter().any(|(_, b)| b == id) {
                self.close_bus_taps(id);
            }
        }
        let outcome = {
            let outputs = self.outputs_by_bus();
            let branches: Vec<&ProgrammeBranch> = self.sources.iter().map(|s| &s.branch).collect();
            let outputs_on = move |bus: &str| outputs.iter().filter(|(_, b)| b == bus).map(|(o, _)| o.clone()).collect();
            let around = Around { branches: &branches, outputs_on: &outputs_on };
            self.desk.call(call, &around)
        };
        if outcome.is_ok() {
            if let Some(slot) = silence.and_then(|id| self.sources.iter().find(|s| s.input.id == id)) {
                slot.set_muted(true);
            }
            if saves {
                self.cfg.audio = self.desk.saved();
                self.persist_runtime();
            }
            if changes {
                self.broadcast_status();
            }
        }
        if let Some(reply) = reply {
            let _ = reply.send(outcome);
        }
    }

    /// Every output whose sound is a bus, attached or waiting, with the bus.
    fn outputs_by_bus(&self) -> Vec<(String, String)> {
        let attached = self.outputs.iter().map(|o| &o.cfg);
        attached
            .chain(self.held.configs())
            .chain(self.unattached.configs())
            .filter_map(|c| c.audio.clone().map(|b| (c.id.clone(), b)))
            .collect()
    }

    pub(super) fn audio_status(&self) -> AudioStatus {
        let outputs = self.outputs_by_bus();
        self.desk.status(&|bus| outputs.iter().filter(|(_, b)| b == bus).map(|(o, _)| o.clone()).collect())
    }

    /// A source that has just been added goes on every bus there is.
    pub(super) fn join_buses(&mut self, id: &SourceId) {
        if let Some(slot) = self.sources.iter().find(|s| &s.input.id == id) {
            self.desk.join_source(&slot.branch);
        }
    }

    /// A bus meter's peaks, as an event, when `element` is a bus meter.
    pub(super) fn bus_level(&self, element: &str, peak_db: &[f64]) -> bool {
        let Some(bus) = self.desk.meter_owner(element) else { return false };
        let _ = self.events.send(Event::BusAudioLevel { bus, peak_db: peak_db.to_vec() });
        true
    }

    /// The audio tee an output on `cfg.audio` links to, in place of the
    /// programme encoder's. None for an output on the programme.
    pub(super) fn bus_audio_for(&mut self, cfg: &OutputConfig) -> anyhow::Result<Option<gst::Element>> {
        let Some(bus) = &cfg.audio else { return Ok(None) };
        anyhow::ensure!(
            cfg.rendition.is_none() && !crate::hls::request::is_hls(cfg),
            "output {} asks for a rendition and for the sound of bus {bus}, and a rendition \
             encodes the programme's sound only so far. Leave out `audio` or the rendition",
            cfg.id
        );
        Ok(Some(self.desk.encoder_for(bus, &cfg.id)?))
    }

    /// The raw sound of a bus, for `/pcm/bus:<id>` and `/opus/bus:<id>`.
    pub(super) fn bus_tap_tee(&self, bus: &str) -> Result<gst::Element, String> {
        self.desk.tap_tee(bus).map_err(|e| e.to_string())
    }

    /// Close every monitoring branch on a bus that is going, before it goes.
    /// Its listeners see their stream end.
    fn close_bus_taps(&mut self, bus: &str) {
        let prefix = format!("{}/", crate::buses::listen_target(bus));
        self.audio_taps.retain(|key, _| !key.starts_with(&prefix));
    }
}
