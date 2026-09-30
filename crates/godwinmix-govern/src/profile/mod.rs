//! A machine's costs, read from its calibration: what an encode, a scale, a
//! decode or an audio encode would take here.
//!
//! The method names and arguments are the planner's `CostModel` trait
//! (`encoders`, `encode_cost`, `scale_cost`, `decode_cost`, `audio_cost`), so
//! plugging this in is one `impl` that calls through.

mod estimate;
mod presets;

use crate::calibration::{mpix, Calibration, EncoderCal};
use crate::fit::Line;
use estimate::{memory_for, Guess};
use godwinmix_protocol::rendition::{AudioShape, Cost, EncoderSlot, VideoCodec, VideoShape};

/// One calibrated encoder, turned into lines.
#[derive(Debug, Clone)]
struct Enc {
    cal: EncoderCal,
    cpu: Line,
    wall: Line,
    /// Speed presets, fastest first, as a ratio to the configured one.
    presets: Vec<(String, f64)>,
}

#[derive(Debug, Clone, Default)]
pub struct Profile {
    cal: Calibration,
    encs: Vec<Enc>,
}

impl Profile {
    pub fn from_calibration(cal: Calibration) -> Profile {
        let margin = if cal.software_margin > 0.0 { cal.software_margin } else { 1.0 };
        let encs = cal.encoders.iter().filter_map(|e| enc_of(e, margin)).collect();
        Profile { cal, encs }
    }

    /// Before anything is measured: no encoder is known, and every cost is
    /// a cautious figure for software on a laptop.
    pub fn uncalibrated() -> Profile {
        Profile::default()
    }

    pub fn is_calibrated(&self) -> bool {
        !self.encs.is_empty()
    }

    pub fn calibration(&self) -> &Calibration {
        &self.cal
    }

    /// The encoders for a codec, the one to use first first: hardware ahead
    /// of software, then the cheapest on the CPU.
    pub fn encoders(&self, codec: VideoCodec) -> Vec<EncoderSlot> {
        let ref_mpix = mpix(1920, 1080, godwinmix_protocol::rendition::Fps::whole(30));
        let mut v: Vec<&Enc> = self.encs.iter().filter(|e| e.cal.slot.codec == codec).collect();
        v.sort_by(|a, b| {
            b.cal.slot.hardware.cmp(&a.cal.slot.hardware).then(a.cpu.at(ref_mpix).total_cmp(&b.cpu.at(ref_mpix)))
        });
        v.into_iter().map(|e| e.cal.slot.clone()).collect()
    }

    /// Every encoder calibrated, in no particular order.
    pub fn all_encoders(&self) -> Vec<EncoderSlot> {
        self.encs.iter().map(|e| e.cal.slot.clone()).collect()
    }

    /// Encoding `shape` on `slot` at the catalogue's own settings.
    pub fn encode_cost(&self, slot: &EncoderSlot, shape: &VideoShape) -> Cost {
        self.encode_cost_at(slot, shape, None)
    }

    /// The same at a named speed preset. An unknown preset is the
    /// configured one.
    pub fn encode_cost_at(&self, slot: &EncoderSlot, shape: &VideoShape, preset: Option<&str>) -> Cost {
        let x = mpix(shape.width, shape.height, shape.fps);
        let memory_mib = memory_for(shape);
        let Some(e) = self.enc(slot) else {
            return Cost { cpu_millicores: Guess::encode(shape.codec, x), memory_mib, ..Cost::default() };
        };
        let ratio = preset.and_then(|p| e.presets.iter().find(|(n, _)| n == p)).map(|(_, r)| *r).unwrap_or(1.0);
        let cpu = (e.cpu.at(x) * ratio).round() as u32;
        if !slot.hardware {
            return Cost { cpu_millicores: cpu.max(1), memory_mib, ..Cost::default() };
        }
        Cost {
            cpu_millicores: cpu,
            device_millis: (e.wall.at(x).round() as u32).clamp(1, 1000),
            device_sessions: 1,
            memory_mib,
            egress_kbps: 0,
        }
    }

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

    /// How many sessions `device` opens at once, when it refused during
    /// calibration. None means no limit was found.
    pub fn session_limit(&self, device: &str) -> Option<u32> {
        self.encs
            .iter()
            .filter(|e| e.cal.slot.device.as_deref() == Some(device))
            .filter_map(|e| e.cal.sessions.and_then(|s| s.limit()))
            .min()
    }

    /// Every hardware device calibration saw.
    pub fn devices(&self) -> Vec<String> {
        let mut v: Vec<String> = self.encs.iter().filter_map(|e| e.cal.slot.device.clone()).collect();
        v.sort();
        v.dedup();
        v
    }

    fn enc(&self, slot: &EncoderSlot) -> Option<&Enc> {
        self.encs.iter().find(|e| e.cal.slot.id == slot.id)
    }
}

fn enc_of(cal: &EncoderCal, margin: f64) -> Option<Enc> {
    let m = if cal.slot.hardware { 1.0 } else { margin };
    let cpu = Line::through(&cal.points.iter().map(|p| (p.mpix(), f64::from(p.cpu_millicores) * m)).collect::<Vec<_>>())?;
    let wall = Line::through(&cal.points.iter().map(|p| (p.mpix(), f64::from(p.wall_ms))).collect::<Vec<_>>())?;
    let presets = cal
        .presets
        .iter()
        .map(|pp| {
            let base = cpu.at(pp.point.mpix()).max(1.0);
            (pp.preset.clone(), f64::from(pp.point.cpu_millicores) * m / base)
        })
        .collect();
    Some(Enc { cal: cal.clone(), cpu, wall, presets })
}

#[cfg(test)]
mod tests;
