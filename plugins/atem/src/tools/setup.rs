//! The tools that say what is connected and set how each switcher is used.

use serde_json::{json, Value};

use super::{answer, args, bad, refuse, Outcome};
use crate::desk::Desk;
use crate::settings::{Mode, SwitcherSpec};
use crate::view;

/// `list`: every switcher, whether it is up, and why not when it is not.
pub fn list(desk: &Desk) -> Outcome {
    let rows: Vec<Value> = desk.switchers.iter().map(|a| view::row(&a.snapshot(), &desk.entries[a.id()])).collect();
    let up = rows.iter().filter(|r| r["connected"] == true).count();
    let text = match rows.len() {
        0 => "No switcher is connected. Add one with connect {id: \"atem-main\", address: \"192.0.2.10\"}.".to_string(),
        n => format!("{n} switcher(s), {up} up."),
    };
    Ok(answer(text, json!({"switchers": rows})))
}

/// `state`: everything one switcher has said about itself.
pub fn state(desk: &Desk, a: &Value) -> Outcome {
    let (atem, entry) = desk.pick(args::switcher(a)).map_err(refuse)?;
    let s = atem.snapshot();
    let text = if s.link.is_up() {
        format!("{} is up: {} on programme.", atem.id(), s.state.program(0).map(|p| s.state.input_name(p)).unwrap_or_default())
    } else {
        format!("{} is {}.", atem.id(), s.link.describe())
    };
    Ok(answer(text, view::state(&s, entry)))
}

/// `tally`: one switcher's tally, or every mapped source's across all.
pub fn tally(desk: &Desk, a: &Value) -> Outcome {
    let facts = match args::switcher(a) {
        Some(id) => {
            let (atem, entry) = desk.pick(Some(id)).map_err(refuse)?;
            view::tally(&atem.snapshot().state, entry)
        }
        None => view::tally_all(desk),
    };
    Ok(answer("Tally by input and by GodwinMix source.", facts))
}

/// `connect`: start talking to a switcher.
pub fn connect(desk: &mut Desk, a: &Value) -> Outcome {
    let spec = SwitcherSpec::from_value(a).map_err(bad)?;
    let id = spec.id.clone();
    desk.connect(spec, false).map_err(refuse)?;
    let (atem, entry) = desk.pick(Some(&id)).map_err(refuse)?;
    let text = format!("Connecting to {id} at {}. list says when it is up, and why not if it is not.", entry.address);
    Ok(answer(text, view::row(&atem.snapshot(), entry)))
}

/// `disconnect`: close a switcher's session and forget it.
pub fn disconnect(desk: &mut Desk, a: &Value) -> Outcome {
    let id = args::switcher(a).ok_or_else(|| bad("give the switcher to disconnect. Example: disconnect {switcher: \"atem-main\"}"))?.to_string();
    desk.disconnect(&id).map_err(refuse)?;
    Ok(answer(format!("Disconnected {id}."), json!({"switcher": id, "disconnected": true})))
}

/// `map`: which ATEM input shows a GodwinMix source. A null input unmaps.
pub fn map(desk: &mut Desk, a: &Value) -> Outcome {
    let source = args::text(a, "source", "map {source: \"cam-pulpit\", input: 1}")?.to_string();
    let (id, entry) = desk.entry_mut(args::switcher(a)).map_err(refuse)?;
    let text = match a.get("input") {
        None | Some(Value::Null) => {
            entry.map.remove(&source);
            format!("'{source}' is no longer mapped on {id}.")
        }
        Some(v) => {
            let input = v.as_u64().and_then(|n| u16::try_from(n).ok()).ok_or_else(|| bad(format!("input is {v}; give an ATEM input number such as 1")))?;
            entry.map.insert(source.clone(), input);
            format!("'{source}' is input {input} on {id}.")
        }
    };
    Ok(answer(text, json!({"switcher": id, "map": entry.map})))
}

/// `mode`: own, follow or lead.
pub fn mode(desk: &mut Desk, a: &Value) -> Outcome {
    let word = args::text(a, "mode", "mode {mode: \"follow\"}")?;
    let mode = Mode::parse(word).ok_or_else(|| bad(format!("mode '{word}' is not one; use {}", Mode::WORDS)))?;
    let (id, entry) = desk.entry_mut(args::switcher(a)).map_err(refuse)?;
    entry.mode = mode;
    let unmapped = entry.map.is_empty() && mode != Mode::Own;
    let mut text = match mode {
        Mode::Own => format!("{id} is on its own: only the window and agents drive it."),
        Mode::Follow => format!("{id} follows GodwinMix: a take also cuts it to the mapped input."),
        Mode::Lead => format!("{id} leads: a cut on its panel moves GodwinMix to the mapped source."),
    };
    if unmapped {
        text.push_str(" Nothing is mapped yet, so nothing will move: map {source, input} first.");
    }
    Ok(answer(text, json!({"switcher": id, "mode": mode.as_str(), "mapped": entry.map.len()})))
}
