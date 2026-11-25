//! Reading one setting out of a filter's params, with the field named and its
//! range when it is wrong.
//!
//! Each setting can be spelled more than one way: the name GodwinMix uses
//! first, then the name OBS writes, so a filter that came across with an OBS
//! import keeps the settings it had there.

use crate::config::Params;
use anyhow::{bail, Result};

fn number(value: &toml::Value) -> Option<f64> {
    match value {
        toml::Value::Float(f) => Some(*f),
        toml::Value::Integer(i) => Some(*i as f64),
        _ => None,
    }
}

/// The first of `keys` the params carry, or `default`, checked against
/// `lo..=hi`.
pub(crate) fn num(
    kind: &str,
    params: &Params,
    keys: &[&str],
    default: f64,
    lo: f64,
    hi: f64,
) -> Result<f64> {
    let Some((key, value)) = keys.iter().find_map(|k| params.get(*k).map(|v| (*k, v))) else {
        return Ok(default);
    };
    match number(value) {
        Some(n) if n >= lo && n <= hi => Ok(n),
        _ => bail!("{kind} params.{key} must be a number from {lo} to {hi}, not `{value}`"),
    }
}

/// A switch, true or false.
pub(crate) fn flag(kind: &str, params: &Params, key: &str, default: bool) -> Result<bool> {
    match params.get(key) {
        None => Ok(default),
        Some(toml::Value::Boolean(b)) => Ok(*b),
        Some(other) => bail!("{kind} params.{key} must be true or false, not `{other}`"),
    }
}

/// Decibels to a linear gain.
pub(crate) fn db_to_gain(db: f64) -> f32 {
    10f64.powf(db / 20.0) as f32
}

/// The smoothing factor for a follower that covers about 63% of a step in
/// `ms`. Zero milliseconds follows at once.
pub(crate) fn time_coeff(ms: f64, rate: u32) -> f32 {
    if ms <= 0.0 || rate == 0 {
        return 0.0;
    }
    (-1.0 / (ms * 0.001 * rate as f64)).exp() as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(text: &str) -> Params {
        toml::from_str(text).unwrap()
    }

    #[test]
    fn the_first_spelling_present_is_read() {
        let p = params("db = 3.0");
        assert_eq!(num("gain/filter", &p, &["gain_db", "db"], 0.0, -60.0, 30.0).unwrap(), 3.0);
        let p = params("gain_db = -6\ndb = 3.0");
        assert_eq!(num("gain/filter", &p, &["gain_db", "db"], 0.0, -60.0, 30.0).unwrap(), -6.0);
    }

    #[test]
    fn a_value_out_of_range_names_the_field_and_the_range() {
        let p = params("gain_db = 90");
        let e = num("gain/filter", &p, &["gain_db"], 0.0, -60.0, 30.0).unwrap_err().to_string();
        assert!(e.contains("params.gain_db") && e.contains("-60 to 30"), "{e}");
    }

    #[test]
    fn decibels_turn_into_gain() {
        assert!((db_to_gain(0.0) - 1.0).abs() < 1e-6);
        assert!((db_to_gain(-6.0206) - 0.5).abs() < 1e-4);
    }
}
