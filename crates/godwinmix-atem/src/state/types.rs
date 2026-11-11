//! The records [`State`](super::State) is made of.

use serde::Serialize;

use crate::command::TransitionStyle;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Version {
    pub major: u16,
    pub minor: u16,
}

/// How many of each thing the switcher has, from `_top`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Topology {
    pub mix_effects: u8,
    pub sources: u8,
    pub downstream_keyers: u8,
    pub aux: u8,
    pub mix_minus_outputs: u8,
    pub media_players: u8,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Input {
    pub long_name: String,
    pub short_name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct MixEffect {
    pub program: u16,
    pub preview: u16,
    pub transition: Transition,
    /// Upstream keyers, by keyer number.
    pub keyers: Vec<UpstreamKeyer>,
    pub fade_to_black: FadeToBlack,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Transition {
    pub style: Option<TransitionStyle>,
    pub next_style: Option<TransitionStyle>,
    /// Bit 0 the background, bit 1 and up the upstream keyers.
    pub selection: u8,
    pub next_selection: u8,
    pub in_transition: bool,
    pub frames_remaining: u8,
    /// Where the bar is, 0 to 10000.
    pub position: u16,
    pub mix_rate: u8,
    pub dip_rate: u8,
    pub wipe_rate: u8,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct UpstreamKeyer {
    pub on_air: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct FadeToBlack {
    pub fully_black: bool,
    pub in_transition: bool,
    pub frames_remaining: u8,
    pub rate: u8,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct DownstreamKeyer {
    pub on_air: bool,
    pub in_transition: bool,
    pub is_auto_transitioning: bool,
    pub frames_remaining: u8,
    pub tie: bool,
    pub rate: u8,
    pub fill_source: u16,
    pub key_source: u16,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Macro {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct MacroRun {
    pub running: bool,
    pub waiting: bool,
    pub looping: bool,
    /// The macro running, when one is.
    pub index: Option<u16>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct MediaPlayer {
    /// "still" or "clip".
    pub source: MediaKind,
    pub still: u8,
    pub clip: u8,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaKind {
    #[default]
    Still,
    Clip,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Tally {
    pub program: bool,
    pub preview: bool,
}
