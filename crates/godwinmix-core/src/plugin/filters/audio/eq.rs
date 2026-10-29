//! `eq/filter`: a low cut, three tone controls and up to ten bands of your own.
//!
//! `{}` changes nothing. The short form covers what a volunteer reaches for:
//! `low_cut_hz` for rumble and handling noise, `low_gain_db`, `mid_gain_db`
//! and `high_gain_db` for the tone. `bands` is the long form, a list of
//! `{ type, hz, gain_db, q }` with `type` one of `peak`, `low_shelf`,
//! `high_shelf`, `low_cut` and `high_cut`, run after the short form. OBS's
//! three band equaliser writes `low`, `mid` and `high`, which are read as the
//! three tone controls.

use super::biquad::{Band, Biquad, Shape};
use super::run::Dsp;
use super::values::num;
use crate::config::Params;
use anyhow::{bail, Result};

const KIND: &str = "eq/filter";
const MAX_BANDS: usize = 10;

#[derive(Default)]
pub struct Eq {
    bands: Vec<Band>,
    run: Vec<Biquad>,
    format: Option<(u32, usize)>,
}

impl Dsp for Eq {
    fn set(&mut self, params: &Params) -> Result<()> {
        let bands = read(params)?;
        if bands != self.bands {
            self.bands = bands;
            if let Some((rate, channels)) = self.format {
                self.prepare(rate, channels);
            }
        }
        Ok(())
    }

    fn prepare(&mut self, rate: u32, channels: usize) {
        self.format = Some((rate, channels));
        self.run = self.bands.iter().map(|b| Biquad::new(b, rate, channels)).collect();
    }

    fn process(&mut self, samples: &mut [f32], channels: usize) {
        for stage in &mut self.run {
            stage.process(samples, channels);
        }
    }
}

fn read(params: &Params) -> Result<Vec<Band>> {
    let mut bands = Vec::new();
    let cut = num(KIND, params, &["low_cut_hz"], 0.0, 0.0, 1000.0)?;
    if cut > 0.0 {
        bands.push(Band { shape: Shape::HighPass, hz: cut, gain_db: 0.0, q: 0.707 });
    }
    let tone = [
        (Shape::LowShelf, ["low_gain_db", "low"], "low_hz", 120.0),
        (Shape::Peak, ["mid_gain_db", "mid"], "mid_hz", 1000.0),
        (Shape::HighShelf, ["high_gain_db", "high"], "high_hz", 8000.0),
    ];
    for (shape, gain_keys, hz_key, hz) in tone {
        let gain_db = num(KIND, params, &gain_keys, 0.0, -24.0, 24.0)?;
        if gain_db != 0.0 {
            let hz = num(KIND, params, &[hz_key], hz, 20.0, 20000.0)?;
            let q = num(KIND, params, &["mid_q"], 0.7, 0.1, 10.0)?;
            let q = if shape == Shape::Peak { q } else { 0.707 };
            bands.push(Band { shape, hz, gain_db, q });
        }
    }
    if let Some(list) = params.get("bands") {
        let Some(list) = list.as_array() else {
            bail!("{KIND} params.bands must be a list of {{ type, hz, gain_db, q }}");
        };
        if list.len() > MAX_BANDS {
            bail!("{KIND} takes up to {MAX_BANDS} bands of your own, and this one has {}", list.len());
        }
        for (n, item) in list.iter().enumerate() {
            bands.push(band(n, item)?);
        }
    }
    Ok(bands)
}

fn band(n: usize, item: &toml::Value) -> Result<Band> {
    let Some(table) = item.as_table() else {
        bail!("{KIND} params.bands[{n}] must be {{ type, hz, gain_db, q }}, not `{item}`");
    };
    let name = table.get("type").and_then(|t| t.as_str()).unwrap_or("peak");
    let Some(shape) = Shape::parse(name) else {
        bail!("{KIND} params.bands[{n}].type must be peak, low_shelf, high_shelf, low_cut or high_cut, not `{name}`");
    };
    let at = format!("{KIND} bands[{n}]");
    let kind: &str = &at;
    Ok(Band {
        shape,
        hz: num(kind, table, &["hz"], 1000.0, 20.0, 20000.0)?,
        gain_db: num(kind, table, &["gain_db"], 0.0, -24.0, 24.0)?,
        q: num(kind, table, &["q"], 0.707, 0.1, 10.0)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn level_after(params: &str, hz: f64) -> f32 {
        let mut eq = Eq::default();
        eq.set(&toml::from_str(params).unwrap()).unwrap();
        eq.prepare(48000, 1);
        let mut s: Vec<f32> =
            (0..48000).map(|n| (0.5 * (2.0 * std::f64::consts::PI * hz * n as f64 / 48000.0).sin()) as f32).collect();
        eq.process(&mut s, 1);
        s[24000..].iter().fold(0.0f32, |m, v| m.max(v.abs()))
    }

    #[test]
    fn nothing_set_changes_nothing() {
        assert!((level_after("", 1000.0) - 0.5).abs() < 1e-3);
    }

    #[test]
    fn a_low_cut_takes_out_rumble_and_keeps_the_voice() {
        assert!(level_after("low_cut_hz = 100", 30.0) < 0.1);
        assert!((level_after("low_cut_hz = 100", 1000.0) - 0.5).abs() < 0.02);
    }

    #[test]
    fn a_band_of_your_own_lifts_its_frequency() {
        let up = level_after("bands = [{ type = \"peak\", hz = 3000, gain_db = 6, q = 1 }]", 3000.0);
        assert!((up - 1.0).abs() < 0.03, "{up}");
    }

    #[test]
    fn obs_three_band_settings_are_the_tone_controls() {
        assert!(level_after("high = 6.0", 15000.0) > 0.9);
    }

    #[test]
    fn a_bad_band_type_names_the_band() {
        let e = read(&toml::from_str("bands = [{ type = \"wobble\" }]").unwrap()).unwrap_err().to_string();
        assert!(e.contains("bands[0].type"), "{e}");
    }
}
