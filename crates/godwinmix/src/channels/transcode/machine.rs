//! What this machine can decode and encode, read from the codec catalogue.
//!
//! The catalogue is data (`codecs.toml`), and which of its entries this
//! machine can run is a question for the GStreamer registry. Both are asked
//! once, the first time a destination wants a rendition. An entry taken out
//! with `GMX_CODEC_DISABLE` is not here, which is how a run is made CPU only
//! on a machine that has hardware encoders.

use godwinmix_core::catalogue::model::{AudioEntry, Role, VideoEntry};
use godwinmix_core::catalogue::select::Registry;
use godwinmix_core::catalogue::Catalogue;
use godwinmix_protocol::rendition::{AudioCodec, EncoderSlot, VideoCodec};

use super::source::{audio_codec, video_codec};

/// The catalogue entries this machine has, best first within each list.
#[derive(Debug, Clone, Default)]
pub struct Machine {
    pub encoders: Vec<VideoEntry>,
    pub decoders: Vec<VideoEntry>,
    pub audio_encoders: Vec<AudioEntry>,
    pub audio_decoders: Vec<AudioEntry>,
}

fn present(needs: Vec<String>, reg: &dyn Registry) -> bool {
    !needs.is_empty() && needs.iter().all(|n| reg.has(n))
}

impl Machine {
    pub fn probe(cat: &Catalogue, reg: &dyn Registry) -> Machine {
        let video = |role: Role| {
            let mut v: Vec<VideoEntry> = cat
                .video
                .iter()
                .filter(|e| !e.disabled && e.element(role).is_some() && present(e.needs(role), reg))
                .cloned()
                .collect();
            v.sort_by_key(|e| std::cmp::Reverse(e.rank));
            v
        };
        let audio = |role: Role| {
            let mut v: Vec<AudioEntry> = cat
                .audio
                .iter()
                .filter(|e| !e.disabled && e.element(role).is_some() && present(e.needs(role), reg))
                .cloned()
                .collect();
            v.sort_by_key(|e| std::cmp::Reverse(e.rank));
            v
        };
        Machine {
            encoders: video(Role::Encode),
            decoders: video(Role::Decode),
            audio_encoders: audio(Role::Encode),
            audio_decoders: audio(Role::Decode),
        }
    }

    /// Every video encoder as the planner names it, best first.
    pub fn slots(&self) -> Vec<EncoderSlot> {
        self.encoders.iter().map(slot).collect()
    }

    pub fn encoder(&self, id: &str) -> Option<&VideoEntry> {
        self.encoders.iter().find(|e| e.id() == id)
    }

    /// The best decoder for a codec.
    pub fn decoder(&self, codec: VideoCodec) -> Option<&VideoEntry> {
        self.decoders.iter().find(|e| video_codec(&e.codec) == codec)
    }

    /// Whether the best decoder for a codec runs on the GPU or a media engine.
    pub fn decodes_in_hardware(&self, codec: VideoCodec) -> bool {
        self.decoder(codec).is_some_and(|e| e.accel != "software")
    }

    pub fn audio_encoder(&self, codec: AudioCodec) -> Option<&AudioEntry> {
        self.audio_encoders.iter().find(|e| audio_codec(&e.codec) == codec)
    }

    pub fn audio_decoder(&self, codec: AudioCodec) -> Option<&AudioEntry> {
        self.audio_decoders.iter().find(|e| audio_codec(&e.codec) == codec)
    }
}

/// An entry as an encoder slot: hardware when it is not `software`, counted
/// against a device named by its acceleration (`videotoolbox`, `nvidia`).
pub fn slot(e: &VideoEntry) -> EncoderSlot {
    let hardware = e.accel != "software";
    EncoderSlot {
        id: e.id(),
        codec: video_codec(&e.codec),
        hardware,
        device: hardware.then(|| e.accel.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use godwinmix_core::catalogue::select::FakeRegistry;

    #[test]
    fn only_what_is_installed_and_not_disabled_is_offered() {
        let mut cat = Catalogue::shipped().unwrap();
        let reg = FakeRegistry::with(&["vtenc_h264_hw", "vtdec_hw", "h264parse", "x264enc", "avdec_h264", "avenc_aac", "avdec_aac", "aacparse"]);
        let m = Machine::probe(&cat, &reg);
        let slots = m.slots();
        assert_eq!(slots[0].id, "h264-videotoolbox", "{slots:?}");
        assert!(slots[0].hardware && slots[0].device.as_deref() == Some("videotoolbox"));
        assert!(slots.iter().any(|s| s.id == "h264-software-x264" && !s.hardware));
        assert!(m.decodes_in_hardware(VideoCodec::H264));
        assert!(m.audio_encoder(AudioCodec::Aac).is_some());

        cat.video.iter_mut().filter(|e| e.accel == "videotoolbox").for_each(|e| e.disabled = true);
        let cpu = Machine::probe(&cat, &reg);
        assert!(cpu.slots().iter().all(|s| !s.hardware), "{:?}", cpu.slots());
        assert!(!cpu.decodes_in_hardware(VideoCodec::H264));
    }
}
