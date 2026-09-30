//! What is not an encode: scaling, decoding, audio.

use super::estimate::{memory_for, Guess};
use super::Profile;
use crate::calibration::mpix;
use godwinmix_protocol::rendition::{AudioShape, Cost, VideoShape};

impl Profile {
    /// Scaling and converting `from` into `to`, on the CPU.
    pub fn scale_cost(&self, from: &VideoShape, to: &VideoShape) -> Cost {
        let x = mpix(from.width, from.height, from.fps) + mpix(to.width, to.height, to.fps);
        let per = self.cal.scale_per_mpix.unwrap_or(Guess::SCALE);
        Cost { cpu_millicores: (per * x).round() as u32, memory_mib: memory_for(to), ..Cost::default() }
    }

    /// Decoding `shape`, in software.
    pub fn decode_cost(&self, shape: &VideoShape) -> Cost {
        let x = mpix(shape.width, shape.height, shape.fps);
        let per = self
            .cal
            .decoders
            .iter()
            .find(|d| d.codec == shape.codec)
            .map(|d| d.per_mpix)
            .unwrap_or_else(|| Guess::decode(shape.codec));
        Cost { cpu_millicores: (per * x).round() as u32, memory_mib: memory_for(shape), ..Cost::default() }
    }

    /// One audio encode.
    pub fn audio_cost(&self, shape: &AudioShape) -> Cost {
        let measured = self.cal.audio.iter().find(|a| a.codec == shape.codec).map(|a| a.cpu_millicores);
        let stereo = measured.unwrap_or(Guess::AUDIO);
        let scaled = f64::from(stereo) * f64::from(shape.channels.max(1)) / 2.0 * f64::from(shape.sample_rate.max(8000))
            / 48_000.0;
        Cost { cpu_millicores: (scaled.round() as u32).max(1), memory_mib: 2, ..Cost::default() }
    }
}
