//! The LUTs that ship: camera conversions from log to Rec.709, and looks.
//!
//! Every one is made here in code from a formula, so nothing is licensed
//! from anyone and nothing has to be on disk. A look works on a Rec.709
//! picture and keeps black and white where they are unless the look is
//! about moving them (faded film lifts its blacks on purpose).

use super::log::{self, Primaries};
use super::Lut;

/// The points a side a built in is made at. 33 is what grading tools write.
pub const SIZE: usize = 33;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Camera,
    Look,
}

impl Group {
    pub fn as_str(self) -> &'static str {
        match self {
            Group::Camera => "camera",
            Group::Look => "look",
        }
    }
}

pub struct BuiltIn {
    pub name: &'static str,
    pub title: &'static str,
    pub group: Group,
    pub about: &'static str,
    f: fn([f32; 3]) -> [f32; 3],
}

impl BuiltIn {
    pub fn lut(&self) -> Lut {
        Lut::from_fn(self.title, SIZE, self.f)
    }
}

pub const ALL: &[BuiltIn] = &[
    camera("slog3-to-709", "Sony S-Log3 to Rec.709", "S-Log3 with S-Gamut3.Cine, from an FX3, FX6, A7S III or similar", |c| from_log(c, log::slog3, &log::S_GAMUT3_CINE)),
    camera("vlog-to-709", "Panasonic V-Log to Rec.709", "V-Log with V-Gamut, from a GH5, GH6, S5 or similar", |c| from_log(c, log::vlog, &log::V_GAMUT)),
    camera("clog3-to-709", "Canon C-Log3 to Rec.709", "C-Log3 with Cinema Gamut, from an R5 C, C70 or similar", |c| from_log(c, log::clog3, &log::CINEMA_GAMUT)),
    camera("logc3-to-709", "ARRI LogC3 to Rec.709", "LogC3 at EI 800 with ARRI Wide Gamut 3", |c| from_log(c, log::logc3, &log::ARRI_WG3)),
    camera("flog-to-709", "Fujifilm F-Log to Rec.709", "F-Log with F-Gamut, from an X-H2, X-T5 or similar", |c| from_log(c, log::flog, &log::REC2020)),
    camera("nlog-to-709", "Nikon N-Log to Rec.709", "N-Log with Rec.2020, from a Z6, Z8 or similar", |c| from_log(c, log::nlog, &log::REC2020)),
    camera("hlg-to-709", "HLG to Rec.709", "HLG with Rec.2020, which phones and many cameras record as HDR", |c| from_log(c, log::hlg, &log::REC2020)),
    look("warm", "Warm", "a little more red and less blue, as under tungsten light", |c| saturate(gains(c, [1.06, 1.0, 0.91]), 1.05)),
    look("cool", "Cool", "a little more blue, as in daylight shade", |c| gains(c, [0.94, 0.99, 1.07])),
    look("teal-orange", "Teal and orange", "shadows towards teal and highlights towards orange, the film trailer look", teal_orange),
    look("faded-film", "Faded film", "lifted blacks, softened whites, gentle colour", |c| saturate(fade(gains(c, [1.02, 1.0, 0.96]), 0.06, 0.94), 0.85)),
    look("bleach-bypass", "Bleach bypass", "low colour and hard contrast, like skipping the bleach in film processing", |c| contrast(saturate(c, 0.45), 1.3)),
    look("high-contrast", "High contrast", "deeper shadows and brighter highlights, colour kept", |c| contrast(c, 1.35)),
    look("vivid", "Vivid", "stronger colour and a touch more contrast", |c| contrast(saturate(c, 1.35), 1.08)),
    look("black-and-white", "Black and white", "Rec.709 luma with a little contrast", |c| contrast(saturate(c, 0.0), 1.1)),
    look("sepia", "Sepia", "black and white toned brown", |c| gains(saturate(c, 0.0), [1.07, 0.97, 0.80])),
    look("day-for-night", "Day for night", "darker, bluer and less colour, so daylight reads as night", |c| gains(saturate(c.map(|v| v.powf(1.4) * 0.55), 0.45), [0.80, 0.92, 1.15])),
];

const fn camera(name: &'static str, title: &'static str, about: &'static str, f: fn([f32; 3]) -> [f32; 3]) -> BuiltIn {
    BuiltIn { name, title, group: Group::Camera, about, f }
}

const fn look(name: &'static str, title: &'static str, about: &'static str, f: fn([f32; 3]) -> [f32; 3]) -> BuiltIn {
    BuiltIn { name, title, group: Group::Look, about, f }
}

pub fn find(name: &str) -> Option<&'static BuiltIn> {
    ALL.iter().find(|b| b.name == name)
}

fn from_log(c: [f32; 3], curve: fn(f32) -> f32, gamut: &Primaries) -> [f32; 3] {
    let lin = log::apply(&log::to_rec709(gamut), c.map(curve));
    lin.map(log::display)
}

fn luma(c: [f32; 3]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

fn clamp(c: [f32; 3]) -> [f32; 3] {
    c.map(|v| v.clamp(0.0, 1.0))
}

fn saturate(c: [f32; 3], s: f32) -> [f32; 3] {
    let y = luma(c);
    clamp(c.map(|v| y + (v - y) * s))
}

fn gains(c: [f32; 3], g: [f32; 3]) -> [f32; 3] {
    clamp(std::array::from_fn(|i| c[i] * g[i]))
}

/// An S curve about the middle that keeps 0 and 1 where they are.
fn contrast(c: [f32; 3], k: f32) -> [f32; 3] {
    let s = |v: f32| {
        let v = v.clamp(0.0, 1.0);
        if v < 0.5 { 0.5 * (2.0 * v).powf(k) } else { 1.0 - 0.5 * (2.0 * (1.0 - v)).powf(k) }
    };
    c.map(s)
}

fn fade(c: [f32; 3], black: f32, white: f32) -> [f32; 3] {
    c.map(|v| black + v.clamp(0.0, 1.0) * (white - black))
}

fn teal_orange(c: [f32; 3]) -> [f32; 3] {
    let y = luma(c);
    // The shadow tint fades out towards black, so black stays black.
    let (shadow, high) = ((1.0 - y).powi(2) * (y * 8.0).min(1.0), y * y);
    let toned: [f32; 3] = std::array::from_fn(|i| c[i] + [-0.05, 0.015, 0.06][i] * shadow + [0.06, 0.01, -0.06][i] * high);
    contrast(saturate(toned, 1.1), 1.1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_slugs_and_unique() {
        let mut seen = std::collections::BTreeSet::new();
        for b in ALL {
            assert_eq!(crate::gallery::slug(b.name), b.name);
            assert!(seen.insert(b.name), "{} twice", b.name);
        }
    }

    #[test]
    fn every_look_keeps_black_black_and_white_near_white() {
        for b in ALL.iter().filter(|b| b.group == Group::Look && !matches!(b.name, "faded-film" | "day-for-night" | "sepia")) {
            let lut = b.lut();
            assert!(lut.sample([0.0; 3]).iter().all(|&v| v < 0.02), "{} lifts black", b.name);
            assert!(lut.sample([1.0; 3]).iter().all(|&v| v > 0.85), "{} dims white", b.name);
        }
    }

    #[test]
    fn a_grey_card_in_slog3_comes_out_a_mid_grey() {
        let lut = find("slog3-to-709").unwrap().lut();
        let out = lut.sample([420.0 / 1023.0; 3]);
        for v in out {
            assert!((v - 0.44).abs() < 0.03, "{out:?}");
        }
    }

    #[test]
    fn black_and_white_has_no_colour() {
        let out = find("black-and-white").unwrap().lut().sample([0.8, 0.2, 0.1]);
        assert!((out[0] - out[1]).abs() < 1e-3 && (out[1] - out[2]).abs() < 1e-3, "{out:?}");
    }
}
