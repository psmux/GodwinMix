//! Command bodies, one small function per layout.

use super::{Command, RateStyle};
use crate::state::Version;

/// Protocol 2.29 (firmware 8.0.1) put a mask byte in front of `DDsA`.
const DSK_AUTO_MASKED: Version = Version { major: 2, minor: 29 };

pub(super) fn name(command: &Command) -> &'static [u8; 4] {
    match command {
        Command::Program { .. } => b"CPgI",
        Command::Preview { .. } => b"CPvI",
        Command::Cut { .. } => b"DCut",
        Command::Auto { .. } => b"DAut",
        Command::TransitionPosition { .. } => b"CTPs",
        Command::TransitionStyle { .. } => b"CTTp",
        Command::TransitionRate { style: RateStyle::Mix, .. } => b"CTMx",
        Command::TransitionRate { style: RateStyle::Dip, .. } => b"CTDp",
        Command::TransitionRate { style: RateStyle::Wipe, .. } => b"CTWp",
        Command::DskOnAir { .. } => b"CDsL",
        Command::DskAuto { .. } => b"DDsA",
        Command::DskTie { .. } => b"CDsT",
        Command::DskRate { .. } => b"CDsR",
        Command::UskOnAir { .. } => b"CKOn",
        Command::FadeToBlack { .. } => b"FtbA",
        Command::FadeToBlackRate { .. } => b"FtbC",
        Command::AuxSource { .. } => b"CAuS",
        Command::MacroRun { .. } | Command::MacroStop | Command::MacroStopRecording => b"MAct",
        Command::MacroRecord { .. } => b"MSRc",
        Command::MediaPlayerStill { .. } => b"MPSS",
    }
}

/// A four byte body: one byte, a pad byte, then a big endian u16.
fn byte_pad_word(a: u8, word: u16) -> Vec<u8> {
    let w = word.to_be_bytes();
    vec![a, 0, w[0], w[1]]
}

fn four(a: u8, b: u8, c: u8) -> Vec<u8> {
    vec![a, b, c, 0]
}

pub(super) fn body(command: &Command, protocol: Option<Version>) -> Vec<u8> {
    match command {
        Command::Program { me, source } | Command::Preview { me, source } => {
            byte_pad_word(*me, *source)
        }
        Command::Cut { me } | Command::Auto { me } | Command::FadeToBlack { me } => four(*me, 0, 0),
        Command::TransitionPosition { me, position } => {
            byte_pad_word(*me, (*position).min(10_000))
        }
        // Mask bit 0 says only the style changes, not the next selection.
        Command::TransitionStyle { me, style } => vec![0x01, *me, *style as u8, 0],
        Command::TransitionRate { me, style, frames } => rate(*me, *style, *frames),
        Command::DskOnAir { dsk, on_air } => four(*dsk, u8::from(*on_air), 0),
        // From 2.29 a mask byte comes first; zero leaves the direction to the
        // switcher, which is a plain toggle.
        Command::DskAuto { dsk } if !matches!(protocol, Some(v) if v < DSK_AUTO_MASKED) => four(0, *dsk, 0),
        Command::DskAuto { dsk } => four(*dsk, 0, 0),
        Command::DskTie { dsk, tie } => four(*dsk, u8::from(*tie), 0),
        Command::DskRate { dsk, frames } => four(*dsk, *frames, 0),
        Command::UskOnAir { me, keyer, on_air } => four(*me, *keyer, u8::from(*on_air)),
        Command::FadeToBlackRate { me, frames } => four(0x01, *me, *frames),
        Command::AuxSource { aux, source } => {
            let s = source.to_be_bytes();
            vec![0x01, *aux, s[0], s[1]]
        }
        Command::MacroRun { index } => macro_action(*index, 0),
        Command::MacroStop => macro_action(0xffff, 1),
        Command::MacroStopRecording => macro_action(0xffff, 2),
        Command::MacroRecord { index, name, description } => {
            macro_record(*index, name, description)
        }
        // Mask bits 0 and 1: the source type and the still index.
        Command::MediaPlayerStill { player, still } => vec![0x03, *player, 1, *still, 0, 0, 0, 0],
    }
}

fn rate(me: u8, style: RateStyle, frames: u8) -> Vec<u8> {
    match style {
        RateStyle::Mix => four(me, frames, 0),
        // Mask bit 0: the rate only, the dip source stays.
        RateStyle::Dip => vec![0x01, me, frames, 0, 0, 0, 0, 0],
        // A sixteen bit mask with bit 0 for the rate, then a twenty byte body.
        RateStyle::Wipe => {
            let mut out = vec![0u8; 20];
            out[1] = 0x01;
            out[2] = me;
            out[3] = frames;
            out
        }
    }
}

fn macro_action(index: u16, action: u8) -> Vec<u8> {
    let i = index.to_be_bytes();
    vec![i[0], i[1], action, 0]
}

fn macro_record(index: u16, name: &str, description: &str) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&index.to_be_bytes());
    out.extend_from_slice(&(name.len() as u16).to_be_bytes());
    out.extend_from_slice(&(description.len() as u16).to_be_bytes());
    out.extend_from_slice(name.as_bytes());
    out.extend_from_slice(description.as_bytes());
    while out.len() % 4 != 0 {
        out.push(0);
    }
    out
}
