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

/// How a picture goes into a size of another shape.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Fit {
    /// Keep the whole picture and fill the rest with bars.
    #[default]
    Pad,
    /// Fill the frame and cut what overhangs.
    Crop,
    /// Fill the frame and change the picture's shape.
    Stretch,
}

/// Whether an interlaced picture is made progressive.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Deinterlace {
    #[default]
    Off,
    On,
    /// Only pictures the stream marks as interlaced.
    Auto,
}

/// How a deinterlace makes the lines of the field it does not have.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum DeinterlaceMethod {
    /// Looks at the fields before and after: no combing on movement and no
    /// flicker on still detail.
    #[default]
    Yadif,
    /// Motion adaptive, a little cheaper than YADIF and a little softer.
    Greedyh,
    /// The missing lines from the lines either side: the cheapest, and the
    /// softest, with some flicker on fine detail.
    Linear,
}

/// The shape a source's picture is shown at.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub enum SourceAspect {
    /// The shape the stream says.
    #[default]
    #[serde(rename = "auto")]
    Auto,
    /// Widescreen, whatever the stream says.
    #[serde(rename = "16:9")]
    Wide,
    /// Standard, whatever the stream says.
    #[serde(rename = "4:3")]
    Standard,
}

impl SourceAspect {
    /// The display aspect it forces, or None for the stream's own.
    pub fn ratio(self) -> Option<(u32, u32)> {
        match self {
            SourceAspect::Auto => None,
            SourceAspect::Wide => Some((16, 9)),
            SourceAspect::Standard => Some((4, 3)),
        }
    }
}

/// A rendition that follows a larger source's size, up to a limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MatchSource {
    /// The tallest picture made, such as 1080.
    pub max_height: u32,
    /// The bitrate at `max_height`. Left out, the rendition's own bitrate
    /// grown by the picture's area, so 1500 kbit/s at 720p is 3375 at 1080p.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bitrate_kbps: Option<u32>,
}

/// Where a rendition's picture is encoded, when it must be one or the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum EncoderPick {
    /// A hardware encoder: NVENC, Quick Sync, VideoToolbox or VA-API.
    Gpu,
    /// A software encoder, such as x264.
    Cpu,
}

/// Which NVIDIA GPU a rendition's decode, scale and encode run on: a CUDA
/// device index (0 is the first card), or `"auto"`, the card with the least
/// work when the channel starts. All three steps stay on that one card, so
/// a picture never crosses from one card's memory to another's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum GpuPick {
    /// A CUDA device index, as `gpu.status` numbers the cards.
    Index(u32),
    /// `"auto"`.
    Word(GpuWord),
}

/// The one word [`GpuPick`] takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum GpuWord {
    Auto,
}

impl GpuPick {
    pub const AUTO: GpuPick = GpuPick::Word(GpuWord::Auto);

    /// The card asked for, or None for `auto`.
    pub fn index(self) -> Option<u32> {
        match self {
            GpuPick::Index(i) => Some(i),
            GpuPick::Word(_) => None,
        }
    }
}

/// Which sound tracks a rendition converts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Tracks {
    #[default]
    First,
    All,
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
