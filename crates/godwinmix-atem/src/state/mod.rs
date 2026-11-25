//! What a switcher has told us about itself.
//!
//! The switcher sends its whole state as command blocks when a session opens
//! and then one block per change. [`State::apply`] folds one block in. Blocks
//! this client does not know, and blocks too short for their layout, are left
//! out without an error: a newer firmware adds fields at the end and new
//! commands, and neither may stop the rest of the state arriving.

mod device;
mod keys;
mod media;
mod me;
mod read;
mod types;

use std::collections::BTreeMap;

use serde::Serialize;

use crate::wire::Block;

pub use types::*;

/// Everything this client keeps about one switcher.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct State {
    /// The product name, such as "ATEM Mini Pro".
    pub product: Option<String>,
    /// The product's model number as the switcher reports it.
    pub model: Option<u8>,
    /// The protocol version, which moves with the firmware.
    pub protocol: Option<Version>,
    pub topology: Topology,
    /// Every source the switcher offers, by its source number.
    pub inputs: BTreeMap<u16, Input>,
    /// One per M/E, counted from zero.
    pub mix_effects: Vec<MixEffect>,
    pub downstream_keyers: Vec<DownstreamKeyer>,
    /// The source routed to each aux output.
    pub aux: Vec<u16>,
    pub macros: BTreeMap<u16, Macro>,
    pub macro_run: MacroRun,
    pub media_players: Vec<MediaPlayer>,
    /// Tally by source number, from `TlSr`.
    pub tally: BTreeMap<u16, Tally>,
    /// True once the initial dump has ended (`InCm`).
    pub initialised: bool,
}

impl State {
    /// Fold one command block in. Returns true when the block was one this
    /// client reads, whether or not anything changed.
    pub fn apply(&mut self, block: Block<'_>) -> bool {
        let b = block.body;
        match &block.name {
            b"_ver" => device::version(self, b),
            b"_pin" => device::product(self, b),
            b"_top" => device::topology(self, b),
            b"InPr" => device::input(self, b),
            b"AuxS" => device::aux(self, b),
            b"TlSr" => device::tally(self, b),
            b"InCm" => {
                self.initialised = true;
                Some(())
            }
            b"_MeC" => me::config(self, b),
            b"PrgI" => me::program(self, b),
            b"PrvI" => me::preview(self, b),
            b"TrSS" => me::transition(self, b),
            b"TrPs" => me::position(self, b),
            b"TMxP" | b"TDpP" | b"TWpP" => me::rate(self, &block.name, b),
            b"KeOn" => me::key_on_air(self, b),
            b"FtbS" => me::ftb_state(self, b),
            b"FtbP" => me::ftb_rate(self, b),
            b"DskS" => keys::dsk_state(self, b),
            b"DskP" => keys::dsk_properties(self, b),
            b"DskB" => keys::dsk_sources(self, b),
            b"MPrp" => media::macro_properties(self, b),
            b"MRPr" => media::macro_run(self, b),
            b"MPCE" => media::player(self, b),
            _ => None,
        }
        .is_some()
    }

    /// The source on programme on an M/E, if the switcher has said.
    pub fn program(&self, me: u8) -> Option<u16> {
        self.mix_effects.get(usize::from(me)).map(|m| m.program)
    }

    /// The source on preview on an M/E, if the switcher has said.
    pub fn preview(&self, me: u8) -> Option<u16> {
        self.mix_effects.get(usize::from(me)).map(|m| m.preview)
    }

    /// An input's long name, or its number when the switcher has not named it.
    pub fn input_name(&self, source: u16) -> String {
        match self.inputs.get(&source) {
            Some(input) if !input.long_name.is_empty() => input.long_name.clone(),
            _ => crate::source::describe(source),
        }
    }
}

/// The slot at `index`, growing the list with defaults to reach it.
pub(crate) fn slot<T: Default>(list: &mut Vec<T>, index: usize) -> &mut T {
    if list.len() <= index {
        list.resize_with(index + 1, T::default);
    }
    &mut list[index]
}

#[cfg(test)]
mod tests;
