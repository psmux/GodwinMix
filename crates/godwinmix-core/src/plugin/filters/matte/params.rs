//! What the cutout takes: which model, on which device, and how its edge is
//! shaped. Every setting has a default that works with nothing set.

use crate::config::Params;
use crate::plugin::filters::chroma::params::Matte;
use anyhow::{bail, Result};

/// Which model cuts the person out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Quality {
    /// The fine model where there is a GPU to run it, the fast one elsewhere.
    Auto,
    /// The small segmentation model: a few milliseconds on any CPU.
    Fast,
    /// The matting model: finer hair and edges, for a machine with a GPU.
    Fine,
    /// A model the operator added, by its file name without `.onnx`.
    Named(String),
}

/// Where the model runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Device {
    /// The fastest the machine has, the CPU last.
    Auto,
    Gpu,
    Cpu,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub quality: Quality,
    pub device: Device,
    /// Where the edge sits in the model's answer, 0 to 1.
    pub cutoff: f32,
    /// How wide the edge is, 0 for a hard edge.
    pub softness: f32,
    /// How much of the last matte carries into the next, 0 to 0.95. Steadies
    /// a model with no memory of its own between frames.
    pub steady: f32,
    /// Pixels the edge is softened inwards.
    pub feather: u32,
    /// Parts of the picture that are never the person.
    pub matte: Matte,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            quality: Quality::Auto,
            device: Device::Auto,
            cutoff: 0.5,
            softness: 0.3,
            steady: 0.5,
            feather: 0,
            matte: Matte::default(),
        }
    }
}

pub const KEYS: &str = "quality, device, cutoff, softness, steady, feather, matte_left, \
                        matte_top, matte_right, matte_bottom";

impl Settings {
    pub fn from_params(params: &Params) -> Result<Settings> {
        let mut s = Settings::default();
        for (key, value) in params {
            match key.as_str() {
                "quality" | "model" => s.quality = quality(key, value)?,
                "device" => s.device = device(key, value)?,
                "cutoff" => s.cutoff = fraction(key, value, 0.0, 1.0)?,
                "softness" => s.softness = fraction(key, value, 0.0, 1.0)?,
                "steady" => s.steady = fraction(key, value, 0.0, 0.95)?,
                "feather" => s.feather = fraction(key, value, 0.0, 20.0)? as u32,
                "matte_left" => s.matte.left = fraction(key, value, 0.0, 0.98)?,
                "matte_top" => s.matte.top = fraction(key, value, 0.0, 0.98)?,
                "matte_right" => s.matte.right = fraction(key, value, 0.0, 0.98)?,
                "matte_bottom" => s.matte.bottom = fraction(key, value, 0.0, 0.98)?,
                "id" | "opacity" => {}
                other => bail!("matte/filter has no setting `{other}`. It takes: {KEYS}"),
            }
        }
        let m = s.matte;
        if m.left + m.right >= 0.98 || m.top + m.bottom >= 0.98 {
            bail!(
                "matte/filter's matte leaves nothing: left {} and right {} (or top {} and bottom \
                 {}) add up to the whole picture. Keep each pair under 0.98.",
                m.left, m.right, m.top, m.bottom
            );
        }
        Ok(s)
    }

    /// The model's answer turned into alpha: 0 below the edge, 255 above it,
    /// and a ramp across it.
    pub fn curve(&self) -> [u8; 256] {
        let lo = (self.cutoff - self.softness / 2.0).clamp(0.0, 1.0);
        let hi = (self.cutoff + self.softness / 2.0).clamp(0.0, 1.0).max(lo + 1.0 / 255.0);
        let mut out = [0u8; 256];
        for (i, o) in out.iter_mut().enumerate() {
            let t = ((i as f32 / 255.0 - lo) / (hi - lo)).clamp(0.0, 1.0);
            // Smoothstep: no visible step where the ramp starts and ends.
            *o = (t * t * (3.0 - 2.0 * t) * 255.0).round() as u8;
        }
        out
    }
}

fn quality(key: &str, value: &toml::Value) -> Result<Quality> {
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

fn device(key: &str, value: &toml::Value) -> Result<Device> {
    match value.as_str().unwrap_or_default().trim() {
        "" | "auto" => Ok(Device::Auto),
        "gpu" => Ok(Device::Gpu),
        "cpu" => Ok(Device::Cpu),
        other => bail!("matte/filter params.{key} must be auto, gpu or cpu, not `{other}`"),
    }
}

fn fraction(key: &str, value: &toml::Value, lo: f32, hi: f32) -> Result<f32> {
    let n = value.as_float().or_else(|| value.as_integer().map(|i| i as f64));
    match n.map(|n| n as f32) {
        Some(n) if (lo..=hi).contains(&n) => Ok(n),
        _ => bail!("matte/filter params.{key} must be a number from {lo} to {hi}, not `{value}`"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(text: &str) -> Params {
        toml::from_str(text).unwrap()
    }

    #[test]
    fn nothing_set_is_a_working_cutout() {
        let s = Settings::from_params(&Params::new()).unwrap();
        assert_eq!(s.quality, Quality::Auto);
        let c = s.curve();
        assert_eq!((c[0], c[255]), (0, 255));
        assert!(c[128] > 100 && c[128] < 160, "{}", c[128]);
    }

    #[test]
    fn a_wrong_setting_names_what_it_takes() {
        let err = Settings::from_params(&params("blur = 3")).unwrap_err().to_string();
        assert!(err.contains("quality") && err.contains("softness"), "{err}");
        let err = Settings::from_params(&params("steady = 2.0")).unwrap_err().to_string();
        assert!(err.contains("0 to 0.95"), "{err}");
        let err = Settings::from_params(&params("matte_left = 0.5\nmatte_right = 0.5")).unwrap_err().to_string();
        assert!(err.contains("leaves nothing"), "{err}");
    }

    #[test]
    fn a_model_of_ones_own_is_named_by_its_file() {
        let s = Settings::from_params(&params("model = \"rvm_mobilenetv3\"")).unwrap();
        assert_eq!(s.quality, Quality::Named("rvm_mobilenetv3".into()));
        assert!(Settings::from_params(&params("model = \"../escape\"")).is_err());
    }

    #[test]
    fn a_hard_edge_is_a_step_at_the_cutoff() {
        let s = Settings { softness: 0.0, ..Settings::default() };
        let c = s.curve();
        assert_eq!((c[126], c[129]), (0, 255));
    }
}
