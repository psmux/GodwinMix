//! The planner's view of this machine: the encoders the catalogue says it
//! has, priced from the governor's calibration where there is one and from
//! the planner's fixed figures where there is not.
//!
//! An uncalibrated profile prices a hardware encoder as if it were software,
//! which would refuse a GPU encode for want of CPU. So a slot the profile has
//! not measured is priced by `StaticCostModel`, which knows a hardware
//! encoder costs a tenth of a core and a share of its device.

use std::collections::BTreeMap;
use std::sync::Arc;

use godwinmix_govern::headroom::UNLIMITED;
use godwinmix_govern::{Governor, Profile};
use godwinmix_protocol::rendition::{AudioCodec, AudioShape, Cost, EncoderSlot, VideoShape};
use godwinmix_render::{AudioWork, CostModel, Room, StaticCostModel};

use super::admit::Held;
use super::machine::Machine;

/// A hardware decode at 1080p30: the copy out of the media engine, in
/// thousandths of a core. A rough figure, rounded up, until calibration
/// times hardware decoders as well.
const HW_DECODE_1080P30: f64 = 60.0;

pub struct Model {
    slots: Vec<EncoderSlot>,
    audio: Vec<AudioCodec>,
    machine: Machine,
    profile: Arc<Profile>,
    fixed: StaticCostModel,
    rooms: BTreeMap<String, Room>,
}

impl Model {
    /// `rooms` is what is left on each hardware device for this plan, with
    /// what the plan being replaced holds already given back.
    pub fn new(machine: &Machine, profile: Arc<Profile>, rooms: BTreeMap<String, Room>) -> Model {
        let slots = machine.slots();
        let audio = [AudioCodec::Aac, AudioCodec::Opus, AudioCodec::Mp3, AudioCodec::Ac3]
            .into_iter()
            .filter(|c| machine.audio_encoder(*c).is_some())
            .collect();
        let fixed = StaticCostModel { encoders: slots.clone(), audio: Vec::new(), rooms: BTreeMap::new() };
        Model { slots, audio, machine: machine.clone(), profile, fixed, rooms }
    }

    fn measured(&self, enc: &EncoderSlot) -> bool {
        self.profile.is_calibrated() && self.profile.all_encoders().iter().any(|s| s.id == enc.id)
    }
}

fn pixel_load(shape: &VideoShape) -> f64 {
    f64::from(shape.width) * f64::from(shape.height) * shape.fps.as_f64() / (1920.0 * 1080.0 * 30.0)
}

impl CostModel for Model {
    fn encoders(&self) -> Vec<EncoderSlot> {
        self.slots.clone()
    }

    fn encode_cost(&self, shape: &VideoShape, enc: &EncoderSlot) -> Option<Cost> {
        if self.measured(enc) {
            return Some(self.profile.encode_cost(enc, shape));
        }
        self.fixed.encode_cost(shape, enc)
    }

    fn scale_cost(&self, from: &VideoShape, to: &VideoShape) -> Cost {
        self.profile.scale_cost(from, to)
    }

    fn decode_cost(&self, shape: &VideoShape) -> Cost {
        if self.machine.decodes_in_hardware(shape.codec) {
            let cpu = (HW_DECODE_1080P30 * pixel_load(shape)).ceil() as u32;
            return Cost { cpu_millicores: cpu.max(1), memory_mib: 48, ..Cost::default() };
        }
        self.profile.decode_cost(shape)
    }

    fn audio_cost(&self, shape: &AudioShape, work: AudioWork) -> Option<Cost> {
        match work {
            AudioWork::Decode | AudioWork::Convert => Some(Cost { cpu_millicores: 5, memory_mib: 2, ..Cost::default() }),
            AudioWork::Encode if self.audio.contains(&shape.codec) => Some(self.profile.audio_cost(shape)),
            AudioWork::Encode => None,
        }
    }

    fn room(&self, device: &str) -> Room {
        self.rooms.get(device).copied().unwrap_or(Room::OPEN)
    }
}

/// What is left on each hardware device for this channel's plan, with what
/// the channel already holds there given back, so a replan does not find the
/// device full of its own encoders.
pub fn rooms(gov: &Governor, held: &BTreeMap<String, Held>, machine: &Machine) -> BTreeMap<String, Room> {
    let mut out = BTreeMap::new();
    for device in machine.slots().into_iter().filter_map(|s| s.device) {
        let have = gov.headroom(Some(&device));
        let mine = held.values().filter(|h| h.node.device == device).fold((0, 0), |(m, s), h| {
            (m + h.ticket.cost().device_millis, s + h.ticket.cost().device_sessions)
        });
        let sessions = (have.device_sessions != UNLIMITED).then(|| have.device_sessions + mine.1);
        out.insert(device, Room { sessions, device_millis: Some(have.device_millis + mine.0) });
    }
    out
}
