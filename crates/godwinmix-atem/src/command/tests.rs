//! The exact bytes of every command, header included. Each expectation is
//! written out by hand from `docs/reference/atem-protocol.md`, not produced
//! by the encoder, so a change to a layout fails here first.

use super::{Command, RateStyle, TransitionStyle};
use crate::state::Version;

fn check(command: Command, expected: &[u8]) {
    assert_eq!(command.encode(), expected, "{command:?}");
}

#[test]
fn programme_and_preview() {
    check(
        Command::Program { me: 0, source: 3 },
        &[0, 12, 0, 0, b'C', b'P', b'g', b'I', 0, 0, 0, 3],
    );
    check(
        Command::Preview { me: 1, source: 1000 },
        &[0, 12, 0, 0, b'C', b'P', b'v', b'I', 1, 0, 0x03, 0xe8],
    );
}

#[test]
fn cut_auto_and_the_bar() {
    check(Command::Cut { me: 0 }, &[0, 12, 0, 0, b'D', b'C', b'u', b't', 0, 0, 0, 0]);
    check(Command::Auto { me: 1 }, &[0, 12, 0, 0, b'D', b'A', b'u', b't', 1, 0, 0, 0]);
    check(
        Command::TransitionPosition { me: 0, position: 5000 },
        &[0, 12, 0, 0, b'C', b'T', b'P', b's', 0, 0, 0x13, 0x88],
    );
    // Past the end of the bar is the end of the bar.
    check(
        Command::TransitionPosition { me: 0, position: 60000 },
        &[0, 12, 0, 0, b'C', b'T', b'P', b's', 0, 0, 0x27, 0x10],
    );
}

#[test]
fn transition_style_and_rates() {
    check(
        Command::TransitionStyle { me: 0, style: TransitionStyle::Wipe },
        &[0, 12, 0, 0, b'C', b'T', b'T', b'p', 1, 0, 2, 0],
    );
    check(
        Command::TransitionRate { me: 0, style: RateStyle::Mix, frames: 25 },
        &[0, 12, 0, 0, b'C', b'T', b'M', b'x', 0, 25, 0, 0],
    );
    check(
        Command::TransitionRate { me: 1, style: RateStyle::Dip, frames: 30 },
        &[0, 16, 0, 0, b'C', b'T', b'D', b'p', 1, 1, 30, 0, 0, 0, 0, 0],
    );
    let mut wipe = vec![0, 28, 0, 0, b'C', b'T', b'W', b'p', 0, 1, 0, 50];
    wipe.extend([0u8; 16]);
    check(Command::TransitionRate { me: 0, style: RateStyle::Wipe, frames: 50 }, &wipe);
}

#[test]
fn downstream_keyers() {
    check(Command::DskOnAir { dsk: 1, on_air: true }, &[0, 12, 0, 0, b'C', b'D', b's', b'L', 1, 1, 0, 0]);
    check(Command::DskAuto { dsk: 1 }, &[0, 12, 0, 0, b'D', b'D', b's', b'A', 0, 1, 0, 0]);
    let old = Some(Version { major: 2, minor: 28 });
    assert_eq!(
        Command::DskAuto { dsk: 1 }.encode_for(old),
        [0, 12, 0, 0, b'D', b'D', b's', b'A', 1, 0, 0, 0],
        "before 2.29 the keyer is the first byte"
    );
    check(Command::DskTie { dsk: 0, tie: true }, &[0, 12, 0, 0, b'C', b'D', b's', b'T', 0, 1, 0, 0]);
    check(Command::DskRate { dsk: 1, frames: 20 }, &[0, 12, 0, 0, b'C', b'D', b's', b'R', 1, 20, 0, 0]);
}

#[test]
fn upstream_key_and_fade_to_black() {
    check(
        Command::UskOnAir { me: 0, keyer: 2, on_air: true },
        &[0, 12, 0, 0, b'C', b'K', b'O', b'n', 0, 2, 1, 0],
    );
    check(Command::FadeToBlack { me: 0 }, &[0, 12, 0, 0, b'F', b't', b'b', b'A', 0, 0, 0, 0]);
    check(
        Command::FadeToBlackRate { me: 0, frames: 40 },
        &[0, 12, 0, 0, b'F', b't', b'b', b'C', 1, 0, 40, 0],
    );
}

#[test]
fn aux_and_media() {
    check(
        Command::AuxSource { aux: 2, source: 10010 },
        &[0, 12, 0, 0, b'C', b'A', b'u', b'S', 1, 2, 0x27, 0x1a],
    );
    check(
        Command::MediaPlayerStill { player: 1, still: 7 },
        &[0, 16, 0, 0, b'M', b'P', b'S', b'S', 3, 1, 1, 7, 0, 0, 0, 0],
    );
}

#[test]
fn macros() {
    check(Command::MacroRun { index: 4 }, &[0, 12, 0, 0, b'M', b'A', b'c', b't', 0, 4, 0, 0]);
    check(Command::MacroStop, &[0, 12, 0, 0, b'M', b'A', b'c', b't', 0xff, 0xff, 1, 0]);
    check(Command::MacroStopRecording, &[0, 12, 0, 0, b'M', b'A', b'c', b't', 0xff, 0xff, 2, 0]);
    check(
        Command::MacroRecord { index: 2, name: "Open".into(), description: "x".into() },
        &[
            0, 20, 0, 0, b'M', b'S', b'R', b'c', 0, 2, 0, 4, 0, 1, b'O', b'p', b'e', b'n', b'x', 0,
        ],
    );
}
