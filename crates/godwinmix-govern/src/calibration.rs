//! What calibration measured, as it is written to disk. Plain data: the
//! GStreamer half that fills it in is `calibrate`, and `profile` reads it.

use crate::fingerprint::Machine;
use godwinmix_protocol::rendition::{AudioCodec, EncoderSlot, Fps, VideoCodec};
use serde::{Deserialize, Serialize};

/// Bumped when the shape of the file changes, so an old file is measured
/// again rather than misread.
pub const FORMAT: u32 = 1;

/// One machine, measured.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Calibration {
    pub format: u32,
    /// What the file is keyed by. A different value on start means the
    /// hardware, the drivers or GStreamer changed, and the machine is measured
    /// again.
    pub fingerprint: String,
    pub machine: Machine,
    /// Seconds since the Unix epoch when it was taken.
    pub taken_unix: u64,
    /// How long the whole calibration took.
    pub took_ms: u64,
    pub encoders: Vec<EncoderCal>,
    /// Scaling and colour conversion, in thousandths of a core per megapixel
    /// per second of picture in plus picture out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale_per_mpix: Option<f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub decoders: Vec<DecoderCal>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub audio: Vec<AudioCal>,
    /// What software encode costs are multiplied by, for pictures harder
    /// than the test bars. 1 when absent.
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub software_margin: f64,
    /// What could not be measured and why, in words.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

fn one() -> f64 {
    1.0
}

fn is_one(v: &f64) -> bool {
    *v == 1.0
}

/// Camera noise costs a software encoder more than scrolling bars do: x264
/// `veryfast` at 1080p30 took 1.7 times the CPU on `videotestsrc
/// pattern=snow` (pure noise, the worst case) as on the scrolling bars, on
/// an M4 Pro. Real pictures sit between the two, so calibration asks for a
/// little under half of that difference on top.
pub const SOFTWARE_MARGIN: f64 = 1.3;

/// One encoder, measured at a few shapes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EncoderCal {
    pub slot: EncoderSlot,
    /// The GStreamer element, `x264enc`, `vtenc_h264_hw`.
    pub element: String,
    /// The speed preset the catalogue configures, where the element has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    /// At the catalogue's own settings, one per calibrated shape.
    pub points: Vec<Point>,
    /// Other speed presets, measured at the largest calibrated shape only
    /// and applied to the others by ratio. Fastest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub presets: Vec<PresetPoint>,
    /// What opening sessions until one was refused found. Hardware only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sessions: Option<Sessions>,
}

/// One timed encode.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub width: u32,
    pub height: u32,
    pub fps: Fps,
    /// CPU for one second of this picture, in thousandths of a core, with the
    /// test source's own cost taken off.
    pub cpu_millicores: u32,
    /// Wall time for one second of picture, with the source's taken off.
    /// For hardware this is how busy the device is at real time.
    pub wall_ms: u32,
}

impl Point {
    /// Megapixels per second, the unit costs scale by.
    pub fn mpix(&self) -> f64 {
        mpix(self.width, self.height, self.fps)
    }
}

/// Megapixels a second of a picture this size at this rate.
pub fn mpix(width: u32, height: u32, fps: Fps) -> f64 {
    f64::from(width) * f64::from(height) * fps.as_f64() / 1_000_000.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PresetPoint {
    pub preset: String,
    pub point: Point,
}

/// Hardware encoder sessions this device would open at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sessions {
    /// How many were open together when the probe stopped.
    pub opened: u32,
    /// The device refused the next one, so `opened` is its limit. False when
    /// the probe stopped at its cap, and the limit is at least `opened`.
    pub refused: bool,
}

impl Sessions {
    /// The limit to admit against: exact when the device refused, none when
    /// it never did.
    pub fn limit(&self) -> Option<u32> {
        self.refused.then_some(self.opened)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecoderCal {
    pub codec: VideoCodec,
    pub element: String,
    /// Thousandths of a core per megapixel per second decoded.
    pub per_mpix: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioCal {
    pub id: String,
    pub codec: AudioCodec,
    pub element: String,
    /// One stereo 48 kHz encode, in thousandths of a core.
    pub cpu_millicores: u32,
}
