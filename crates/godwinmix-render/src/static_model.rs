//! Fixed costs for a machine with no calibration yet, and for tests.
//!
//! The numbers are rough figures for a recent laptop CPU, scaled by pixel
//! rate from 1080p30. x264 at `veryfast` on 1080p30 is about one and a half
//! cores; x265 and SVT-AV1 cost three to four times that; a hardware encoder
//! costs a tenth of a core for the upload and about a sixth of its device.
//! The governor replaces all of this with what it measured.

use std::collections::BTreeMap;

use godwinmix_protocol::rendition::{
    AudioCodec, AudioShape, Cost, EncoderSlot, VideoCodec, VideoShape,
};

use crate::model::{AudioWork, CostModel, Room};

/// Pixels per second of 1080p30, the unit the figures below are for.
const REF_PIXEL_RATE: f64 = 1920.0 * 1080.0 * 30.0;

/// Widest or tallest picture the static model lets a hardware encoder make.
const HW_MAX_SIDE: u32 = 4096;

/// A cost model from fixed numbers.
#[derive(Debug, Clone)]
pub struct StaticCostModel {
    pub encoders: Vec<EncoderSlot>,
    pub audio: Vec<AudioCodec>,
    pub rooms: BTreeMap<String, Room>,
}

impl StaticCostModel {
    /// Software encoders only, as a machine with no GPU has: x264, x265,
    /// SVT-AV1, libvpx for VP8 and VP9, AAC and Opus.
    pub fn software() -> StaticCostModel {
        let sw = |id: &str, codec| EncoderSlot {
            id: id.into(),
            codec,
            hardware: false,
            device: None,
        };
        StaticCostModel {
            encoders: vec![
                sw("h264-software-x264", VideoCodec::H264),
                sw("h265-software", VideoCodec::H265),
                sw("av1-software", VideoCodec::Av1),
                sw("vp8-software", VideoCodec::Vp8),
                sw("vp9-software", VideoCodec::Vp9),
            ],
            audio: vec![AudioCodec::Aac, AudioCodec::Opus],
            rooms: BTreeMap::new(),
        }
    }

    /// Adds a hardware encoder on `device`, in front of the others.
    pub fn with_hardware(mut self, id: &str, codec: VideoCodec, device: &str) -> Self {
        let slot = EncoderSlot {
            id: id.into(),
            codec,
            hardware: true,
            device: Some(device.into()),
        };
        self.encoders.insert(0, slot);
        self
    }

    /// Says how much is left on `device`.
    pub fn with_room(mut self, device: &str, room: Room) -> Self {
        self.rooms.insert(device.into(), room);
        self
    }

    /// Takes every encoder of `codec` away, to model a machine without one.
    pub fn without(mut self, codec: VideoCodec) -> Self {
        self.encoders.retain(|e| e.codec != codec);
        self
    }
}

fn load(shape: &VideoShape) -> f64 {
    f64::from(shape.width) * f64::from(shape.height) * shape.fps.as_f64() / REF_PIXEL_RATE
}

fn millicores(per_ref: f64, shape: &VideoShape) -> u32 {
    (per_ref * load(shape)).ceil() as u32
}

/// Software encode cost of 1080p30 in millicores, per codec.
fn software_ref(codec: VideoCodec) -> Option<f64> {
    match codec {
        VideoCodec::H264 => Some(1500.0),
        VideoCodec::H265 => Some(6000.0),
        VideoCodec::Av1 => Some(4500.0),
        VideoCodec::Vp8 => Some(2200.0),
        VideoCodec::Vp9 => Some(4500.0),
        _ => None,
    }
}

impl CostModel for StaticCostModel {
    fn encoders(&self) -> Vec<EncoderSlot> {
        self.encoders.clone()
    }

    fn encode_cost(&self, shape: &VideoShape, enc: &EncoderSlot) -> Option<Cost> {
        let mem = 32 + (96.0 * load(shape)).ceil() as u32;
        if enc.hardware {
            if shape.width > HW_MAX_SIDE || shape.height > HW_MAX_SIDE {
                return None;
            }
            let device_millis = millicores(160.0, shape).max(1);
            let cpu_millicores = millicores(100.0, shape);
            return Some(Cost {
                cpu_millicores,
                device_millis,
                device_sessions: 1,
                memory_mib: mem,
                egress_kbps: 0,
            });
        }
        let per_ref = software_ref(enc.codec)?;
        Some(Cost {
            cpu_millicores: millicores(per_ref, shape),
            memory_mib: mem * 2,
            ..Cost::default()
        })
    }

    fn scale_cost(&self, from: &VideoShape, to: &VideoShape) -> Cost {
        let cpu = millicores(60.0, from) + millicores(60.0, to);
        Cost {
            cpu_millicores: cpu,
            memory_mib: 8 + (24.0 * load(to)).ceil() as u32,
            ..Cost::default()
        }
    }

    fn decode_cost(&self, shape: &VideoShape) -> Cost {
        let per_ref = match shape.codec {
            VideoCodec::H264 | VideoCodec::Mpeg2 => 250.0,
            VideoCodec::H265 | VideoCodec::Vp9 => 400.0,
            VideoCodec::Av1 => 500.0,
            _ => 300.0,
        };
        Cost {
            cpu_millicores: millicores(per_ref, shape),
            memory_mib: 48,
            ..Cost::default()
        }
    }

    fn audio_cost(&self, shape: &AudioShape, work: AudioWork) -> Option<Cost> {
        let cpu = match work {
            AudioWork::Decode | AudioWork::Convert => 5,
            AudioWork::Encode if self.audio.contains(&shape.codec) => {
                15 + 2 * u32::from(shape.channels)
            }
            AudioWork::Encode => return None,
        };
        Some(Cost {
            cpu_millicores: cpu,
            memory_mib: 2,
            ..Cost::default()
        })
    }

    fn room(&self, device: &str) -> Room {
        self.rooms.get(device).copied().unwrap_or(Room::OPEN)
    }
}
