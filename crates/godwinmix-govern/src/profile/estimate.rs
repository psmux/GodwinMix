//! Figures used before a machine is measured, or for work calibration does
//! not time. Deliberately on the heavy side: a guess that admits too little
//! costs an output, one that admits too much costs the programme.
//!
//! Units are thousandths of a core per megapixel a second (1080p30 is 62
//! megapixels a second). They are rough figures for x264 `veryfast`, x265
//! `ultrafast`, SVT-AV1 preset 10 and the libav decoders on a modest four
//! core laptop, set high on purpose, and they are replaced by measured
//! ones as soon as calibration has run.

use godwinmix_protocol::rendition::{VideoCodec, VideoShape};

pub struct Guess;

impl Guess {
    pub const SCALE: f64 = 2.0;
    /// A stereo 48 kHz AAC or Opus encode.
    pub const AUDIO: u32 = 15;

    pub fn encode(codec: VideoCodec, mpix: f64) -> u32 {
        let per = match codec {
            VideoCodec::H264 => 22.0,
            VideoCodec::H265 => 60.0,
            VideoCodec::Av1 => 45.0,
            VideoCodec::Vp8 | VideoCodec::Vp9 => 35.0,
            _ => 30.0,
        };
        (per * mpix).round() as u32
    }

    pub fn decode(codec: VideoCodec) -> f64 {
        match codec {
            VideoCodec::H264 | VideoCodec::Mpeg2 | VideoCodec::Vp8 => 3.5,
            VideoCodec::Prores => 2.0,
            _ => 6.0,
        }
    }
}

/// Memory an encoder or a scaler holding a few frames of this picture adds:
/// eight frames of 4:2:0 plus a fixed eight MiB for the element itself.
pub fn memory_for(shape: &VideoShape) -> u32 {
    let frame = u64::from(shape.width) * u64::from(shape.height) * 3 / 2;
    ((frame * 8) / 1_048_576) as u32 + 8
}
