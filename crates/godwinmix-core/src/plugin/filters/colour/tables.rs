//! The two tables the correction is, built once per change of settings.
//!
//! Luma is one table of 256: gamma, then contrast about the middle, then
//! brightness, on the video range 16 to 235 the canvas carries. Chroma is one
//! table of 65536, indexed by U and V together: a rotation for the hue and a
//! scale for the saturation, about the neutral 128. A frame is then one read
//! per luma sample and one per chroma pair, which is what lets a Raspberry Pi
//! correct a camera at 1080p.

use super::params::Settings;

pub struct Tables {
    pub luma: Option<Box<[u8; 256]>>,
    pub chroma: Option<Vec<[u8; 2]>>,
}

impl Tables {
    pub fn build(s: &Settings) -> Tables {
        Tables {
            luma: (!s.luma_is_identity()).then(|| luma(s)),
            chroma: (!s.chroma_is_identity()).then(|| chroma(s)),
        }
    }
}

fn byte(v: f64) -> u8 {
    v.round().clamp(0.0, 255.0) as u8
}

fn luma(s: &Settings) -> Box<[u8; 256]> {
    // OBS's own curves: a negative gamma darkens, a positive one lightens.
    let gamma = if s.gamma < 0.0 { 1.0 - s.gamma } else { 1.0 / (1.0 + s.gamma) };
    let contrast = if s.contrast < 0.0 { 1.0 / (1.0 - s.contrast) } else { 1.0 + s.contrast };
    let mut out = Box::new([0u8; 256]);
    for (i, o) in out.iter_mut().enumerate() {
        let y = ((i as f64 - 16.0) / 219.0).clamp(0.0, 1.0);
        let y = y.powf(gamma);
        let y = (y - 0.5) * contrast + 0.5 + s.brightness;
        *o = byte(16.0 + y.clamp(0.0, 1.0) * 219.0);
    }
    out
}

fn chroma(s: &Settings) -> Vec<[u8; 2]> {
    let (sin, cos) = s.hue_shift.to_radians().sin_cos();
    let scale = 1.0 + s.saturation;
    let mut out = vec![[128u8; 2]; 65536];
    for u in 0..256usize {
        for v in 0..256usize {
            let (cu, cv) = (u as f64 - 128.0, v as f64 - 128.0);
            let ru = (cu * cos - cv * sin) * scale;
            let rv = (cu * sin + cv * cos) * scale;
            out[u << 8 | v] = [byte(128.0 + ru), byte(128.0 + rv)];
        }
    }
    out
}

/// Run the tables over one I420 frame's planes, in place.
pub fn apply(t: &Tables, y: &mut [u8], u: &mut [u8], v: &mut [u8]) {
    if let Some(table) = &t.luma {
        y.iter_mut().for_each(|p| *p = table[*p as usize]);
    }
    if let Some(table) = &t.chroma {
        for (pu, pv) in u.iter_mut().zip(v.iter_mut()) {
            let [nu, nv] = table[(*pu as usize) << 8 | *pv as usize];
            *pu = nu;
            *pv = nv;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(text: &str) -> Settings {
        Settings::from_params(&toml::from_str(text).unwrap()).unwrap()
    }

    #[test]
    fn defaults_build_no_table_at_all() {
        let t = Tables::build(&settings(""));
        assert!(t.luma.is_none() && t.chroma.is_none());
    }

    #[test]
    fn brightness_lifts_the_middle_and_keeps_the_range() {
        let t = Tables::build(&settings("brightness = 0.2"));
        let l = t.luma.unwrap();
        assert!(l[126] > 126 + 30, "{}", l[126]);
        assert_eq!(l[255], 235, "white stays legal");
    }

    #[test]
    fn contrast_spreads_away_from_the_middle() {
        let l = Tables::build(&settings("contrast = 1.0")).luma.unwrap();
        assert!(l[80] < 80 && l[180] > 180, "{} {}", l[80], l[180]);
    }

    #[test]
    fn no_saturation_is_grey() {
        let c = Tables::build(&settings("saturation = -1")).chroma.unwrap();
        assert_eq!(c[200 << 8 | 60], [128, 128]);
    }

    #[test]
    fn a_half_turn_of_hue_is_the_opposite_colour() {
        let c = Tables::build(&settings("hue_shift = 180")).chroma.unwrap();
        assert_eq!(c[200 << 8 | 60], [56, 196]);
    }

    #[test]
    fn apply_changes_only_what_has_a_table() {
        let t = Tables::build(&settings("saturation = -1"));
        let (mut y, mut u, mut v) = (vec![90u8; 4], vec![200u8], vec![60u8]);
        apply(&t, &mut y, &mut u, &mut v);
        assert_eq!((y, u, v), (vec![90u8; 4], vec![128], vec![128]));
    }
}
