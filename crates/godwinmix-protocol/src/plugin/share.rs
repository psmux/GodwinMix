//! `share = { bus = "camera", params = ["device"] }` on a source provide.
//!
//! The one rule a plugin follows to have what it opens decoded once and
//! shared: name the params that say what it opens. Two sources whose values
//! for those params are equal open the same thing, so the core runs the plugin
//! for the first of them only and hands every other one the decoded frames
//! over the frame bus, in this process or in another. The plugin itself does
//! nothing different. docs/reference/plugin-manifest.md has the rule and
//! docs/explanation/frame-bus.md the mechanism.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// What a source opens, as the frame bus names it.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Share {
    /// `camera` for a device this machine opens (a camera, a capture card, a
    /// screen), `channel` for a stream arriving on a channel.
    pub bus: String,
    /// The params that together name what is opened. For `channel`, one param
    /// whose value is `<channel>/<stream>`.
    pub params: Vec<String>,
}

/// The bus kinds a provide may declare.
pub const BUSES: [&str; 2] = ["camera", "channel"];

impl Share {
    /// Everything wrong with this declaration, as `(key, message)` pairs
    /// relative to `share`. `audio` is whether the provide declares sound,
    /// which the frame bus does not carry for a device.
    pub fn problems(&self, audio: bool) -> Vec<(String, String)> {
        let mut out = vec![];
        if !BUSES.contains(&self.bus.as_str()) {
            out.push((
                "bus".into(),
                format!("'{}' is neither 'camera' nor 'channel'.", self.bus),
            ));
        }
        if self.params.is_empty() || self.params.iter().any(|p| p.trim().is_empty()) {
            out.push((
                "params".into(),
                "name the params that say what this source opens, for example \
                 params = [\"device\"]."
                    .into(),
            ));
        }
        if self.bus == "channel" && self.params.len() > 1 {
            out.push((
                "params".into(),
                "a channel is named by one param holding <channel>/<stream>.".into(),
            ));
        }
        if self.bus == "camera" && audio {
            out.push((
                "bus".into(),
                "the frame bus carries pictures, not sound, so a device with audio cannot be \
                 shared. Declare audio = \"none\" and offer the sound as its own source, or \
                 remove share."
                    .into(),
            ));
        }
        out
    }

    /// The values of the named params, in order, as text. A param that is
    /// missing or empty is an empty string: two sources that both leave the
    /// device unset both mean the default one.
    pub fn values(&self, params: &Value) -> Vec<String> {
        self.params
            .iter()
            .map(|p| match params.get(p) {
                None | Some(Value::Null) => String::new(),
                Some(Value::String(s)) => s.trim().to_string(),
                Some(other) => other.to_string(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_camera_with_one_param_is_fine_and_sound_is_refused() {
        let share = Share { bus: "camera".into(), params: vec!["device".into()] };
        assert!(share.problems(false).is_empty());
        let audio = share.problems(true);
        assert_eq!(audio.len(), 1);
        assert!(audio[0].1.contains("sound"), "{audio:?}");
    }

    #[test]
    fn an_unknown_bus_and_no_params_say_what_to_write() {
        let share = Share { bus: "tape".into(), params: vec![] };
        let problems = share.problems(false);
        assert!(problems.iter().any(|(k, m)| k == "bus" && m.contains("camera")));
        assert!(problems.iter().any(|(k, m)| k == "params" && m.contains("device")));
    }

    #[test]
    fn values_come_out_in_order_and_missing_is_empty() {
        let share = Share {
            bus: "camera".into(),
            params: vec!["monitor".into(), "region".into(), "device".into()],
        };
        let got = share.values(&json!({"monitor": 1, "region": " 0,0,640,360 "}));
        assert_eq!(got, vec!["1", "0,0,640,360", ""]);
    }
}
