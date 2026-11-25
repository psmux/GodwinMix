//! Settings value parsing.
use super::*;

pub(super) fn quality(key: &str, value: &toml::Value) -> Result<Quality> {
    match value.as_str().unwrap_or_default().trim() {
        "" | "auto" => Ok(Quality::Auto),
        "fast" => Ok(Quality::Fast),
        "fine" => Ok(Quality::Fine),
        name if key == "model" && is_name(name) => Ok(Quality::Named(name.to_string())),
        other => bail!(
            "matte/filter params.{key} must be auto, fast or fine, or `model` the name of a \
             model in the models folder, not `{other}`"
        ),
    }
}

fn is_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

pub(super) fn device(key: &str, value: &toml::Value) -> Result<Device> {
    match value.as_str().unwrap_or_default().trim() {
        "" | "auto" => Ok(Device::Auto),
        "gpu" => Ok(Device::Gpu),
        "cpu" => Ok(Device::Cpu),
        other => bail!("matte/filter params.{key} must be auto, gpu or cpu, not `{other}`"),
    }
}

pub(super) fn fraction(key: &str, value: &toml::Value, lo: f32, hi: f32) -> Result<f32> {
    let n = value.as_float().or_else(|| value.as_integer().map(|i| i as f64));
    match n.map(|n| n as f32) {
        Some(n) if (lo..=hi).contains(&n) => Ok(n),
        _ => bail!("matte/filter params.{key} must be a number from {lo} to {hi}, not `{value}`"),
    }
}

