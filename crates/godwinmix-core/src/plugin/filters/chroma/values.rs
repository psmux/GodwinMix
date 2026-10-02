//! Reading one number or colour out of the params, with the field named and
//! its range when it is wrong.

use anyhow::{bail, Result};

/// OBS packs the colour as 0xAABBGGRR in an integer.
pub(super) fn obs_colour(value: &toml::Value) -> Result<[u8; 3]> {
    let n = value.as_integer().filter(|n| *n >= 0).ok_or_else(|| {
        anyhow::anyhow!("chroma/filter params.key_color must be OBS's packed colour, not `{value}`")
    })?;
    Ok([(n & 0xff) as u8, ((n >> 8) & 0xff) as u8, ((n >> 16) & 0xff) as u8])
}

fn number(value: &toml::Value) -> Option<f64> {
    match value {
        toml::Value::Float(f) => Some(*f),
        toml::Value::Integer(i) => Some(*i as f64),
        _ => None,
    }
}

/// 0 to 1. Where `obs` is set, a number from 1 to 1000 is read as OBS's own
/// scale and divided by 1000, so an imported key keeps its look.
pub(super) fn fraction(key: &str, value: &toml::Value, obs: bool) -> Result<f32> {
    let n = number(value).unwrap_or(-1.0);
    let n = if obs && n > 1.0 && n <= 1000.0 { n / 1000.0 } else { n };
    if !(0.0..=1.0).contains(&n) {
        bail!("chroma/filter params.{key} must be a number from 0 to 1, not `{value}`");
    }
    Ok(n as f32)
}

pub(super) fn byte(key: &str, value: &toml::Value) -> Result<u8> {
    Ok(whole(key, value, 0, 255)? as u8)
}

pub(super) fn whole(key: &str, value: &toml::Value, lo: u32, hi: u32) -> Result<u32> {
    match number(value) {
        Some(n) if n >= lo as f64 && n <= hi as f64 => Ok(n.round() as u32),
        _ => bail!("chroma/filter params.{key} must be a whole number from {lo} to {hi}, not `{value}`"),
    }
}
