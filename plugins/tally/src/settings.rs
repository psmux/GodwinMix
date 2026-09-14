//! What the operator set, read out of the validated settings object.

use serde_json::Value;

use crate::tsl::Lamp;

/// The port TSL's own documentation uses in its examples, and what most tally
/// interfaces are shipped set to.
pub const DEFAULT_PORT: u16 = 8900;

/// One lamp: which source it watches, which UMD index it is, what is written
/// on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lampinfo {
    pub source: String,
    pub index: u16,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// `udp` or `tcp`. UDP is what a rack of lamps usually wants; TCP is what
    /// a multiviewer or a router usually wants.
    pub protocol: Protocol,
    /// Where the packets go, as `address:port`.
    pub address: String,
    /// The display group. A rig with one tally interface leaves this at 0.
    pub screen: u16,
    /// Source id to lamp. A source with no entry lights nothing.
    pub lamps: Vec<Lampinfo>,
    pub program_colour: Lamp,
    pub preview_colour: Lamp,
    /// 0 dimmest to 3 brightest.
    pub brightness: u8,
    /// Write labels as UTF-16LE rather than ASCII.
    pub unicode: bool,
    /// Send the whole set again this often, so a lamp that was power cycled
    /// catches up without anyone taking a source. 0 turns it off.
    pub refresh_secs: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    Udp,
    Tcp,
}

impl Protocol {
    pub fn parse(name: &str) -> Option<Protocol> {
        match name {
            "udp" => Some(Protocol::Udp),
            "tcp" => Some(Protocol::Tcp),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Protocol::Udp => "udp",
            Protocol::Tcp => "tcp",
        }
    }
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            protocol: Protocol::Udp,
            address: format!("127.0.0.1:{DEFAULT_PORT}"),
            screen: 0,
            lamps: Vec::new(),
            program_colour: Lamp::Red,
            preview_colour: Lamp::Green,
            brightness: 3,
            unicode: false,
            refresh_secs: 10,
        }
    }
}

impl Settings {
    pub fn from_value(value: &Value) -> Settings {
        let mut settings = Settings::default();
        if let Some(protocol) = value.get("protocol").and_then(Value::as_str) {
            if let Some(protocol) = Protocol::parse(protocol) {
                settings.protocol = protocol;
            }
        }
        if let Some(address) = value.get("address").and_then(Value::as_str) {
            settings.address = with_host(address);
        }
        if let Some(screen) = value.get("screen").and_then(Value::as_u64) {
            settings.screen = screen.min(u16::MAX as u64) as u16;
        }
        if let Some(list) = value.get("lamps").and_then(Value::as_array) {
            settings.lamps = list.iter().enumerate().filter_map(lamp_from).collect();
        }
        if let Some(colour) = value.get("program_colour").and_then(Value::as_str) {
            if let Some(lamp) = Lamp::parse(colour) {
                settings.program_colour = lamp;
            }
        }
        if let Some(colour) = value.get("preview_colour").and_then(Value::as_str) {
            if let Some(lamp) = Lamp::parse(colour) {
                settings.preview_colour = lamp;
            }
        }
        if let Some(brightness) = value.get("brightness").and_then(Value::as_u64) {
            settings.brightness = brightness.min(3) as u8;
        }
        if let Some(on) = value.get("unicode").and_then(Value::as_bool) {
            settings.unicode = on;
        }
        if let Some(secs) = value.get("refresh_secs").and_then(Value::as_u64) {
            settings.refresh_secs = secs;
        }
        settings
    }

    /// The lamp watching this source, if any.
    pub fn lamp_for(&self, source: &str) -> Option<&Lampinfo> {
        self.lamps.iter().find(|lamp| lamp.source == source)
    }

    /// The colour a lamp shows for a tally state.
    pub fn colour_for(&self, state: &str) -> Lamp {
        match state {
            "program" => self.program_colour,
            "preview" => self.preview_colour,
            _ => Lamp::Off,
        }
    }
}

/// An entry is `{source, index, label}`. `index` defaults to the position in
/// the list, which is what an operator who numbered their cameras in order
/// expects, and `label` defaults to the source id.
fn lamp_from((position, value): (usize, &Value)) -> Option<Lampinfo> {
    let source = value.get("source").and_then(Value::as_str)?;
    if source.is_empty() {
        return None;
    }
    let index = value
        .get("index")
        .and_then(Value::as_u64)
        .unwrap_or(position as u64)
        .min(u16::MAX as u64) as u16;
    let label = value
        .get("label")
        .and_then(Value::as_str)
        .filter(|l| !l.is_empty())
        .unwrap_or(source)
        .to_string();
    Some(Lampinfo {
        source: source.to_string(),
        index,
        label,
    })
}

fn with_host(text: &str) -> String {
    let text = text.trim();
    if text.contains(':') {
        text.to_string()
    } else if text.chars().all(|c| c.is_ascii_digit()) {
        format!("127.0.0.1:{text}")
    } else {
        format!("{text}:{DEFAULT_PORT}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_defaults_are_the_ones_in_the_schema() {
        let settings = Settings::from_value(&json!({}));
        assert_eq!(settings, Settings::default());
        assert_eq!(settings.protocol, Protocol::Udp);
        assert_eq!(settings.address, "127.0.0.1:8900");
        assert_eq!(settings.program_colour, Lamp::Red);
        assert_eq!(settings.preview_colour, Lamp::Green);
    }

    #[test]
    fn a_lamp_takes_its_index_from_its_position_when_nobody_said() {
        let settings = Settings::from_value(&json!({
            "lamps": [{"source": "cam1"}, {"source": "cam2"}, {"source": "cam3", "index": 9}]
        }));
        assert_eq!(settings.lamps[0].index, 0);
        assert_eq!(settings.lamps[1].index, 1);
        assert_eq!(settings.lamps[2].index, 9);
        assert_eq!(settings.lamps[0].label, "cam1", "the label falls back to the id");
    }

    #[test]
    fn a_lamp_with_no_source_is_dropped_rather_than_lighting_nothing() {
        let settings = Settings::from_value(&json!({
            "lamps": [{"index": 3}, {"source": ""}, {"source": "cam1", "label": "CAM 1"}]
        }));
        assert_eq!(settings.lamps.len(), 1);
        assert_eq!(settings.lamps[0].label, "CAM 1");
    }

    #[test]
    fn a_source_with_no_lamp_is_not_found() {
        let settings = Settings::from_value(&json!({"lamps": [{"source": "cam1"}]}));
        assert!(settings.lamp_for("cam1").is_some());
        assert!(settings.lamp_for("cam2").is_none());
    }

    #[test]
    fn the_colours_are_the_ones_the_operator_chose() {
        let settings = Settings::from_value(&json!({
            "program_colour": "amber", "preview_colour": "red"
        }));
        assert_eq!(settings.colour_for("program"), Lamp::Amber);
        assert_eq!(settings.colour_for("preview"), Lamp::Red);
        assert_eq!(settings.colour_for("off"), Lamp::Off);
        assert_eq!(settings.colour_for("anything else"), Lamp::Off);
    }

    #[test]
    fn a_colour_nobody_has_leaves_the_default_alone() {
        let settings = Settings::from_value(&json!({"program_colour": "puce"}));
        assert_eq!(settings.program_colour, Lamp::Red);
    }

    #[test]
    fn brightness_is_held_inside_its_two_bits() {
        assert_eq!(Settings::from_value(&json!({"brightness": 99})).brightness, 3);
        assert_eq!(Settings::from_value(&json!({"brightness": 0})).brightness, 0);
    }

    #[test]
    fn a_bare_port_or_host_becomes_an_address() {
        assert_eq!(Settings::from_value(&json!({"address": "8901"})).address, "127.0.0.1:8901");
        assert_eq!(
            Settings::from_value(&json!({"address": "tally.local"})).address,
            "tally.local:8900"
        );
    }

    #[test]
    fn tcp_is_chosen_by_name() {
        assert_eq!(Settings::from_value(&json!({"protocol": "tcp"})).protocol, Protocol::Tcp);
        assert_eq!(Protocol::Tcp.name(), "tcp");
        assert_eq!(Protocol::parse("sctp"), None);
    }
}
