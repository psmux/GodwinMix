//! The built in rendition presets, as `rendition.presets` lists them.
//!
//! Plain data: a preset fills the fields of a [`RenditionRequest`] the way a
//! person would, and anything it leaves out still means "whatever the source
//! has". Whether this machine can make one is the planner's question, not
//! this table's.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{AudioCodec, AudioWant, Container, Fps, RenditionRequest, VideoCodec, VideoWant};

/// One preset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RenditionPreset {
    /// `youtube-720p30`.
    pub id: String,
    /// What a menu shows: "YouTube 720p30".
    pub title: String,
    /// Which heading it sits under: `platform`, `ladder`, `audio`, `copy`.
    pub group: String,
    /// What one output gets. For a ladder, its top rung.
    pub request: RenditionRequest,
    /// Every rung, top first, when the preset is a ladder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ladder: Option<Vec<RenditionRequest>>,
}

impl RenditionPreset {
    pub fn is_ladder(&self) -> bool {
        self.ladder.is_some()
    }
}

/// H.264 at a size, rate and bitrate, with a two second keyframe interval
/// and AAC at 128 kbit/s, which is what every platform's ingest page asks for.
fn h264(id: &str, width: u32, height: u32, fps: u32, kbps: u32) -> RenditionRequest {
    RenditionRequest {
        id: id.into(),
        container: Container::Flv,
        video: Some(VideoWant {
            codec: Some(VideoCodec::H264),
            width: Some(width),
            height: Some(height),
            fps: Some(Fps::whole(fps)),
            bitrate_kbps: Some(kbps),
            bitrate_tolerance: None,
            keyframe_ms: Some(2000),
        }),
        audio: Some(aac()),
        no_video: false,
        no_audio: false,
    }
}

fn aac() -> AudioWant {
    AudioWant { codec: Some(AudioCodec::Aac), channels: None, sample_rate: None, bitrate_kbps: Some(128) }
}

fn preset(id: &str, title: &str, group: &str, request: RenditionRequest) -> RenditionPreset {
    RenditionPreset { id: id.into(), title: title.into(), group: group.into(), request, ladder: None }
}

fn ladder(id: &str, title: &str, rungs: Vec<RenditionRequest>) -> RenditionPreset {
    RenditionPreset {
        id: id.into(),
        title: title.into(),
        group: "ladder".into(),
        request: rungs[0].clone(),
        ladder: Some(rungs),
    }
}

/// Every built in preset, in the order a menu lists them.
pub fn rendition_presets() -> Vec<RenditionPreset> {
    let audio_only = RenditionRequest {
        id: "audio-only-aac".into(),
        audio: Some(aac()),
        no_video: true,
        ..RenditionRequest::default()
    };
    let copy = RenditionRequest { id: "copy".into(), ..RenditionRequest::default() };
    let r1080 = || h264("1080p", 1920, 1080, 30, 6000);
    let r720 = || h264("720p", 1280, 720, 30, 3000);
    let r480 = || h264("480p", 854, 480, 30, 1400);
    let r360 = || h264("360p", 640, 360, 30, 800);
    vec![
        preset("copy", "Send as it arrives", "copy", copy),
        preset("youtube-1080p30", "YouTube 1080p30", "platform", h264("youtube-1080p30", 1920, 1080, 30, 6000)),
        preset("youtube-720p30", "YouTube 720p30", "platform", h264("youtube-720p30", 1280, 720, 30, 3000)),
        preset("facebook-720p30", "Facebook 720p30", "platform", h264("facebook-720p30", 1280, 720, 30, 3000)),
        preset("twitch-1080p60", "Twitch 1080p60", "platform", h264("twitch-1080p60", 1920, 1080, 60, 6000)),
        preset("twitch-720p30", "Twitch 720p30", "platform", h264("twitch-720p30", 1280, 720, 30, 3000)),
        preset("audio-only-aac", "Sound only, AAC", "audio", audio_only),
        ladder("abr-ladder-4", "Ladder of four: 1080p, 720p, 480p, 360p", vec![r1080(), r720(), r480(), r360()]),
        ladder("abr-ladder-3", "Ladder of three: 720p, 480p, 360p", vec![r720(), r480(), r360()]),
    ]
}

/// One preset by id.
pub fn rendition_preset(id: &str) -> Option<RenditionPreset> {
    rendition_presets().into_iter().find(|p| p.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_contracts_presets_are_all_there_and_copy_asks_for_nothing() {
        let ids: Vec<String> = rendition_presets().into_iter().map(|p| p.id).collect();
        for id in [
            "youtube-1080p30",
            "youtube-720p30",
            "facebook-720p30",
            "twitch-1080p60",
            "twitch-720p30",
            "audio-only-aac",
            "abr-ladder-4",
            "abr-ladder-3",
            "copy",
        ] {
            assert!(ids.iter().any(|i| i == id), "{id}");
        }
        let copy = rendition_preset("copy").unwrap().request;
        assert!(copy.video.is_none() && copy.audio.is_none() && !copy.no_video);
        assert_eq!(rendition_preset("abr-ladder-4").unwrap().ladder.unwrap().len(), 4);
    }
}
