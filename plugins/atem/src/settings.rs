//! `schemas/atem.json`, read into structs. Anything wrong in one switcher is
//! a problem to report, never a reason to drop the others.

use std::collections::BTreeMap;

use serde_json::Value;

/// How a switcher and GodwinMix's programme move together.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// The switcher is driven only from the window or by agents.
    #[default]
    Own,
    /// A take in GodwinMix also cuts the switcher to the mapped input.
    Follow,
    /// A programme change on the switcher moves GodwinMix to the mapped source.
    Lead,
}

impl Mode {
    pub const WORDS: &'static str = "own, follow or lead";

    pub fn parse(text: &str) -> Option<Mode> {
        match text {
            "own" => Some(Mode::Own),
            "follow" => Some(Mode::Follow),
            "lead" => Some(Mode::Lead),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Own => "own",
            Mode::Follow => "follow",
            Mode::Lead => "lead",
        }
    }
}

/// One switcher as the settings or `connect` give it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherSpec {
    pub id: String,
    pub address: String,
    pub name: Option<String>,
    pub mode: Mode,
    /// GodwinMix source id to ATEM input number.
    pub map: BTreeMap<String, u16>,
}

impl SwitcherSpec {
    /// Read one from JSON, or say what is wrong with it.
    pub fn from_value(v: &Value) -> Result<SwitcherSpec, String> {
        let text = |key: &str| v.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty());
        let id = text("id").ok_or("a switcher needs an id, such as atem-main")?.to_string();
        let address = text("address").ok_or_else(|| format!("switcher '{id}' needs an address, such as 192.0.2.10"))?.to_string();
        let mode = match text("mode") {
            None => Mode::Own,
            Some(m) => Mode::parse(m).ok_or_else(|| format!("switcher '{id}' has mode '{m}'; use {}", Mode::WORDS))?,
        };
        let mut map = BTreeMap::new();
        for (source, input) in v.get("map").and_then(Value::as_object).into_iter().flatten() {
            let input = input
                .as_u64()
                .and_then(|n| u16::try_from(n).ok())
                .ok_or_else(|| format!("switcher '{id}' maps '{source}' to {input}; give an ATEM input number such as 1"))?;
            map.insert(source.clone(), input);
        }
        Ok(SwitcherSpec { id, address, name: text("name").map(str::to_string), mode, map })
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Settings {
    pub switchers: Vec<SwitcherSpec>,
    pub problems: Vec<String>,
}

impl Settings {
    pub fn from_value(v: &Value) -> Settings {
        let mut out = Settings::default();
        for item in v.get("switchers").and_then(Value::as_array).into_iter().flatten() {
            match SwitcherSpec::from_value(item) {
                Ok(spec) => out.switchers.push(spec),
                Err(problem) => out.problems.push(problem),
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_good_switcher_and_a_bad_one() {
        let s = Settings::from_value(&json!({"switchers": [
            {"id": "atem-main", "address": "192.0.2.10", "mode": "follow", "map": {"cam-pulpit": 1}},
            {"id": "atem-overflow", "mode": "sideways"},
        ]}));
        assert_eq!(s.switchers.len(), 1);
        assert_eq!(s.switchers[0].mode, Mode::Follow);
        assert_eq!(s.switchers[0].map["cam-pulpit"], 1);
        assert_eq!(s.problems, ["switcher 'atem-overflow' needs an address, such as 192.0.2.10"]);
        assert!(SwitcherSpec::from_value(&json!({"id": "a", "address": "x", "mode": "up"})).unwrap_err().contains("own, follow or lead"));
    }
}
