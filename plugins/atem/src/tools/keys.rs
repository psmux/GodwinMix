//! The keyers, aux outputs and macros.

use godwinmix_atem::Command;
use serde_json::{json, Value};

use super::control::{done, send, within};
use super::{args, bad, refuse, Outcome};
use crate::desk::Desk;

/// `dsk`: on, off, auto (its own transition) or tie.
pub fn dsk(desk: &Desk, a: &Value) -> Outcome {
    let (atem, _) = desk.pick(args::switcher(a)).map_err(refuse)?;
    let keyer = args::index(a, "keyer", Some(0), "{keyer: 0, action: \"auto\"}")?;
    let state = atem.snapshot().state;
    within(usize::from(keyer), state.downstream_keyers.len(), "downstream keyers")?;
    let tie_now = state.downstream_keyers.get(usize::from(keyer)).is_some_and(|d| d.tie);
    let command = match args::text(a, "action", "{keyer: 0, action: \"auto\"}")? {
        "on" => Command::DskOnAir { dsk: keyer, on_air: true },
        "off" => Command::DskOnAir { dsk: keyer, on_air: false },
        "auto" => Command::DskAuto { dsk: keyer },
        "tie" => Command::DskTie { dsk: keyer, tie: a.get("tie").and_then(Value::as_bool).unwrap_or(!tie_now) },
        other => return Err(bad(format!("action '{other}' is not one; use on, off, auto or tie"))),
    };
    send(atem, vec![command])?;
    done(format!("DSK {} on {}: done.", keyer + 1, atem.id()), atem, json!({"keyer": keyer}))
}

/// `usk`: an upstream keyer on or off air.
pub fn usk(desk: &Desk, a: &Value) -> Outcome {
    let (atem, _) = desk.pick(args::switcher(a)).map_err(refuse)?;
    let me = args::me(a)?;
    let keyer = args::index(a, "keyer", Some(0), "{keyer: 0, on: true}")?;
    let on = args::flag(a, "on", "{keyer: 0, on: true}")?;
    let have = atem.snapshot().state.mix_effects.get(usize::from(me)).map_or(0, |m| m.keyers.len());
    within(usize::from(keyer), have, "upstream keyers on that M/E")?;
    send(atem, vec![Command::UskOnAir { me, keyer, on_air: on }])?;
    let words = if on { "on air" } else { "off air" };
    done(format!("Key {} on M/E {} is {words}.", keyer + 1, me + 1), atem, json!({"me": me, "keyer": keyer, "on": on}))
}

/// `aux`: route an input to an aux output.
pub fn aux(desk: &Desk, a: &Value) -> Outcome {
    let (atem, entry) = desk.pick(args::switcher(a)).map_err(refuse)?;
    let aux = args::index(a, "aux", None, "{aux: 0, input: 2}")?;
    let input = args::input(a, "input", entry)?;
    let state = atem.snapshot().state;
    within(usize::from(aux), usize::from(state.topology.aux).max(state.aux.len()), "aux outputs")?;
    send(atem, vec![Command::AuxSource { aux, source: input }])?;
    done(format!("Aux {} shows {}.", aux + 1, state.input_name(input)), atem, json!({"aux": aux, "input": input}))
}

/// `macro`: run a stored macro, or stop the one running.
pub fn macro_action(desk: &mut Desk, a: &Value) -> Outcome {
    let (atem, _) = desk.pick(args::switcher(a)).map_err(refuse)?;
    let action = a.get("action").and_then(Value::as_str).unwrap_or("run");
    let index = match action {
        "run" => Some(a.get("index").and_then(Value::as_u64).and_then(|n| u16::try_from(n).ok()).ok_or_else(|| bad("give the macro's index, counted from 0, as state lists the macros. Example: {index: 0}"))?),
        "stop" => None,
        other => return Err(bad(format!("action '{other}' is not one; use run or stop"))),
    };
    let macros = atem.snapshot().state.macros;
    if let Some(i) = index.filter(|i| atem.snapshot().link.is_up() && !macros.contains_key(i)) {
        return Err(bad(format!("{} has no macro in slot {i}. state lists the macros saved on it", atem.id())));
    }
    send(atem, vec![index.map_or(Command::MacroStop, |index| Command::MacroRun { index })])?;
    let outcome = done(format!("Macro {action} sent to {}.", atem.id()), atem, json!({"action": action, "index": index}));
    let id = atem.id().to_string();
    if index.is_some() {
        desk.entry_mut(Some(&id)).map_err(refuse)?.1.last_macro = index;
    }
    outcome
}
