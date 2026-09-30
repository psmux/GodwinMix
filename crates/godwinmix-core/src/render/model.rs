//! The governor's view of this machine, as the planner's `CostModel`.
//!
//! The planner asks four things with the argument shapes of its own trait;
//! the governor's `Profile` answers them from calibration, or from cautious
//! figures before there is one. This is the thin adapter between the two.

use godwinmix_govern::headroom::UNLIMITED;
use godwinmix_govern::{Governor, Profile};
use godwinmix_protocol::rendition::{AudioCodec, AudioShape, Cost, EncoderSlot, VideoShape};
use godwinmix_render::{AudioWork, CostModel, Room};
use std::collections::BTreeMap;
use std::sync::Arc;

/// Widest or tallest side a hardware encoder is trusted with.
const HW_MAX_SIDE: u32 = 4096;

/// Resampling or remixing a stereo stream, thousandths of a core.
const AUDIO_CONVERT: u32 = 5;

pub struct GovernorModel {
    pub governor: Governor,
    profile: Arc<Profile>,
    /// The catalogue's encoders on this machine, in rank order.
    slots: Vec<EncoderSlot>,
    audio: Vec<AudioCodec>,
    /// What the running plan already holds on each device, given back when
    /// the planner asks what is left, so a replan does not find the GPU full
    /// of its own encoders.
    held: BTreeMap<String, Cost>,
}

impl GovernorModel {
    pub fn new(
        governor: Governor,
        slots: Vec<EncoderSlot>,
        audio: Vec<AudioCodec>,
        held: BTreeMap<String, Cost>,
    ) -> Self {
        let profile = governor.profile();
        GovernorModel { governor, profile, slots, audio, held }
    }
}

impl CostModel for GovernorModel {
    fn encoders(&self) -> Vec<EncoderSlot> {
        if !self.profile.is_calibrated() {
            return self.slots.clone();
        }
        // An encoder calibration could not run is present but does not work.
        let measured = self.profile.all_encoders();
        self.slots
            .iter()
            .filter(|s| measured.iter().any(|m| m.id == s.id))
            .cloned()
            .collect()
    }

    fn encode_cost(&self, shape: &VideoShape, enc: &EncoderSlot) -> Option<Cost> {
        if enc.hardware && (shape.width > HW_MAX_SIDE || shape.height > HW_MAX_SIDE) {
            return None;
        }
        Some(self.profile.encode_cost(enc, shape))
    }

    fn scale_cost(&self, from: &VideoShape, to: &VideoShape) -> Cost {
        self.profile.scale_cost(from, to)
    }

    fn decode_cost(&self, shape: &VideoShape) -> Cost {
        self.profile.decode_cost(shape)
    }

    fn audio_cost(&self, shape: &AudioShape, work: AudioWork) -> Option<Cost> {
        match work {
            AudioWork::Encode if !self.audio.contains(&shape.codec) => None,
            AudioWork::Encode => Some(self.profile.audio_cost(shape)),
            AudioWork::Decode | AudioWork::Convert => Some(Cost {
                cpu_millicores: AUDIO_CONVERT,
                memory_mib: 1,
                ..Cost::default()
            }),
        }
    }

    fn room(&self, device: &str) -> Room {
        let have = self.governor.headroom(Some(device));
        let ours = self.held.get(device).copied().unwrap_or_default();
        let sessions = (have.device_sessions != UNLIMITED)
            .then(|| have.device_sessions.saturating_add(ours.device_sessions));
        Room {
            sessions,
            device_millis: Some(have.device_millis.saturating_add(ours.device_millis)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use godwinmix_govern::GovernorConfig;
    use godwinmix_protocol::rendition::{Fps, VideoCodec};

    fn slot(id: &str, hardware: bool) -> EncoderSlot {
        EncoderSlot {
            id: id.into(),
            codec: VideoCodec::H264,
            hardware,
            device: hardware.then(|| "videotoolbox".into()),
        }
    }

    fn model() -> GovernorModel {
        let g = Governor::with_machine(GovernorConfig::default(), Profile::uncalibrated(), 8, 16_384);
        GovernorModel::new(g, vec![slot("hw", true), slot("x264", false)], vec![AudioCodec::Aac], BTreeMap::new())
    }

    #[test]
    fn before_calibration_every_catalogue_encoder_is_offered() {
        let ids: Vec<String> = model().encoders().into_iter().map(|s| s.id).collect();
        assert_eq!(ids, ["hw", "x264"]);
    }

    #[test]
    fn a_picture_too_big_for_hardware_is_refused_there_and_priced_in_software() {
        let m = model();
        let big = VideoShape { codec: VideoCodec::H264, width: 7680, height: 4320, fps: Fps::whole(30), bitrate_kbps: 0, keyframe_ms: 0 };
        assert!(m.encode_cost(&big, &slot("hw", true)).is_none());
        assert!(m.encode_cost(&big, &slot("x264", false)).unwrap().cpu_millicores > 0);
    }

    #[test]
    fn audio_is_priced_only_for_codecs_this_machine_encodes() {
        let m = model();
        let shape = AudioShape { codec: AudioCodec::Opus, channels: 2, sample_rate: 48_000, bitrate_kbps: 96 };
        assert!(m.audio_cost(&shape, AudioWork::Encode).is_none());
        let aac = AudioShape { codec: AudioCodec::Aac, ..shape };
        assert!(m.audio_cost(&aac, AudioWork::Encode).is_some());
        assert!(m.audio_cost(&shape, AudioWork::Convert).is_some());
    }
}
