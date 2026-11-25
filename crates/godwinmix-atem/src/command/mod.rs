//! What GodwinMix can ask a switcher to do.
//!
//! Each variant is one command block. [`Command::encode`] gives the whole
//! block, header included, so a test can compare it byte for byte.
//! Mix effect, keyer, DSK, aux and media player numbers count from zero,
//! the way the switcher does; the plugin adds one when it talks to people.

mod encode;

use serde::{Deserialize, Serialize};

use crate::state::Version;

/// The five transition styles, numbered as the switcher numbers them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransitionStyle {
    Mix = 0,
    Dip = 1,
    Wipe = 2,
    Dve = 3,
    Sting = 4,
}

impl TransitionStyle {
    pub fn from_u8(value: u8) -> Option<TransitionStyle> {
        Some(match value {
            0 => TransitionStyle::Mix,
            1 => TransitionStyle::Dip,
            2 => TransitionStyle::Wipe,
            3 => TransitionStyle::Dve,
            4 => TransitionStyle::Sting,
            _ => return None,
        })
    }
}

/// The styles whose rate this client can set. DVE and stinger rates sit in
/// longer commands that are not implemented yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RateStyle {
    Mix,
    Dip,
    Wipe,
}

/// One command to a switcher.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Put a source on programme straight away (`CPgI`).
    Program { me: u8, source: u16 },
    /// Put a source on preview (`CPvI`).
    Preview { me: u8, source: u16 },
    /// Swap programme and preview with no transition (`DCut`).
    Cut { me: u8 },
    /// Run the selected transition at its rate (`DAut`).
    Auto { me: u8 },
    /// Move the transition bar, 0 to 10000 (`CTPs`).
    TransitionPosition { me: u8, position: u16 },
    /// Choose the next transition style (`CTTp`, style only).
    TransitionStyle { me: u8, style: TransitionStyle },
    /// Set a style's rate in frames (`CTMx`, `CTDp` or `CTWp`).
    TransitionRate { me: u8, style: RateStyle, frames: u8 },
    /// Put a downstream keyer on or off air at once (`CDsL`).
    DskOnAir { dsk: u8, on_air: bool },
    /// Run a downstream keyer's own transition (`DDsA`).
    DskAuto { dsk: u8 },
    /// Tie a downstream keyer to the next transition (`CDsT`).
    DskTie { dsk: u8, tie: bool },
    /// A downstream keyer's rate in frames (`CDsR`).
    DskRate { dsk: u8, frames: u8 },
    /// Put an upstream keyer on or off air (`CKOn`).
    UskOnAir { me: u8, keyer: u8, on_air: bool },
    /// Fade to black, or back from it (`FtbA`).
    FadeToBlack { me: u8 },
    /// The fade to black rate in frames (`FtbC`).
    FadeToBlackRate { me: u8, frames: u8 },
    /// Route a source to an aux output (`CAuS`).
    AuxSource { aux: u8, source: u16 },
    /// Run a stored macro (`MAct`).
    MacroRun { index: u16 },
    /// Stop the macro that is running (`MAct`).
    MacroStop,
    /// Start recording a macro into a slot (`MSRc`).
    MacroRecord { index: u16, name: String, description: String },
    /// Stop recording (`MAct`).
    MacroStopRecording,
    /// Show a still from the media pool on a media player (`MPSS`).
    MediaPlayerStill { player: u8, still: u8 },
}

impl Command {
    /// The four byte block name.
    pub fn name(&self) -> &'static [u8; 4] {
        encode::name(self)
    }

    /// The whole command block, header included, as the newest firmware
    /// takes it.
    pub fn encode(&self) -> Vec<u8> {
        self.encode_for(None)
    }

    /// The whole block for a switcher that reported `protocol` in `_ver`.
    /// A few layouts changed between versions; `None` means the newest.
    pub fn encode_for(&self, protocol: Option<Version>) -> Vec<u8> {
        crate::wire::block(self.name(), &encode::body(self, protocol))
    }
}

#[cfg(test)]
mod tests;
