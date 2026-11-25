//! The camera's own units: speeds, positions and their limits.

/// Speed and position limits in the camera's own units. Pan and tilt speeds
/// are hundredths of a degree a second; positions are hundredths of a
/// degree; zoom is the camera's zoom position, low at tele.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ranges {
    pub pan_speed: i32,
    pub tilt_speed: i32,
    pub zoom_speed: i32,
    pub pan: (i32, i32),
    pub tilt: (i32, i32),
    pub zoom: (i32, i32),
}

impl Default for Ranges {
    /// The CR-N500's: 100 degrees a second, plus or minus 170 degrees of pan,
    /// -30 to 90 of tilt, and a zoom speed of 0 to 15, which every model takes.
    fn default() -> Ranges {
        Ranges { pan_speed: 10_000, tilt_speed: 10_000, zoom_speed: 15, pan: (-17_000, 17_000), tilt: (-3_000, 9_000), zoom: (570, 7_300) }
    }
}

pub fn norm(v: i32, (lo, hi): (i32, i32)) -> f32 {
    if v >= 0 { v as f32 / hi.max(1) as f32 } else { -(v as f32) / lo.min(-1) as f32 }
}

pub fn denorm(v: f32, (lo, hi): (i32, i32)) -> i32 {
    (if v >= 0.0 { v * hi as f32 } else { -v * lo as f32 }).round() as i32
}
