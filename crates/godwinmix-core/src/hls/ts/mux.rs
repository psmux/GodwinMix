//! The muxer: one program of H.264 or HEVC and AAC, from [`Unit`]s, into
//! 188 byte packets. A copy of the ingest plugin's `tsmux` cut down to what
//! HLS carries, fed with frames already taken out of their FLV bodies.
//!
//! PAT and PMT go before every keyframe, at least every 400 ms, and when a
//! caller starts a segment of sound alone ([`Muxer::tables`]). The clock
//! rides on the video PID, 0.7 s behind the decode time, or on the sound's
//! when there is no picture. Continuity counters carry on for the muxer's
//! life, so two segments back to back are one clean stream.

use super::clock::Forward;
use super::es::{AudioConfig, VideoConfig};
use super::packet::{self, Counter, First};
use super::psi;
use bytes::Bytes;

const BASE: u64 = 126_000;
const PCR_LEAD: u64 = 63_000;
const PSI_MS: u32 = 400;
const WRAP: u64 = 1 << 33;
const AAC: u8 = 0x0f;

/// What one unit is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitKind {
    /// An `avcC` record.
    AvcConfig,
    /// An `hvcC` record.
    HevcConfig,
    /// An AudioSpecificConfig.
    AacConfig,
    /// One access unit, NAL units with length prefixes.
    Video,
    /// One raw AAC frame.
    Audio,
}

/// One frame or configuration, as the packager reads it off the hub.
#[derive(Debug, Clone)]
pub struct Unit {
    pub kind: UnitKind,
    /// Decode time in ms on the input's own timeline.
    pub ms: u32,
    pub cts_ms: i32,
    pub key: bool,
    pub data: Bytes,
}

impl Unit {
    pub fn is_config(&self) -> bool {
        matches!(self.kind, UnitKind::AvcConfig | UnitKind::HevcConfig | UnitKind::AacConfig)
    }
}

#[derive(Default)]
pub struct Muxer {
    video: Option<VideoConfig>,
    audio: Option<AudioConfig>,
    cc: [Counter; 4],
    psi_at: Option<u32>,
    announced: (Option<u8>, Option<u8>),
    version: u8,
    clocks: [Forward; 2],
}

impl Muxer {
    /// Take a configuration, or append the packets for one frame.
    pub fn unit(&mut self, u: &Unit, out: &mut Vec<u8>) {
        match u.kind {
            UnitKind::AvcConfig => self.video = VideoConfig::from_avcc(&u.data).or(self.video.take()),
            UnitKind::HevcConfig => self.video = VideoConfig::from_hvcc(&u.data).or(self.video.take()),
            UnitKind::AacConfig => self.audio = AudioConfig::from_asc(&u.data).or(self.audio),
            UnitKind::Video => self.video_frame(u, out),
            UnitKind::Audio => self.audio_frame(u, out),
        }
    }

    /// PAT and PMT now, whatever is due: the start of a segment of sound.
    pub fn tables(&mut self, ms: u32, out: &mut Vec<u8>) {
        let video = self.video.as_ref().map(VideoConfig::stream_type);
        let audio = self.audio.map(|_| AAC);
        if self.psi_at.is_some() && self.announced != (video, audio) {
            self.version = (self.version + 1) & 0x1f;
        }
        self.announced = (video, audio);
        packet::write(out, 0, &mut self.cc[0], First::default(), true, &psi::pat());
        packet::write(out, psi::PMT_PID, &mut self.cc[1], First::default(), true, &psi::pmt(self.version, video, audio));
        self.psi_at = Some(ms);
    }

    fn due(&self, ms: u32) -> bool {
        self.psi_at.is_none_or(|at| ms.wrapping_sub(at) >= PSI_MS)
    }

    fn video_frame(&mut self, u: &Unit, out: &mut Vec<u8>) {
        let Some(config) = self.video.as_ref() else { return };
        let (au, key) = config.annex_b(&u.data, u.key);
        if key || self.due(u.ms) {
            self.tables(u.ms, out);
        }
        let dts = self.clocks[0].next((u64::from(u.ms) * 90 + BASE) % WRAP);
        let pts = (dts as i64 + i64::from(u.cts_ms.max(0)) * 90).rem_euclid(WRAP as i64) as u64;
        let mut pes = packet::pes(0xe0, pts, (pts != dts).then_some(dts), au.len());
        pes.extend_from_slice(&au);
        let first = First { pcr: Some((dts + WRAP - PCR_LEAD) % WRAP), random_access: key };
        packet::write(out, psi::VIDEO_PID, &mut self.cc[2], first, false, &pes);
    }

    fn audio_frame(&mut self, u: &Unit, out: &mut Vec<u8>) {
        let Some(config) = self.audio else { return };
        let alone = self.video.is_none();
        if alone && self.due(u.ms) {
            self.tables(u.ms, out);
        } else if self.psi_at.is_none() {
            // Nothing before the first picture: a receiver starts there.
            return;
        }
        let pts = self.clocks[1].next((u64::from(u.ms) * 90 + BASE) % WRAP);
        let mut pes = packet::pes(0xc0, pts, None, u.data.len() + 7);
        pes.extend_from_slice(&config.adts(u.data.len()));
        pes.extend_from_slice(&u.data);
        let first = First { pcr: alone.then_some((pts + WRAP - PCR_LEAD) % WRAP), random_access: alone };
        packet::write(out, psi::AUDIO_PID, &mut self.cc[3], first, false, &pes);
    }
}
