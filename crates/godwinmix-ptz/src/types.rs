//! The values every driver speaks in, independent of any one protocol.
//!
//! Speeds are fractions of the camera's maximum, -1 to 1. Positions are
//! normalised too: pan and tilt -1 to 1 across the camera's range, zoom 0
//! (widest) to 1 (tightest). Each driver maps these to its own units.

/// Pan, tilt and zoom speeds, each -1 to 1. Positive pan turns right,
/// positive tilt looks up, positive zoom goes tighter.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Velocity {
    pub pan: f32,
    pub tilt: f32,
    pub zoom: f32,
}

impl Velocity {
    pub const STOP: Velocity = Velocity { pan: 0.0, tilt: 0.0, zoom: 0.0 };

    pub fn new(pan: f32, tilt: f32, zoom: f32) -> Velocity {
        Velocity { pan: clamp1(pan), tilt: clamp1(tilt), zoom: clamp1(zoom) }
    }

    pub fn is_stop(&self) -> bool {
        self.pan.abs() < DEAD && self.tilt.abs() < DEAD && self.zoom.abs() < DEAD
    }
}

/// Below this a speed is zero. A joystick resting near its centre must not
/// creep the camera.
pub const DEAD: f32 = 0.01;

pub fn clamp1(v: f32) -> f32 {
    if v.is_nan() { 0.0 } else { v.clamp(-1.0, 1.0) }
}

/// Where the camera points, normalised. `raw` keeps the protocol's own
/// numbers for a person reading a log next to the camera's manual.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Position {
    pub pan: f32,
    pub tilt: f32,
    pub zoom: f32,
    pub raw: [i32; 3],
}

/// One focus action.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Focus {
    Auto,
    Manual,
    /// Focus once on what is in the middle, then stay manual.
    OnePush,
    /// Drive focus at this speed, -1 (near) to 1 (far). 0 stops.
    Drive(f32),
}

/// Exposure, where the protocol has it. Every field is optional so a call
/// changes only what it names.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Exposure {
    pub mode: Option<ExposureMode>,
    /// The camera's own iris step, as its manual numbers it.
    pub iris: Option<u16>,
    /// The camera's own shutter step.
    pub shutter: Option<u16>,
    /// The camera's own gain step.
    pub gain: Option<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExposureMode {
    Auto,
    Manual,
    ShutterPriority,
    IrisPriority,
}

impl ExposureMode {
    pub fn parse(s: &str) -> Option<ExposureMode> {
        Some(match s {
            "auto" => ExposureMode::Auto,
            "manual" => ExposureMode::Manual,
            "shutter" | "shutter-priority" => ExposureMode::ShutterPriority,
            "iris" | "iris-priority" => ExposureMode::IrisPriority,
            _ => return None,
        })
    }
}

/// What a driver can do, so a panel greys out what it cannot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    pub absolute: bool,
    pub position: bool,
    pub focus: bool,
    pub exposure: bool,
    /// The highest preset number the protocol takes.
    pub presets: u16,
}
