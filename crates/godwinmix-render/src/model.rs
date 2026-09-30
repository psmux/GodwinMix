//! What the planner asks of the machine. It never measures anything itself:
//! the governor answers from its calibration, and `StaticCostModel` answers
//! from fixed numbers where there is no calibration yet.

use godwinmix_protocol::rendition::{AudioShape, Container, Cost, EncoderSlot, VideoShape};
use serde::Serialize;

/// The device key work on the CPU is counted under in `Plan::cost`.
pub const CPU: &str = "cpu";

/// The device key a hardware encoder with no named device is counted under.
pub const GPU: &str = "gpu";

/// Which device an encoder's work is counted against: `cpu` for software,
/// the slot's `device` for hardware, `gpu` when a hardware slot names none.
pub fn device_of(enc: &EncoderSlot) -> &str {
    if !enc.hardware {
        return CPU;
    }
    enc.device.as_deref().unwrap_or(GPU)
}

/// What is left on one hardware device for the plan being made. `None` means
/// no limit the model knows of.
///
/// This must not count what the plan being replaced already holds on the
/// device, or replanning a running show would find the GPU full of its own
/// encoders and move them to software.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Room {
    /// Encoder sessions still free. Consumer NVIDIA cards cap these.
    pub sessions: Option<u32>,
    /// Thousandths of the device's throughput still free.
    pub device_millis: Option<u32>,
}

impl Room {
    /// No limit known.
    pub const OPEN: Room = Room { sessions: None, device_millis: None };

    /// Whether `used` so far plus `more` still fits.
    pub fn fits(&self, used: &Cost, more: &Cost) -> bool {
        let sessions = used.device_sessions + more.device_sessions;
        let millis = used.device_millis + more.device_millis;
        self.sessions.is_none_or(|cap| sessions <= cap)
            && self.device_millis.is_none_or(|cap| millis <= cap)
    }
}

/// Which piece of audio work is being priced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioWork {
    Decode,
    /// Resample or remix to another rate or channel count.
    Convert,
    Encode,
}

/// The planner's only view of the machine.
pub trait CostModel {
    /// Every video encoder this machine has, best first within each codec.
    /// The planner prefers hardware over software and otherwise keeps this
    /// order.
    fn encoders(&self) -> Vec<EncoderSlot>;

    /// What encoding `shape` on `enc` costs, or `None` when that encoder
    /// cannot make that shape at all (too big, too fast, a profile it lacks).
    fn encode_cost(&self, shape: &VideoShape, enc: &EncoderSlot) -> Option<Cost>;

    /// Scaling and converting `from` into `to`.
    fn scale_cost(&self, from: &VideoShape, to: &VideoShape) -> Cost;

    /// Decoding one encoded picture stream of this shape.
    fn decode_cost(&self, shape: &VideoShape) -> Cost;

    /// One piece of audio work, or `None` when this machine cannot do it
    /// (no encoder for that codec).
    fn audio_cost(&self, shape: &AudioShape, work: AudioWork) -> Option<Cost>;

    /// Wrapping `egress_kbps` of streams in `container` and sending it out.
    fn mux_cost(&self, container: Container, egress_kbps: u32) -> Cost {
        let _ = container;
        Cost { cpu_millicores: 5, memory_mib: 2, egress_kbps, ..Cost::default() }
    }

    /// What is left on a hardware device (a key from `device_of`). The CPU is
    /// never asked: whether the CPU can take the plan is the governor's
    /// `admit`, and software is the last resort the planner falls back to.
    fn room(&self, device: &str) -> Room {
        let _ = device;
        Room::OPEN
    }
}
