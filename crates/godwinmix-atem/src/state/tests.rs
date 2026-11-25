//! Each block layout read from bytes written out by hand.

use super::*;
use crate::command::TransitionStyle;
use crate::wire::{block, Blocks};

fn apply(state: &mut State, name: &[u8; 4], body: &[u8]) -> bool {
    let bytes = block(name, body);
    let parsed = Blocks::new(&bytes).next().unwrap();
    state.apply(parsed)
}

fn padded(text: &str, width: usize) -> Vec<u8> {
    let mut out = text.as_bytes().to_vec();
    out.resize(width, 0);
    out
}

#[test]
fn product_version_and_topology() {
    let mut s = State::default();
    apply(&mut s, b"_ver", &[0, 2, 0, 30]);
    let mut pin = padded("ATEM Mini Pro", 40);
    pin.extend([14, 0, 0, 0]);
    apply(&mut s, b"_pin", &pin);
    apply(&mut s, b"_top", &[1, 14, 1, 1, 0, 1, 0, 0]);
    assert_eq!(s.protocol, Some(Version { major: 2, minor: 30 }));
    assert_eq!(s.product.as_deref(), Some("ATEM Mini Pro"));
    assert_eq!(s.model, Some(14));
    assert_eq!(s.topology.mix_effects, 1);
    assert_eq!(s.topology.downstream_keyers, 1);
    assert_eq!(s.topology.media_players, 1);
}

#[test]
fn input_names() {
    let mut s = State::default();
    let mut body = vec![0, 3];
    body.extend(padded("Pulpit", 20));
    body.extend(padded("PULP", 4));
    body.extend([0; 10]);
    apply(&mut s, b"InPr", &body);
    assert_eq!(s.inputs[&3].long_name, "Pulpit");
    assert_eq!(s.inputs[&3].short_name, "PULP");
    assert_eq!(s.input_name(3), "Pulpit");
    assert_eq!(s.input_name(1000), "Bars");
}

#[test]
fn programme_preview_and_transition() {
    let mut s = State::default();
    apply(&mut s, b"PrgI", &[1, 0, 0, 4]);
    apply(&mut s, b"PrvI", &[1, 0, 0x03, 0xe8, 0, 0, 0, 0]);
    apply(&mut s, b"TrSS", &[1, 1, 1, 2, 3, 0, 0, 0]);
    apply(&mut s, b"TrPs", &[1, 1, 12, 0, 0x13, 0x88, 0, 0]);
    apply(&mut s, b"TMxP", &[1, 25, 0, 0]);
    apply(&mut s, b"TDpP", &[1, 30, 0, 0]);
    apply(&mut s, b"TWpP", &[1, 40, 0, 0]);
    assert_eq!(s.program(1), Some(4));
    assert_eq!(s.preview(1), Some(1000));
    assert_eq!(s.program(0), Some(0), "M/E 1 exists once M/E 2 is reported");
    let t = &s.mix_effects[1].transition;
    assert_eq!(t.style, Some(TransitionStyle::Dip));
    assert_eq!(t.next_style, Some(TransitionStyle::Wipe));
    assert_eq!(t.next_selection, 3);
    assert!(t.in_transition);
    assert_eq!((t.frames_remaining, t.position), (12, 5000));
    assert_eq!((t.mix_rate, t.dip_rate, t.wipe_rate), (25, 30, 40));
}

#[test]
fn keyers_and_fade_to_black() {
    let mut s = State::default();
    apply(&mut s, b"_MeC", &[0, 4, 0, 0]);
    apply(&mut s, b"KeOn", &[0, 2, 1, 0]);
    apply(&mut s, b"FtbS", &[0, 1, 0, 0]);
    apply(&mut s, b"FtbP", &[0, 50, 0, 0]);
    apply(&mut s, b"DskS", &[1, 1, 1, 1, 9, 0, 0, 0]);
    apply(&mut s, b"DskP", &[1, 1, 20, 0, 0, 0, 0, 0]);
    apply(&mut s, b"DskB", &[1, 0, 0x0b, 0xc2, 0x0b, 0xc3, 0, 0]);
    let me = &s.mix_effects[0];
    assert_eq!(me.keyers.len(), 4);
    assert!(me.keyers[2].on_air && !me.keyers[0].on_air);
    assert!(me.fade_to_black.fully_black);
    assert_eq!(me.fade_to_black.rate, 50);
    let dsk = s.downstream_keyers[1];
    assert!(dsk.on_air && dsk.in_transition && dsk.is_auto_transitioning && dsk.tie);
    assert_eq!((dsk.frames_remaining, dsk.rate), (9, 20));
    assert_eq!((dsk.fill_source, dsk.key_source), (3010, 3011));
}

#[test]
fn aux_tally_macros_and_media() {
    let mut s = State::default();
    apply(&mut s, b"AuxS", &[2, 0, 0x27, 0x1a]);
    apply(&mut s, b"TlSr", &[0, 3, 0, 1, 1, 0, 2, 2, 0, 3, 0, 0, 0, 0]);
    let mut mprp = vec![0, 5, 1, 0, 0, 5, 0, 2];
    mprp.extend(b"Intro");
    mprp.extend(b"go");
    apply(&mut s, b"MPrp", &mprp);
    apply(&mut s, b"MRPr", &[1, 0, 0, 5]);
    apply(&mut s, b"MPCE", &[0, 1, 7, 0]);
    assert_eq!(s.aux, [0, 0, 10010]);
    assert!(s.tally[&1].program && !s.tally[&1].preview);
    assert!(s.tally[&2].preview);
    assert_eq!(s.tally[&3], Tally::default());
    assert_eq!(s.macros[&5].name, "Intro");
    assert_eq!(s.macros[&5].description, "go");
    assert_eq!(s.macro_run.index, Some(5));
    assert_eq!(s.media_players[0].still, 7);
    // An unused slot takes the macro away.
    apply(&mut s, b"MPrp", &[0, 5, 0, 0, 0, 0, 0, 0]);
    assert!(s.macros.is_empty());
}

#[test]
fn short_and_unknown_blocks_change_nothing() {
    let mut s = State::default();
    assert!(!apply(&mut s, b"PrgI", &[0, 0]));
    assert!(!apply(&mut s, b"Zzzz", &[1, 2, 3, 4]));
    assert!(!apply(&mut s, b"TlSr", &[0, 9, 0, 1, 1]));
    assert_eq!(s, State::default());
    assert!(apply(&mut s, b"InCm", &[1, 0, 0, 0]));
    assert!(s.initialised);
}

#[test]
fn random_bytes_never_panic() {
    let mut seed = 0x2545_f491_u32;
    let mut s = State::default();
    for _ in 0..20_000 {
        let mut payload = Vec::new();
        for _ in 0..(seed % 64) {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            payload.push(seed as u8);
        }
        let names: [&[u8; 4]; 6] = [b"InPr", b"TlSr", b"MPrp", b"_pin", b"KeOn", b"DskB"];
        let name = names[(seed % 6) as usize];
        apply(&mut s, name, &payload);
    }
}
