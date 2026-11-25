//! What the colour correction takes, read from the filter's params.
//!
//! Every setting is 0 by default, and all zeros changes nothing and costs
//! nothing. The names and ranges are OBS's own `color_filter_v2`, so an
//! imported filter keeps its look: `gamma` -3 to 3, `contrast` -4 to 4,
//! `brightness` -1 to 1, `saturation` -1 to 5 and `hue_shift` -180 to 180.

use crate::config::Params;
use crate::plugin::filters::audio::values::num;
use anyhow::Result;

const KIND: &str = "color/filter";

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Settings {
    pub gamma: f64,
    pub contrast: f64,
    pub brightness: f64,
    pub saturation: f64,
    pub hue_shift: f64,
}

impl Settings {
    pub fn from_params(params: &Params) -> Result<Settings> {
        Ok(Settings {
            gamma: num(KIND, params, &["gamma"], 0.0, -3.0, 3.0)?,
            contrast: num(KIND, params, &["contrast"], 0.0, -4.0, 4.0)?,
            brightness: num(KIND, params, &["brightness"], 0.0, -1.0, 1.0)?,
            saturation: num(KIND, params, &["saturation"], 0.0, -1.0, 5.0)?,
            hue_shift: num(KIND, params, &["hue_shift", "hue"], 0.0, -180.0, 180.0)?,
        })
    }

    /// True when nothing would change: the frame is left alone.
    pub fn is_identity(&self) -> bool {
        *self == Settings::default()
    }

    /// The luma half changes nothing.
    pub fn luma_is_identity(&self) -> bool {
        self.gamma == 0.0 && self.contrast == 0.0 && self.brightness == 0.0
    }

    /// The chroma half changes nothing.
    pub fn chroma_is_identity(&self) -> bool {
        self.saturation == 0.0 && self.hue_shift == 0.0
    }
}
