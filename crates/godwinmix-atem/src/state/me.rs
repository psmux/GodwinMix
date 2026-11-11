//! One M/E: programme, preview, the transition, upstream keys and FTB.

use super::read::{bool_at, u16_at, u8_at};
use super::{slot, MixEffect, State};
use crate::command::TransitionStyle;

fn me(state: &mut State, index: u8) -> &mut MixEffect {
    slot(&mut state.mix_effects, usize::from(index))
}

/// `_MeC`: an M/E's number and how many upstream keyers it has.
pub fn config(state: &mut State, b: &[u8]) -> Option<()> {
    let keyers = usize::from(u8_at(b, 1)?);
    let me = me(state, u8_at(b, 0)?);
    me.keyers.resize_with(keyers, Default::default);
    Some(())
}

/// `PrgI`: M/E, a pad byte, the source.
pub fn program(state: &mut State, b: &[u8]) -> Option<()> {
    let source = u16_at(b, 2)?;
    me(state, u8_at(b, 0)?).program = source;
    Some(())
}

/// `PrvI`: M/E, a pad byte, the source, then bytes this client ignores.
pub fn preview(state: &mut State, b: &[u8]) -> Option<()> {
    let source = u16_at(b, 2)?;
    me(state, u8_at(b, 0)?).preview = source;
    Some(())
}

/// `TrSS`: M/E, style, selection, next style, next selection.
pub fn transition(state: &mut State, b: &[u8]) -> Option<()> {
    let (style, selection) = (u8_at(b, 1)?, u8_at(b, 2)?);
    let (next_style, next_selection) = (u8_at(b, 3)?, u8_at(b, 4)?);
    let t = &mut me(state, u8_at(b, 0)?).transition;
    t.style = TransitionStyle::from_u8(style);
    t.selection = selection;
    t.next_style = TransitionStyle::from_u8(next_style);
    t.next_selection = next_selection;
    Some(())
}

/// `TrPs`: M/E, in transition, frames remaining, a pad byte, the bar.
pub fn position(state: &mut State, b: &[u8]) -> Option<()> {
    let (in_transition, frames) = (bool_at(b, 1)?, u8_at(b, 2)?);
    let position = u16_at(b, 4)?;
    let t = &mut me(state, u8_at(b, 0)?).transition;
    t.in_transition = in_transition;
    t.frames_remaining = frames;
    t.position = position;
    Some(())
}

/// `TMxP`, `TDpP` and `TWpP` all start with the M/E and the rate.
pub fn rate(state: &mut State, name: &[u8; 4], b: &[u8]) -> Option<()> {
    let rate = u8_at(b, 1)?;
    let t = &mut me(state, u8_at(b, 0)?).transition;
    match name {
        b"TMxP" => t.mix_rate = rate,
        b"TDpP" => t.dip_rate = rate,
        _ => t.wipe_rate = rate,
    }
    Some(())
}

/// `KeOn`: M/E, keyer, on air.
pub fn key_on_air(state: &mut State, b: &[u8]) -> Option<()> {
    let (keyer, on_air) = (u8_at(b, 1)?, bool_at(b, 2)?);
    slot(&mut me(state, u8_at(b, 0)?).keyers, usize::from(keyer)).on_air = on_air;
    Some(())
}

/// `FtbS`: M/E, fully black, in transition, frames remaining.
pub fn ftb_state(state: &mut State, b: &[u8]) -> Option<()> {
    let (black, moving, frames) = (bool_at(b, 1)?, bool_at(b, 2)?, u8_at(b, 3)?);
    let f = &mut me(state, u8_at(b, 0)?).fade_to_black;
    f.fully_black = black;
    f.in_transition = moving;
    f.frames_remaining = frames;
    Some(())
}

/// `FtbP`: M/E, rate.
pub fn ftb_rate(state: &mut State, b: &[u8]) -> Option<()> {
    let rate = u8_at(b, 1)?;
    me(state, u8_at(b, 0)?).fade_to_black.rate = rate;
    Some(())
}
