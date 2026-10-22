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
    let mut a = Assembler::default();
    for p in ts.chunks_exact(PACKET) {
        let Some(h) = header(p).filter(|h| h.pid == PAT_PID) else { continue };
        let mut found = None;
        a.push(h.start, &p[h.payload_at..], |s| found = parse_pat(s).first().copied());
        if let Some(f) = found {
            return f;
        }
    }
    panic!("no PAT in the file")
}

/// Every tag the native path makes of `ts`, in order.
pub fn tags(ts: &[u8], carry: Carry) -> Vec<MediaTag> {
    let (number, pmt) = first_program(ts);
    let mut d = Demux::new(carry);
    d.follow(number, pmt);
    let zero = Zero::default();
    let mut out = Vec::new();
    for chunk in ts.chunks(7 * PACKET) {
        d.feed(chunk, &mut out);
    }
    out.into_iter().map(|o| timeline::tag(o, &zero)).collect()
}
