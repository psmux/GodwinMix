//! A VISCA camera's limits, and the mapping between its steps and the
//! normalised position every driver speaks in.

use crate::types::Position;

/// Speed and position limits. Cameras differ; these defaults suit most 20x
/// and 30x church cameras and every one can be set per camera.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ranges {
    /// Highest pan speed byte, `0x18` on Sony and PTZOptics.
    pub pan_speed: u8,
    /// Highest tilt speed byte, `0x14` on Sony, `0x14` to `0x17` elsewhere.
    pub tilt_speed: u8,
    /// Pan steps from the centre to either end.
    pub pan: i32,
    /// Tilt steps from level to fully up, and to fully down.
    pub tilt_up: i32,
    pub tilt_down: i32,
    /// Zoom steps from wide to tele.
    pub zoom: i32,
}

impl Default for Ranges {
    fn default() -> Ranges {
        Ranges { pan_speed: 0x18, tilt_speed: 0x14, pan: 0x0990, tilt_up: 0x0510, tilt_down: 0x01B0, zoom: 0x4000 }
    }
}

impl Ranges {
    /// Raw pan, tilt and zoom steps as a normalised position.
    pub fn normalise(&self, pan: i32, tilt: i32, zoom: i32) -> Position {
        let t = if tilt >= 0 { tilt as f32 / self.tilt_up as f32 } else { tilt as f32 / self.tilt_down as f32 };
        Position {
            pan: (pan as f32 / self.pan as f32).clamp(-1.0, 1.0),
            tilt: t.clamp(-1.0, 1.0),
            zoom: (zoom as f32 / self.zoom as f32).clamp(0.0, 1.0),
            raw: [pan, tilt, zoom],
        }
    }

    /// A normalised position as raw pan, tilt and zoom steps.
    pub fn steps(&self, p: &Position) -> (i32, i32, u16) {
        let pan = (p.pan.clamp(-1.0, 1.0) * self.pan as f32).round() as i32;
        let tilt = if p.tilt >= 0.0 { p.tilt.min(1.0) * self.tilt_up as f32 } else { p.tilt.max(-1.0) * self.tilt_down as f32 };
        (pan, tilt.round() as i32, (p.zoom.clamp(0.0, 1.0) * self.zoom as f32).round() as u16)
    }
}
