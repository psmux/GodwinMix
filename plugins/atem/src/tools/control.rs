//! The tools that drive a switcher's M/E: cut, auto, fade to black, the
//! programme and preview buses, and the transition.

use godwinmix_atem::{Atem, Command, RateStyle, TransitionStyle};
use serde_json::{json, Value};

use super::{answer, args, bad, refuse, Outcome};
use crate::desk::Desk;
use godwinmix_sdk::prelude::RpcError;

pub(super) fn send(atem: &Atem, commands: Vec<Command>) -> Result<(), RpcError> {
    atem.send_now(commands).map_err(refuse)
}

/// Refuse a number past what the switcher said it has, once it has said.
pub(super) fn within(n: usize, have: usize, what: &str) -> Result<(), RpcError> {
    if have > 0 && n >= have {
        return Err(bad(format!("this switcher has {have} {what}, numbered from 0, so {n} is not one")));
    }
    Ok(())
}

pub(super) fn done(text: String, atem: &Atem, more: Value) -> Outcome {
    let mut facts = json!({"switcher": atem.id(), "sent": true});
    facts.as_object_mut().unwrap().extend(more.as_object().cloned().unwrap_or_default());
    Ok(answer(text, facts))
}

/// `cut`, `auto` and `ftb`: one M/E, nothing else.
pub fn me_action(desk: &Desk, name: &str, a: &Value) -> Outcome {
    let (atem, _) = desk.pick(args::switcher(a)).map_err(refuse)?;
    let me = args::me(a)?;
    within(usize::from(me), atem.snapshot().state.mix_effects.len(), "M/Es")?;
    let (command, words) = match name {
        "cut" => (Command::Cut { me }, "Cut"),
        "auto" => (Command::Auto { me }, "Ran the transition"),
        _ => (Command::FadeToBlack { me }, "Faded to or from black"),
    };
    send(atem, vec![command])?;
    done(format!("{words} on {} M/E {}.", atem.id(), me + 1), atem, json!({"me": me}))
}

/// `preview` and `program`: an input on a bus.
pub fn bus(desk: &Desk, name: &str, a: &Value) -> Outcome {
    let (atem, entry) = desk.pick(args::switcher(a)).map_err(refuse)?;
    let me = args::me(a)?;
    let input = args::input(a, "input", entry)?;
    let state = atem.snapshot().state;
    within(usize::from(me), state.mix_effects.len(), "M/Es")?;
    if !state.inputs.is_empty() && !state.inputs.contains_key(&input) {
        return Err(bad(format!("{} has no input {input}. state lists the inputs it has", atem.id())));
    }
    let command = if name == "program" { Command::Program { me, source: input } } else { Command::Preview { me, source: input } };
    send(atem, vec![command])?;
    let words = if name == "program" { "on programme" } else { "on preview" };
    done(format!("{} is {words} on {}.", state.input_name(input), atem.id()), atem, json!({"me": me, "input": input}))
}

/// `transition`: the style, its rate in frames, and the bar from 0 to 1.
pub fn transition(desk: &Desk, a: &Value) -> Outcome {
    let (atem, _) = desk.pick(args::switcher(a)).map_err(refuse)?;
    let me = args::me(a)?;
    let mut commands = Vec::new();
    let style = match a.get("style").and_then(Value::as_str) {
        Some(s) => Some(serde_json::from_value::<TransitionStyle>(json!(s)).map_err(|_| bad(format!("style '{s}' is not one; use mix, dip, wipe, dve or sting")))?),
        None => None,
    };
    if let Some(style) = style {
        commands.push(Command::TransitionStyle { me, style });
    }
    if let Some(rate) = a.get("rate").filter(|v| !v.is_null()) {
        let frames = rate.as_u64().filter(|n| (1..=250).contains(n)).ok_or_else(|| bad(format!("rate is {rate}; give frames from 1 to 250, such as 25 for one second at 25p")))? as u8;
        let current = atem.snapshot().state.mix_effects.get(usize::from(me)).and_then(|m| m.transition.style);
        let style = match style.or(current) {
            Some(TransitionStyle::Dip) => RateStyle::Dip,
            Some(TransitionStyle::Wipe) => RateStyle::Wipe,
            Some(TransitionStyle::Dve | TransitionStyle::Sting) => return Err(bad("setting the rate of a DVE or stinger transition is not supported yet. Set it on the switcher, or pick mix, dip or wipe")),
            _ => RateStyle::Mix,
        };
        commands.push(Command::TransitionRate { me, style, frames });
    }
    if let Some(p) = a.get("position").filter(|v| !v.is_null()) {
        let p = p.as_f64().filter(|p| (0.0..=1.0).contains(p)).ok_or_else(|| bad(format!("position is {p}; give 0 to 1, where 1 is the end of the bar")))?;
        commands.push(Command::TransitionPosition { me, position: (p * 10_000.0).round() as u16 });
    }
    if commands.is_empty() {
        return Err(bad("give style, rate or position. Example: transition {style: \"mix\", rate: 25}"));
    }
    send(atem, commands)?;
    done(format!("Transition set on {} M/E {}.", atem.id(), me + 1), atem, json!({"me": me}))
}
