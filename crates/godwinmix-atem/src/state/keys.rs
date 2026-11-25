//! Downstream keyers.

use super::read::{bool_at, u16_at, u8_at};
use super::{slot, DownstreamKeyer, State};

fn dsk(state: &mut State, index: u8) -> &mut DownstreamKeyer {
    slot(&mut state.downstream_keyers, usize::from(index))
}

/// `DskS`: keyer, on air, in transition, auto transitioning, frames remaining.
pub fn dsk_state(state: &mut State, b: &[u8]) -> Option<()> {
    let (on_air, moving, auto) = (bool_at(b, 1)?, bool_at(b, 2)?, bool_at(b, 3)?);
    let frames = u8_at(b, 4)?;
    let d = dsk(state, u8_at(b, 0)?);
    d.on_air = on_air;
    d.in_transition = moving;
    d.is_auto_transitioning = auto;
    d.frames_remaining = frames;
    Some(())
}

/// `DskP`: keyer, tie, rate, then the key settings this client leaves alone.
pub fn dsk_properties(state: &mut State, b: &[u8]) -> Option<()> {
    let (tie, rate) = (bool_at(b, 1)?, u8_at(b, 2)?);
    let d = dsk(state, u8_at(b, 0)?);
    d.tie = tie;
    d.rate = rate;
    Some(())
}

/// `DskB`: keyer, a pad byte, the fill source, the key source.
pub fn dsk_sources(state: &mut State, b: &[u8]) -> Option<()> {
    let (fill, key) = (u16_at(b, 2)?, u16_at(b, 4)?);
    let d = dsk(state, u8_at(b, 0)?);
    d.fill_source = fill;
    d.key_source = key;
    Some(())
}
