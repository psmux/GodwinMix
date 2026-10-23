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
    /// `cudaconvertscale` is here, so pictures an NVIDIA decoder made can be
    /// scaled and padded where they are, in the GPU's memory.
    pub cuda_scale: bool,
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
            // A converter ranks its decoders by `convert_rank`.
            v.sort_by_key(|e| std::cmp::Reverse(if role == Role::Decode { e.convert_rank() } else { e.rank }));
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
            cuda_scale: reg.has("cudaconvertscale"),
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

    /// The best encoder for a codec that takes this many channels.
    pub fn audio_encoder_for(&self, codec: AudioCodec, channels: u8) -> Option<&AudioEntry> {
        self.audio_encoders.iter().find(|e| audio_codec(&e.codec) == codec && e.max_channels.is_none_or(|m| channels <= m))
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

    fn h264_decoder(names: &[&str]) -> String {
        let m = Machine::probe(&Catalogue::shipped().unwrap(), &FakeRegistry::with(names));
        m.decoder(VideoCodec::H264).and_then(|e| e.decoder.clone()).unwrap_or_default()
    }

    /// openh264 refuses 4:4:4, 4:2:2, 10 bit and interlaced H.264, and a
    /// converter has no decodebin to fall back on: libav converts where it
    /// is installed, openh264 only where it is not, and NVDEC beats both.
    #[test]
    fn a_converter_decodes_h264_with_libav_before_openh264_and_nvdec_before_either() {
        let cpu = ["h264parse", "x264enc", "openh264enc", "openh264dec", "avdec_h264", "avenc_aac", "avdec_aac", "aacparse"];
        assert_eq!(h264_decoder(&cpu), "avdec_h264");
        assert_eq!(h264_decoder(&["h264parse", "openh264enc", "openh264dec"]), "openh264dec");
        let gpu: Vec<&str> = cpu.iter().copied().chain(["nvh264enc", "nvh264dec", "cudadownload"]).collect();
        assert_eq!(h264_decoder(&gpu), "nvh264dec");
        // The mixer's own selection, which decodebin backs, is not moved.
        let mixer: Vec<&str> = cpu.iter().copied().chain(["compositor", "videoconvert", "flvmux"]).collect();
        let sel = Catalogue::shipped().unwrap().select(&Default::default(), &FakeRegistry::with(&mixer)).unwrap();
        assert_eq!(sel.video_decode.element, "openh264dec");
    }
}
