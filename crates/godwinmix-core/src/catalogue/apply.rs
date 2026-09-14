//! Turning catalogue property tables into properties on a real element.
//!
//! Two jobs. One is the unit indirection: the catalogue states the programme
//! bitrate once, in kilobits, and every encoder gets it in whatever unit that
//! encoder happens to want. The other is that nothing here may ever be fatal.
//! A property this element version does not have, an enum nickname it does not
//! know, a unit that cannot convert: all of them are a logged warning and the
//! encoder runs with its default. A mixer that refuses to start because it
//! could not set `rc-lookahead` is worse than one that runs without it.

use super::model::{Derived, Keyframe, PropValue};
use crate::probe;
use gstreamer as gst;
use std::collections::BTreeMap;
use tracing::warn;

/// The running configuration, in the units the catalogue names.
#[derive(Debug, Clone, Copy)]
pub struct Vars {
    pub video_bitrate_kbps: i64,
    pub audio_bitrate_kbps: i64,
    pub keyframe_frames: i64,
    pub keyframe_secs: i64,
    pub fps: i64,
    pub cpu_count: i64,
}

impl Default for Vars {
    fn default() -> Self {
        Self {
            video_bitrate_kbps: 4000,
            audio_bitrate_kbps: 128,
            keyframe_frames: 60,
            keyframe_secs: 2,
            fps: 30,
            cpu_count: cpu_count(),
        }
    }
}

pub fn cpu_count() -> i64 {
    std::thread::available_parallelism()
        .map(|n| n.get() as i64)
        .unwrap_or(1)
}

/// Which family a unit belongs to, so a conversion that makes no sense is
/// caught by the CI validation rather than by a 160 bit/s stream on air.
fn family(unit: &str) -> Option<&'static str> {
    match unit {
        "bit" | "kbit" | "mbit" => Some("bitrate"),
        "frames" | "seconds" => Some("interval"),
        "count" | "raw" => Some("count"),
        _ => None,
    }
}

/// Every variable a `from` may name, with the unit its value is held in.
pub const UNITS: &[(&str, &str)] = &[
    ("video.bitrate_kbps", "kbit"),
    ("audio.bitrate_kbps", "kbit"),
    ("keyframe.frames", "frames"),
    ("keyframe.secs", "seconds"),
    ("canvas.fps", "count"),
    ("cpu.count", "count"),
];

pub fn known_variable(from: &str) -> Option<&'static str> {
    UNITS.iter().find(|(n, _)| *n == from).map(|(_, u)| *u)
}

/// Check a `{unit, from}` pair without a GStreamer registry, for the CI
/// validation test. Returns the reason it is wrong, or None.
pub fn check_derived(d: &Derived) -> Option<String> {
    let Some(native) = known_variable(&d.from) else {
        let names: Vec<&str> = UNITS.iter().map(|(n, _)| *n).collect();
        return Some(format!(
            "unknown variable {:?}; known: {}",
            d.from,
            names.join(", ")
        ));
    };
    match (family(native), family(&d.unit)) {
        (_, None) => Some(format!("unknown unit {:?}", d.unit)),
        (Some(a), Some(b)) if a != b => Some(format!(
            "{:?} is in {native}, which cannot convert to {:?}",
            d.from, d.unit
        )),
        _ => None,
    }
}

impl Vars {
    /// The value of a variable in its own unit.
    fn native(&self, from: &str) -> Option<i64> {
        Some(match from {
            "video.bitrate_kbps" => self.video_bitrate_kbps,
            "audio.bitrate_kbps" => self.audio_bitrate_kbps,
            "keyframe.frames" => self.keyframe_frames,
            "keyframe.secs" => self.keyframe_secs,
            "canvas.fps" => self.fps,
            "cpu.count" => self.cpu_count,
            _ => return None,
        })
    }

    /// Resolve a `{unit, from}` into the number this element wants.
    pub fn resolve(&self, d: &Derived) -> Option<i64> {
        if let Some(why) = check_derived(d) {
            warn!(from = %d.from, unit = %d.unit, %why, "catalogue property skipped");
            return None;
        }
        let value = self.native(&d.from)?;
        let native = known_variable(&d.from)?;
        Some(convert(value, native, &d.unit, self.fps))
    }
}

/// Convert between two units of the same family. `fps` is needed because
/// frames and seconds only relate through the canvas framerate.
fn convert(value: i64, from: &str, to: &str, fps: i64) -> i64 {
    let fps = fps.max(1);
    match (from, to) {
        (a, b) if a == b => value,
        ("kbit", "bit") => value.saturating_mul(1000),
        ("kbit", "mbit") => value / 1000,
        ("bit", "kbit") => value / 1000,
        ("bit", "mbit") => value / 1_000_000,
        ("mbit", "kbit") => value.saturating_mul(1000),
        ("mbit", "bit") => value.saturating_mul(1_000_000),
        ("seconds", "frames") => value.saturating_mul(fps),
        ("frames", "seconds") => value / fps,
        _ => value,
    }
}

/// Set every property in a catalogue table on an element.
pub fn apply(el: &gst::Element, props: &BTreeMap<String, PropValue>, vars: &Vars) {
    for (name, value) in props {
        set_one(el, name, value, vars);
    }
}

fn set_one(el: &gst::Element, name: &str, value: &PropValue, vars: &Vars) {
    match value {
        PropValue::Derived(d) => {
            if let Some(v) = vars.resolve(d) {
                probe::set_int(el, name, v);
            }
        }
        PropValue::Bool(b) => probe::set_bool(el, name, *b),
        PropValue::Int(i) => probe::set_int(el, name, *i),
        PropValue::Float(f) => probe::set_float(el, name, *f),
        PropValue::Text(s) => probe::set_enum(el, name, s),
    }
}

/// Put the keyframe interval where this encoder keeps it.
pub fn apply_keyframe(el: &gst::Element, kf: Option<&Keyframe>, vars: &Vars) {
    let Some(kf) = kf else { return };
    let native = "frames";
    let value = convert(vars.keyframe_frames, native, &kf.unit, vars.fps);
    probe::set_int(el, &kf.property, value);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn derived(unit: &str, from: &str) -> Derived {
        Derived {
            unit: unit.into(),
            from: from.into(),
        }
    }

    #[test]
    fn bitrate_converts_into_the_unit_the_element_wants() {
        let vars = Vars {
            video_bitrate_kbps: 4000,
            ..Default::default()
        };
        assert_eq!(
            vars.resolve(&derived("kbit", "video.bitrate_kbps")),
            Some(4000)
        );
        assert_eq!(
            vars.resolve(&derived("bit", "video.bitrate_kbps")),
            Some(4_000_000)
        );
        assert_eq!(
            vars.resolve(&derived("mbit", "video.bitrate_kbps")),
            Some(4)
        );
    }

    #[test]
    fn keyframe_seconds_and_frames_relate_through_the_framerate() {
        let vars = Vars {
            keyframe_secs: 2,
            keyframe_frames: 120,
            fps: 60,
            ..Default::default()
        };
        assert_eq!(vars.resolve(&derived("frames", "keyframe.secs")), Some(120));
        assert_eq!(
            vars.resolve(&derived("seconds", "keyframe.frames")),
            Some(2)
        );
    }

    #[test]
    fn a_nonsense_conversion_is_refused_rather_than_guessed() {
        assert!(check_derived(&derived("bit", "cpu.count")).is_some());
        assert!(check_derived(&derived("furlongs", "cpu.count")).is_some());
        assert!(check_derived(&derived("count", "cpu.threads")).is_some());
        assert!(check_derived(&derived("count", "cpu.count")).is_none());
    }
}
