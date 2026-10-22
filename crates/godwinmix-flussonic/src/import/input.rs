//! A Flussonic `input` as a direct show's input.
//!
//! `mpts-udp://eno1@224.2.2.2:2001?program=7` and `udp://...` become
//! `udp://@224.2.2.2:2001` with `program` and `params.interface`, which is
//! how a direct show joins a group on one card. In Flussonic the part before
//! the `@` is the card; in GodwinMix it would be a source address, so it
//! moves to `params`. Every other scheme is passed through as written and
//! the station says whether it can read it.

use super::Note;
use crate::model::{Input, Source};
use serde_json::{json, Map, Value};
use std::net::Ipv4Addr;

/// Query keys of a UDP input this import understands.
const KNOWN_QUERY: &[&str] = &["program"];

/// The input spec of `show.add` for one Flussonic input.
pub fn spec(input: &Input, line: usize, notes: &mut Vec<Note>) -> Value {
    match &input.source {
        Source::Udp { interface, group, port, program, .. } => {
            udp(&input.url, interface.as_deref(), *group, *port, *program, line, notes)
        }
        Source::Url { .. } => json!({ "uri": input.url }),
    }
}

fn udp(url: &str, interface: Option<&str>, group: Ipv4Addr, port: u16, program: Option<u16>, line: usize, notes: &mut Vec<Note>) -> Value {
    let uri = if group.is_multicast() { format!("udp://@{group}:{port}") } else { format!("udp://{group}:{port}") };
    let mut spec = Map::new();
    spec.insert("uri".into(), Value::String(uri));
    if let Some(p) = program {
        spec.insert("program".into(), json!(p));
    }
    if let Some(i) = interface.filter(|i| !i.is_empty()) {
        spec.insert("params".into(), json!({ "interface": i }));
    }
    let query = url.split_once('?').map(|(_, q)| q).unwrap_or_default();
    for key in query.split('&').filter_map(|kv| kv.split('=').next()).filter(|k| !k.is_empty()) {
        if !KNOWN_QUERY.contains(&key) {
            notes.push(Note::ignored(line, format!("input option '{key}' in {}", redact(url))));
        }
    }
    Value::Object(spec)
}

/// A URL with any password in it replaced, for a report or an answer
/// that is not the config itself.
pub fn redact(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else { return url.to_string() };
    let end = rest.find(['/', '?']).unwrap_or(rest.len());
    let (authority, path) = rest.split_at(end);
    match authority.rsplit_once('@') {
        Some((userinfo, host)) if userinfo.contains(':') => {
            let user = userinfo.split(':').next().unwrap_or_default();
            format!("{scheme}://{user}:***@{host}{path}")
        }
        _ => url.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::parse_source;

    fn of(url: &str) -> (Value, Vec<Note>) {
        let mut notes = Vec::new();
        let input = Input { url: url.into(), source: parse_source(url).unwrap() };
        (spec(&input, 3, &mut notes), notes)
    }

    #[test]
    fn mpts_udp_moves_the_card_into_params() {
        let (v, notes) = of("mpts-udp://eth1@239.10.0.1:2001?program=7");
        assert_eq!(v, json!({"uri": "udp://@239.10.0.1:2001", "program": 7, "params": {"interface": "eth1"}}));
        assert!(notes.is_empty());
        let (v, _) = of("udp://239.1.1.1:5000");
        assert_eq!(v, json!({"uri": "udp://@239.1.1.1:5000"}));
        let (v, _) = of("udp://0.0.0.0:5000");
        assert_eq!(v, json!({"uri": "udp://0.0.0.0:5000"}));
    }

    #[test]
    fn other_schemes_pass_through_and_unknown_options_are_reported() {
        assert_eq!(of("rtmp://127.0.0.1:1935/live/program").0, json!({"uri": "rtmp://127.0.0.1:1935/live/program"}));
        let (_, notes) = of("udp://239.1.1.1:5000?program=1&pids=256");
        assert_eq!(notes.len(), 1);
        assert!(notes[0].what.contains("'pids'"), "{notes:?}");
        assert_eq!(redact("rtmp://bob:secret@host/app/key"), "rtmp://bob:***@host/app/key");
        assert_eq!(redact("rtmp://host/app/key"), "rtmp://host/app/key");
    }
}
