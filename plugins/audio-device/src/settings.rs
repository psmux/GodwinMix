//! What `schemas/source.json` lets an operator change.

use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub device: String,
    /// Decibels, 0 is unity.
    pub gain_db: f64,
    pub muted: bool,
    pub label: String,
    pub element: String,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            device: String::new(),
            gain_db: 0.0,
            muted: false,
            label: String::new(),
            element: String::new(),
        }
    }
}

impl Settings {
    pub fn from(params: &Value) -> Settings {
        Settings {
            device: text(params, "device"),
            gain_db: params
                .get("gain_db")
                .and_then(Value::as_f64)
                .unwrap_or(0.0)
                .clamp(-60.0, 12.0),
            muted: params
                .get("muted")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            label: text(params, "label"),
            element: text(params, "element"),
        }
    }

    /// Only the device and the element need the input reopened. Gain and mute
    /// are properties of an element that is already running, and a lectern mic
    /// that went quiet because someone typed a number is the wrong failure.
    pub fn needs_restart(&self, next: &Settings) -> bool {
        self.device != next.device || self.element != next.element
    }

    /// Decibels as the linear multiplier `volume` wants. 0 dB is 1.0, and
    /// anything at or under the floor is silence rather than a very small
    /// number that still leaks hiss.
    pub fn linear_gain(&self) -> f64 {
        if self.muted || self.gain_db <= -60.0 {
            return 0.0;
        }
        10f64.powf(self.gain_db / 20.0)
    }
}

fn text(params: &Value, key: &str) -> String {
    params
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_empty_object_is_unity_gain_and_not_muted() {
        let s = Settings::from(&json!({}));
        assert_eq!(s, Settings::default());
        assert_eq!(s.gain_db, 0.0);
        assert!(!s.muted);
        assert_eq!(s.linear_gain(), 1.0);
    }

    #[test]
    fn gain_is_clamped_to_what_the_schema_allows() {
        assert_eq!(Settings::from(&json!({"gain_db": 99})).gain_db, 12.0);
        assert_eq!(Settings::from(&json!({"gain_db": -400})).gain_db, -60.0);
    }

    #[test]
    fn six_decibels_down_is_half_the_voltage_near_enough() {
        let s = Settings::from(&json!({"gain_db": -6.0}));
        assert!(
            (s.linear_gain() - 0.5012).abs() < 0.001,
            "{}",
            s.linear_gain()
        );
    }

    #[test]
    fn muted_and_the_floor_are_both_silence_and_not_nearly_silence() {
        assert_eq!(Settings::from(&json!({"muted": true})).linear_gain(), 0.0);
        assert_eq!(Settings::from(&json!({"gain_db": -60})).linear_gain(), 0.0);
        assert_eq!(
            Settings::from(&json!({"gain_db": 0, "muted": true})).linear_gain(),
            0.0
        );
    }

    #[test]
    fn only_the_device_and_the_element_need_the_input_reopened() {
        let a = Settings::from(&json!({"device": "desk"}));
        assert!(!a.needs_restart(&Settings::from(&json!({"device": "desk", "gain_db": 6}))));
        assert!(!a.needs_restart(&Settings::from(&json!({"device": "desk", "muted": true}))));
        assert!(a.needs_restart(&Settings::from(&json!({"device": "lectern"}))));
        assert!(a.needs_restart(&Settings::from(
            &json!({"device": "desk", "element": "alsasrc"})
        )));
    }
}
