//! Macros and media players.

use super::read::{bool_at, text_len, u16_at, u8_at};
use super::{slot, Macro, MacroRun, MediaKind, State};

/// `MPrp`: index, used, has unsupported steps, name length, description
/// length, then the name and the description. An unused slot is removed.
pub fn macro_properties(state: &mut State, b: &[u8]) -> Option<()> {
    let index = u16_at(b, 0)?;
    if !bool_at(b, 2)? {
        state.macros.remove(&index);
        return Some(());
    }
    let name_len = usize::from(u16_at(b, 4)?);
    let desc_len = usize::from(u16_at(b, 6)?);
    let name = text_len(b, 8, name_len)?;
    let description = text_len(b, 8 + name_len, desc_len)?;
    state.macros.insert(index, Macro { name, description });
    Some(())
}

/// `MRPr`: a status byte (bit 0 running, bit 1 waiting), loop, the index.
pub fn macro_run(state: &mut State, b: &[u8]) -> Option<()> {
    let status = u8_at(b, 0)?;
    let index = u16_at(b, 2)?;
    let running = status & 1 != 0;
    state.macro_run = MacroRun {
        running,
        waiting: status & 2 != 0,
        looping: bool_at(b, 1)?,
        index: (running && index != 0xffff).then_some(index),
    };
    Some(())
}

/// `MPCE`: player, source type (1 still, 2 clip), still index, clip index.
pub fn player(state: &mut State, b: &[u8]) -> Option<()> {
    let kind = if u8_at(b, 1)? == 2 { MediaKind::Clip } else { MediaKind::Still };
    let (still, clip) = (u8_at(b, 2)?, u8_at(b, 3)?);
    let p = slot(&mut state.media_players, usize::from(u8_at(b, 0)?));
    p.source = kind;
    p.still = still;
    p.clip = clip;
    Some(())
}
