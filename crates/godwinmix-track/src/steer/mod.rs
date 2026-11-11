//! Layer five: two outputs from the same target. Speeds for a PTZ camera,
//! and a smoothed rectangle for a digital crop.

mod crop;
mod ptz;

pub use crop::{CropConfig, CropSteer};
pub use ptz::{PtzConfig, PtzSteer, PtzVelocity};
