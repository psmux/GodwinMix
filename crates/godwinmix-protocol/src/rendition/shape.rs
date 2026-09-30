//! The concrete shape of a picture and a sound, and the codecs and
//! containers that name them.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum VideoCodec {
    H264,
    H265,
    Av1,
    Vp8,
    Vp9,
    Mpeg2,
    Prores,
    /// Anything the decoders know that has no name here yet. Never produced.
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum AudioCodec {
    Aac,
    Opus,
    Mp3,
    Ac3,
    Pcm,
    Other,
}

/// How the bytes leave. Decides which codecs are allowed: FLV carries H.264
/// (and HEVC and AV1 in enhanced RTMP), WebRTC wants VP8, VP9, H.264 or AV1.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Container {
    #[default]
    Flv,
    MpegTs,
    Mp4Fragmented,
    Mkv,
    Hls,
    LlHls,
    Dash,
    Rtp,
    Webrtc,
}

/// A frame rate as a fraction, so 29.97 is exact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct Fps {
    pub num: u32,
    pub den: u32,
}

impl Fps {
    pub fn whole(n: u32) -> Fps {
        Fps { num: n, den: 1 }
    }

    pub fn as_f64(self) -> f64 {
        f64::from(self.num) / f64::from(self.den.max(1))
    }
}

/// A picture as it is: codec, size, rate, bitrate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct VideoShape {
    pub codec: VideoCodec,
    pub width: u32,
    pub height: u32,
    pub fps: Fps,
    /// Measured or configured. 0 when unknown (a raw source).
    pub bitrate_kbps: u32,
    /// 0 when unknown.
    pub keyframe_ms: u32,
}

/// A sound as it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct AudioShape {
    pub codec: AudioCodec,
    pub channels: u8,
    pub sample_rate: u32,
    /// 0 when unknown.
    pub bitrate_kbps: u32,
}
