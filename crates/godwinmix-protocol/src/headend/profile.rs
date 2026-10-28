//! Transcode profiles: one conversion, named, that many channels take. A
//! few are built in; the rest are saved on the station by
//! `headend.profile.save`. Changing a profile changes every channel that
//! takes it, which is how a whole headend moves to another bitrate.

use crate::rendition::{AudioCodec, AudioWant, Deinterlace, Fit, Tracks, VideoCodec, VideoWant};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// One conversion. `video` and `audio` are a rendition's wants: left out,
/// that half is passed through as it comes, so a profile with neither is a
/// pass through.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TranscodeProfile {
    /// A slug. The built in ones are listed by `headend.channel.list`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    /// What the screen calls it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// True for a built in profile, which cannot be changed or removed. Set
    /// by the station; ignored when sent.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub builtin: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub video: Option<VideoWant>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio: Option<AudioWant>,
}

impl TranscodeProfile {
    /// Nothing converted.
    pub fn is_copy(&self) -> bool {
        self.video.is_none() && self.audio.is_none()
    }
}

/// `headend.profile.save`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProfileSaveRequest {
    pub profile: TranscodeProfile,
}

/// `headend.profile.remove`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProfileRemoveRequest {
    pub id: String,
    /// The profile the channels that take this one move to. Left out, a
    /// profile some channel takes is not removed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub move_to: Option<String>,
}

/// `headend.profile.save` and `headend.profile.remove`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ProfileChange {
    pub id: String,
    /// The channels that take it, whose shows follow the change.
    pub channels: Vec<String>,
}

fn sound() -> AudioWant {
    AudioWant { codec: Some(AudioCodec::Aac), bitrate_kbps: Some(128), tracks: Some(Tracks::All), ..AudioWant::default() }
}

fn h264(width: u32, height: u32, kbps: u32) -> VideoWant {
    VideoWant {
        codec: Some(VideoCodec::H264),
        width: Some(width),
        height: Some(height),
        fit: Some(Fit::Pad),
        bitrate_kbps: Some(kbps),
        keyframe_ms: Some(2000),
        deinterlace: Some(Deinterlace::Auto),
        ..VideoWant::default()
    }
}

fn builtin(id: &str, name: &str, video: Option<VideoWant>, audio: Option<AudioWant>) -> TranscodeProfile {
    TranscodeProfile { id: id.into(), name: name.into(), builtin: true, video, audio }
}

/// The id of the profile that passes the input through.
pub const COPY: &str = "copy";
/// The `profile` a channel names when it carries its own conversion.
pub const CUSTOM: &str = "custom";

/// The profiles every station has. Each picture one fits the picture into
/// its size with black bars, deinterlaces what the stream marks as
/// interlaced, puts a keyframe every two seconds and converts every sound
/// track to AAC at 128 kbit/s. None of them pins the encoder: the planner
/// takes a GPU first and the CPU when the GPU is full or missing.
pub fn builtins() -> Vec<TranscodeProfile> {
    vec![
        builtin(COPY, "Pass through", None, None),
        builtin("aac-sound", "Picture as it comes, sound to AAC", None, Some(sound())),
        builtin("h264-480p-800", "480p H.264, 800 kbit/s", Some(h264(854, 480, 800)), Some(sound())),
        builtin("h264-720p-1500", "720p H.264, 1.5 Mbit/s", Some(h264(1280, 720, 1500)), Some(sound())),
        builtin("h264-1080p-4000", "1080p H.264, 4 Mbit/s", Some(h264(1920, 1080, 4000)), Some(sound())),
    ]
}

