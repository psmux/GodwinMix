//! An MPEG-TS muxer for the hub's tags: one program, H.264 or HEVC video,
//! AAC sound, remuxed and never decoded.
//!
//! ```text
//!   MediaTag ──► es (Annex B, ADTS) ──► packet::pes ──► packet::write ──► 188 byte packets
//!                                     PAT and PMT before every keyframe, and at least every 400 ms
//! ```
//!
//! What UDP, multicast, RIST and recording to a file send. It is a few
//! hundred lines rather than GStreamer's `mpegtsmux` because an output is
//! then one thread and a socket, with no pipeline, no parser and no
//! aggregator thread per output, which is what lets one process carry
//! hundreds of copies. The clock rides on the video PID, 0.7 s behind the
//! decode time, as ffmpeg's muxer puts it.

mod es;
mod packet;
mod psi;

use crate::media_tag::{MediaTag, TagKind};
use packet::{Counter, First};
pub use packet::PACKET;

/// Where stamps start, in 90 kHz ticks, so that a PCR behind them is never
/// negative.
const BASE: u64 = 126_000;
/// How far the clock runs behind the decode time.
const PCR_LEAD: u64 = 63_000;
/// The longest the tables go unrepeated.
const PSI_MS: u32 = 400;
const WRAP: u64 = 1 << 33;

#[derive(Default)]
pub struct Muxer {
    video: Option<es::VideoConfig>,
    audio: Option<es::AudioConfig>,
    /// Continuity counters: PAT, PMT, video, audio.
    cc: [Counter; 4],
    /// When the tables last went out, and whether they have changed since.
    psi_at: Option<u32>,
    psi_dirty: bool,
}

impl Muxer {
    pub fn new() -> Muxer {
        Muxer::default()
    }

    /// Append the packets for one tag, stamped `ms` on the output's own
    /// timeline. A frame before its codec's sequence header is dropped.
    pub fn tag(&mut self, tag: &MediaTag, ms: u32, out: &mut Vec<u8>) {
        match tag.kind {
            TagKind::Script => {}
            TagKind::Video if tag.sequence_header => self.video_header(&tag.payload),
            TagKind::Audio if tag.sequence_header => self.audio_header(&tag.payload),
            TagKind::Video => self.video_frame(tag, ms, out),
            TagKind::Audio => self.audio_frame(tag, ms, out),
        }
    }

    fn video_header(&mut self, body: &[u8]) {
        let record = body.get(5..).unwrap_or(&[]);
        let config = match crate::eflv::fourcc(body) {
            None if body[0] & 0x0f == 7 => es::VideoConfig::from_avcc(record),
            Some(cc) if &cc == crate::eflv::HEVC => es::VideoConfig::from_hvcc(record),
            _ => None,
        };
        self.psi_dirty |= config.as_ref().map(|c| c.hevc) != self.video.as_ref().map(|c| c.hevc);
        if config.is_some() {
            self.video = config;
        }
    }

    fn audio_header(&mut self, body: &[u8]) {
        if body.first().is_some_and(|b| b >> 4 == 10) {
            let config = body.get(2..).and_then(es::AudioConfig::from_asc);
            self.psi_dirty |= self.audio.is_none() && config.is_some();
            self.audio = config.or(self.audio);
        }
    }

    fn tables(&mut self, ms: u32, force: bool, out: &mut Vec<u8>) {
        let due = self.psi_at.is_none_or(|at| ms.wrapping_sub(at) >= PSI_MS);
        if !(force || due || self.psi_dirty) {
            return;
        }
        let video = self.video.as_ref().map(es::VideoConfig::stream_type);
        let audio = self.audio.map(|_| 0x0f);
        packet::write(out, 0, &mut self.cc[0], First::default(), true, &psi::pat());
        packet::write(out, psi::PMT_PID, &mut self.cc[1], First::default(), true, &psi::pmt(video, audio));
        self.psi_at = Some(ms);
        self.psi_dirty = false;
    }

    fn video_frame(&mut self, tag: &MediaTag, ms: u32, out: &mut Vec<u8>) {
        let Some(config) = self.video.as_ref() else { return };
        let Some((skip, cts)) = crate::eflv::frame(&tag.payload) else { return };
        let au = config.annex_b(tag.payload.get(skip..).unwrap_or(&[]), tag.keyframe);
        self.tables(ms, tag.keyframe, out);
        let dts = (u64::from(ms) * 90 + BASE) % WRAP;
        let pts = (dts as i64 + i64::from(cts) * 90).rem_euclid(WRAP as i64) as u64;
        let mut pes = packet::pes(0xe0, pts, (pts != dts).then_some(dts), au.len());
        pes.extend_from_slice(&au);
        let first = First { pcr: Some((dts + WRAP - PCR_LEAD) % WRAP), random_access: tag.keyframe };
        packet::write(out, psi::VIDEO_PID, &mut self.cc[2], first, false, &pes);
    }

    fn audio_frame(&mut self, tag: &MediaTag, ms: u32, out: &mut Vec<u8>) {
        let Some(config) = self.audio else { return };
        let Some(frame) = tag.payload.get(2..) else { return };
        let alone = self.video.is_none();
        if alone {
            self.tables(ms, false, out);
        } else if self.psi_at.is_none() {
            // Nothing before the first picture: a receiver starts there.
            return;
        }
        let pts = (u64::from(ms) * 90 + BASE) % WRAP;
        let mut pes = packet::pes(0xc0, pts, None, frame.len() + 7);
        pes.extend_from_slice(&config.adts(frame.len()));
        pes.extend_from_slice(frame);
        let first = First { pcr: alone.then_some((pts + WRAP - PCR_LEAD) % WRAP), random_access: alone };
        packet::write(out, psi::AUDIO_PID, &mut self.cc[3], first, false, &pes);
    }
}

#[cfg(test)]
mod tests;
