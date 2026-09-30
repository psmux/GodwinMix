//! Renditions: what an output asks for, what a source has, and what doing the
//! difference costs. The shared words of `dev/plans/shows-and-renditions.md`.
//!
//! The planner (`godwinmix-render`), the governor (`godwinmix-govern`), the
//! frame bus and every transport speak these types and nothing else, so each
//! can be built, tested and replaced without the others.

mod shape;
mod table;
mod wire;

pub use table::events;

pub use shape::*;
pub use wire::*;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// What one output wants. A field left out means "whatever the source has",
/// so an empty request is a plain copy.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RenditionRequest {
    /// Slug, unique within the show or channel that asks.
    pub id: String,
    /// How the bytes are wrapped on the way out.
    pub container: Container,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub video: Option<VideoWant>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio: Option<AudioWant>,
    /// Drop the video altogether (an audio only stream).
    #[serde(default)]
    pub no_video: bool,
    /// Drop the audio altogether.
    #[serde(default)]
    pub no_audio: bool,
}

/// The video an output wants. Every field left out is taken from the source.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct VideoWant {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codec: Option<VideoCodec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fps: Option<Fps>,
    /// Target bitrate. A copy is kept when the source is within
    /// `bitrate_tolerance` of it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bitrate_kbps: Option<u32>,
    /// Fraction either way a source's bitrate may differ and still be copied.
    /// 0.25 when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bitrate_tolerance: Option<f32>,
    /// Keyframe interval. Renditions in one ladder share it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keyframe_ms: Option<u32>,
}

/// The audio an output wants. Every field left out is taken from the source.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AudioWant {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codec: Option<AudioCodec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channels: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample_rate: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bitrate_kbps: Option<u32>,
}

/// What a source actually carries, as the hub or a decoder reports it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StreamInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub video: Option<VideoShape>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio: Option<AudioShape>,
    /// Encoded (can be copied) or already raw frames (a camera, the
    /// programme), which must be encoded for any output.
    pub encoded: bool,
}

/// One encoder this machine has, as the codec catalogue names it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct EncoderSlot {
    /// The catalogue id, `h264-videotoolbox`, `x264`, `h265-nvidia`.
    pub id: String,
    pub codec: VideoCodec,
    /// On a GPU or a media engine, rather than on the CPU.
    pub hardware: bool,
    /// Which device, where a machine has more than one. None on the CPU.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
}

/// What running one piece of work costs, in units the governor adds up.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Cost {
    /// Thousandths of one CPU core. 1000 is one whole core.
    pub cpu_millicores: u32,
    /// Share of one hardware device, in thousandths of what it can do, when
    /// the work runs on one.
    pub device_millis: u32,
    /// Hardware encoder sessions held (consumer NVIDIA cards cap these).
    pub device_sessions: u32,
    /// Resident memory the work adds, in MiB.
    pub memory_mib: u32,
    /// Bytes per second out of the machine, in kbit/s.
    pub egress_kbps: u32,
}

impl Cost {
    pub fn plus(self, other: Cost) -> Cost {
        Cost {
            cpu_millicores: self.cpu_millicores + other.cpu_millicores,
            device_millis: self.device_millis + other.device_millis,
            device_sessions: self.device_sessions + other.device_sessions,
            memory_mib: self.memory_mib + other.memory_mib,
            egress_kbps: self.egress_kbps + other.egress_kbps,
        }
    }
}
