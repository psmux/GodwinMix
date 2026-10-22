//! One elementary stream of the followed program: its continuity counter,
//! its PES assembly and its parser.

use gmx_udp::ts::Header;

use super::clock::Clock;
use super::es::{Codec, Parser, Pes, Unit};
use super::pes::{self, Assembly};
use super::Counts;
use crate::media_tag::TagKind;

/// A unit on its way to becoming a tag, with the track it belongs to.
#[derive(Debug)]
pub struct Out {
    pub track: u8,
    pub kind: TagKind,
    pub unit: Unit,
}

pub struct Es {
    pub pid: u16,
    pub stream_type: u8,
    pub codec: Codec,
    pub track: u8,
    pub parser: Box<dyn Parser>,
    asm: Assembly,
    /// The last continuity counter, 16 before the first.
    cc: u8,
}

impl Es {
    pub fn new(pid: u16, stream_type: u8, codec: Codec, track: u8) -> Es {
        Es { pid, stream_type, codec, track, parser: codec.parser(), asm: Assembly::default(), cc: 16 }
    }

    /// The sender restarted: its counters start wherever they like.
    pub fn resumed(&mut self) {
        self.cc = 16;
        self.asm.lose();
    }

    /// One TS packet of this stream.
    pub fn packet(&mut self, h: &Header, p: &[u8], clock: &mut Clock, no_pcr: bool, n: &mut Counts, out: &mut Vec<Out>) {
        if !h.has_payload {
            return;
        }
        match check(&mut self.cc, h) {
            Check::Repeat => return,
            Check::Lost(missing) => {
                n.cc_errors += 1;
                n.ts_lost += u64::from(missing);
                self.asm.lose();
            }
            Check::Ok => {}
        }
        if p[3] >> 6 != 0 {
            n.scrambled += 1;
            self.asm.lose();
            return;
        }
        let (track, kind) = (self.track, if self.codec.video() { TagKind::Video } else { TagKind::Audio });
        let parser = &mut self.parser;
        let mut units = Vec::new();
        self.asm.push(h.start, &p[h.payload_at..], &mut |whole| {
            let Some(head) = pes::header(whole) else { return };
            if no_pcr {
                if let Some(first) = head.dts.or(head.pts) {
                    clock.no_pcr(first);
                }
            }
            let (dts, pts) = (head.dts.and_then(|t| clock.time(t)), head.pts.and_then(|t| clock.time(t)));
            if head.pts.is_some() && pts.is_none() {
                return;
            }
            parser.pes(&Pes { pts, dts, data: &whole[head.data_at..] }, &mut units);
        });
        out.extend(units.into_iter().map(|unit| Out { track, kind, unit }));
    }
}

pub enum Check {
    Ok,
    /// The same packet again, which TS allows once and which is skipped.
    Repeat,
    /// This many packets never came.
    Lost(u8),
}

/// The continuity counter of a packet with a payload against the last one.
pub fn check(last: &mut u8, h: &Header) -> Check {
    let before = std::mem::replace(last, h.cc);
    if before == 16 || h.discontinuity {
        return Check::Ok;
    }
    match h.cc.wrapping_sub(before) & 0x0F {
        1 => Check::Ok,
        0 => Check::Repeat,
        gap => Check::Lost(gap - 1),
    }
}
