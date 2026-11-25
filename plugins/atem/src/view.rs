//! What `list`, `state` and `tally` answer, built from a switcher's snapshot
//! and its entry. Numbers count from zero, as the switcher counts them;
//! the transition position is 0 to 1.

use godwinmix_atem::state::{MixEffect, State, Version};
use godwinmix_atem::{LinkStatus, Snapshot};
use serde_json::{json, Value};

use crate::desk::{Desk, Entry};

/// The firmware a protocol version shipped with, from community notes.
pub fn firmware(v: Version) -> Option<&'static str> {
    Some(match (v.major, v.minor) {
        (2, 27) => "7.5.2",
        (2, 28) => "8.0",
        (2, 29) => "8.0.1",
        (2, 30) => "8.1.1",
        (2, 31) => "9.4",
        (2, 32) => "9.6",
        _ => return None,
    })
}

/// One row of `list`.
pub fn row(s: &Snapshot, e: &Entry) -> Value {
    json!({
        "id": s.id, "name": e.name, "address": e.address,
        "connected": s.link.is_up(), "status": s.link.status, "why": s.link.reason,
        "mode": e.mode.as_str(), "product": s.state.product,
    })
}

fn style_name(t: &godwinmix_atem::state::Transition) -> Value {
    t.style.map(|s| json!(s)).unwrap_or(Value::Null)
}

fn mix_effect(i: usize, m: &MixEffect) -> Value {
    let t = &m.transition;
    let rate = match t.style.map(|s| s as u8) {
        Some(0) => Some(t.mix_rate),
        Some(1) => Some(t.dip_rate),
        Some(2) => Some(t.wipe_rate),
        _ => None,
    };
    let f = &m.fade_to_black;
    json!({
        "me": i, "program": m.program, "preview": m.preview,
        "transition": {
            "style": style_name(t), "next_style": t.next_style, "rate": rate,
            "rates": {"mix": t.mix_rate, "dip": t.dip_rate, "wipe": t.wipe_rate},
            "position": f64::from(t.position) / 10_000.0, "in_transition": t.in_transition,
        },
        "usk": m.keyers.iter().enumerate().map(|(k, u)| json!({"keyer": k, "on_air": u.on_air})).collect::<Vec<_>>(),
        "ftb": {"on": f.fully_black || f.in_transition, "fully_black": f.fully_black, "in_transition": f.in_transition, "rate": f.rate},
    })
}

/// The whole state of one switcher, as `state` answers it.
pub fn state(s: &Snapshot, e: &Entry) -> Value {
    let st = &s.state;
    let mut out = row(s, e);
    let more = json!({
        "sessions": s.link.sessions,
        "syncing": s.link.status == LinkStatus::Syncing,
        "model": st.model,
        "protocol": st.protocol.map(|v| format!("{}.{}", v.major, v.minor)),
        "firmware": st.protocol.and_then(firmware),
        "inputs": st.inputs.iter().map(|(id, i)| json!({"id": id, "long": i.long_name, "short": i.short_name})).collect::<Vec<_>>(),
        "mes": st.mix_effects.iter().enumerate().map(|(i, m)| mix_effect(i, m)).collect::<Vec<_>>(),
        "dsk": st.downstream_keyers.iter().enumerate().map(|(k, d)| json!({
            "keyer": k, "on_air": d.on_air, "tie": d.tie, "rate": d.rate,
            "in_transition": d.in_transition, "fill": d.fill_source, "key": d.key_source,
        })).collect::<Vec<_>>(),
        "aux": st.aux.iter().enumerate().map(|(a, i)| json!({"aux": a, "input": i})).collect::<Vec<_>>(),
        "macros": st.macros.iter().map(|(i, m)| json!({"index": i, "name": m.name, "description": m.description})).collect::<Vec<_>>(),
        "macro": {"running": st.macro_run.running, "index": st.macro_run.index, "last_run": e.last_macro},
        "tally": tally(st, e),
        "map": e.map,
    });
    out.as_object_mut().unwrap().extend(more.as_object().unwrap().clone());
    out
}

/// Inputs on programme and preview, and the same by GodwinMix source id.
pub fn tally(st: &State, e: &Entry) -> Value {
    let on = |f: fn(&godwinmix_atem::state::Tally) -> bool| st.tally.iter().filter(|(_, t)| f(t)).map(|(i, _)| *i).collect::<Vec<u16>>();
    let (program, preview) = (on(|t| t.program), on(|t| t.preview));
    let mut sources = serde_json::Map::new();
    for (source, input) in &e.map {
        let word = if program.contains(input) { "program" } else if preview.contains(input) { "preview" } else { "off" };
        sources.insert(source.clone(), json!(word));
    }
    json!({"program": program, "preview": preview, "sources": sources})
}

/// Every mapped source's tally across every switcher: programme on any
/// switcher wins over preview on another.
pub fn tally_all(desk: &Desk) -> Value {
    let mut sources = serde_json::Map::new();
    for atem in desk.switchers.iter() {
        let s = atem.snapshot();
        let t = tally(&s.state, &desk.entries[atem.id()]);
        for (source, word) in t["sources"].as_object().into_iter().flatten() {
            let rank = |w: &Value| ["off", "preview", "program"].iter().position(|x| w == x).unwrap_or(0);
            if !sources.get(source).is_some_and(|old| rank(word) <= rank(old)) {
                sources.insert(source.clone(), word.clone());
            }
        }
    }
    json!({"sources": sources})
}
