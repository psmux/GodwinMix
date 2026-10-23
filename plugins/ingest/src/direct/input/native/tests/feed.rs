//! Reading a whole TS file through the native demuxer, for the tests.

use gmx_udp::ts::psi::Assembler;
use gmx_udp::ts::tables::parse_pat;
use gmx_udp::ts::{header, PACKET, PAT_PID};

use super::super::program::Demux;
use super::super::timeline;
use crate::direct::input::spec::Carry;
use crate::media_tag::MediaTag;
use crate::tagger::Zero;

/// The first program's number and PMT PID, from the file's first PAT.
pub fn first_program(ts: &[u8]) -> (u16, u16) {
    programs(ts)[0]
}

/// Every program's number and PMT PID, from the file's first PAT.
pub fn programs(ts: &[u8]) -> Vec<(u16, u16)> {
    let mut a = Assembler::default();
    for p in ts.chunks_exact(PACKET) {
        let Some(h) = header(p).filter(|h| h.pid == PAT_PID) else { continue };
        let mut found = Vec::new();
        a.push(h.start, &p[h.payload_at..], |s| found = parse_pat(s));
        if !found.is_empty() {
            return found;
        }
    }
    panic!("no PAT in the file")
}

/// Every tag the native path makes of `ts`, in order.
pub fn tags(ts: &[u8], carry: Carry) -> Vec<MediaTag> {
    tags_of(ts, carry, None)
}

/// The same for `program`, or the first.
pub fn tags_of(ts: &[u8], carry: Carry, program: Option<u16>) -> Vec<MediaTag> {
    let (number, pmt) = match program {
        Some(p) => programs(ts).into_iter().find(|x| x.0 == p).expect("the program is in the file"),
        None => first_program(ts),
    };
    let mut d = Demux::new(carry);
    d.follow(number, pmt);
    let zero = Zero::default();
    let mut out = Vec::new();
    for chunk in ts.chunks(7 * PACKET) {
        d.feed(chunk, &mut out);
    }
    out.into_iter().map(|o| timeline::tag(o, &zero)).collect()
}

/// The video's tags and how many were fields, for `program` of `ts`.
pub fn pictures(ts: &[u8], program: u16) -> (u64, u64) {
    let (number, pmt) = programs(ts).into_iter().find(|x| x.0 == program).expect("the program is in the file");
    let mut d = Demux::new(Carry::default());
    d.follow(number, pmt);
    let mut out = Vec::new();
    for chunk in ts.chunks(7 * PACKET) {
        d.feed(chunk, &mut out);
    }
    d.pictures()
}
