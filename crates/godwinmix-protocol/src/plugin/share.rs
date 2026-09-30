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
    /// Params that say where the thing is read from, when the same name can
    /// mean different things in different places: two channel servers can
    /// each have a `live/main`. Sources share only when these match too.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scope: Vec<String>,
}

/// The bus kinds a provide may declare.
pub const BUSES: [&str; 2] = ["camera", "channel"];

impl Share {
    /// Everything wrong with this declaration, as `(key, message)` pairs
    /// relative to `share`.
    pub fn problems(&self) -> Vec<(String, String)> {
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
        out
    }

    /// The values of the named params, in order, as text. A param that is
    /// missing or empty is an empty string: two sources that both leave the
    /// device unset both mean the default one.
    pub fn values(&self, params: &Value) -> Vec<String> {
        text_of(&self.params, params)
    }

    /// The values of the `scope` params, the same way.
    pub fn scope_values(&self, params: &Value) -> Vec<String> {
        text_of(&self.scope, params)
    }
}

fn text_of(names: &[String], params: &Value) -> Vec<String> {
    names
        .iter()
        .map(|p| match params.get(p) {
            None | Some(Value::Null) => String::new(),
            Some(Value::String(s)) => s.trim().to_string(),
            Some(other) => other.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_camera_with_one_param_is_fine() {
        let share = Share { bus: "camera".into(), params: vec!["device".into()], ..Share::default() };
        assert!(share.problems().is_empty());
    }

    #[test]
    fn an_unknown_bus_and_no_params_say_what_to_write() {
        let share = Share { bus: "tape".into(), ..Share::default() };
        let problems = share.problems();
        assert!(problems.iter().any(|(k, m)| k == "bus" && m.contains("camera")));
        assert!(problems.iter().any(|(k, m)| k == "params" && m.contains("device")));
    }

    #[test]
    fn values_come_out_in_order_and_missing_is_empty() {
        let share = Share {
            bus: "camera".into(),
            params: vec!["monitor".into(), "region".into(), "device".into()],
            scope: vec!["relay".into()],
        };
        let params = json!({"monitor": 1, "region": " 0,0,640,360 ", "relay": "127.0.0.1:1935"});
        assert_eq!(share.values(&params), vec!["1", "0,0,640,360", ""]);
        assert_eq!(share.scope_values(&params), vec!["127.0.0.1:1935"]);
    }
}
