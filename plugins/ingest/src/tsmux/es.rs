//! FLV's framing turned into MPEG-TS's, byte for byte, nothing decoded.
//!
//! FLV carries H.264 and HEVC as length prefixed NAL units with the
//! parameter sets in a configuration record, and AAC as raw frames with an
//! AudioSpecificConfig. MPEG-TS wants Annex B (start codes, parameter sets
//! in line before each keyframe, an access unit delimiter first) and ADTS
//! (a seven byte header on every AAC frame).

/// A video stream's configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct VideoConfig {
    pub hevc: bool,
    /// Bytes in each NAL unit's length prefix.
    pub length_size: usize,
    /// The parameter sets, each with its start code, in order.
    pub parameter_sets: Vec<u8>,
}

impl VideoConfig {
    /// The PMT's stream type.
    pub fn stream_type(&self) -> u8 {
        if self.hevc { 0x24 } else { 0x1b }
    }

    /// Read an `avcC` record.
    pub fn from_avcc(r: &[u8]) -> Option<VideoConfig> {
        let length_size = usize::from(*r.get(4)? & 3) + 1;
        let mut sets = Vec::new();
        let mut at = 5;
        for _ in 0..2 {
            // SPS then PPS: a count (five bits for SPS), then length and bytes.
            let count = *r.get(at)? & if sets.is_empty() { 0x1f } else { 0xff };
            at += 1;
            for _ in 0..count {
                at = take_set(r, at, &mut sets)?;
            }
        }
        Some(VideoConfig { hevc: false, length_size, parameter_sets: sets })
    }

    /// Read an `hvcC` record: arrays of VPS, SPS and PPS.
    pub fn from_hvcc(r: &[u8]) -> Option<VideoConfig> {
        let length_size = usize::from(*r.get(21)? & 3) + 1;
        let arrays = *r.get(22)?;
        let (mut at, mut sets) = (23, Vec::new());
        for _ in 0..arrays {
            let count = u16::from_be_bytes([*r.get(at + 1)?, *r.get(at + 2)?]);
            at += 3;
            for _ in 0..count {
                at = take_set(r, at, &mut sets)?;
            }
        }
        Some(VideoConfig { hevc: true, length_size, parameter_sets: sets })
    }

    /// One access unit in Annex B: a delimiter, the parameter sets when it
    /// is a keyframe, and its NAL units each behind a start code.
    pub fn annex_b(&self, frame: &[u8], keyframe: bool) -> Vec<u8> {
        let mut out = Vec::with_capacity(frame.len() + 16 + if keyframe { self.parameter_sets.len() } else { 0 });
        out.extend_from_slice(if self.hevc { &[0, 0, 0, 1, 0x46, 0x01, 0x50] } else { &[0, 0, 0, 1, 0x09, 0xf0] });
        if keyframe {
            out.extend_from_slice(&self.parameter_sets);
        }
        let mut at = 0;
        while at + self.length_size <= frame.len() {
            let len = frame[at..at + self.length_size].iter().fold(0usize, |n, b| (n << 8) | usize::from(*b));
            at += self.length_size;
            let Some(nal) = frame.get(at..at + len) else { break };
            at += len;
            if nal.is_empty() || self.is_delimiter(nal[0]) {
                continue;
            }
            out.extend_from_slice(&[0, 0, 0, 1]);
            out.extend_from_slice(nal);
        }
        out
    }

    fn is_delimiter(&self, first: u8) -> bool {
        if self.hevc { (first >> 1) & 0x3f == 35 } else { first & 0x1f == 9 }
    }
}

fn take_set(r: &[u8], at: usize, sets: &mut Vec<u8>) -> Option<usize> {
    let len = usize::from(u16::from_be_bytes([*r.get(at)?, *r.get(at + 1)?]));
    let set = r.get(at + 2..at + 2 + len)?;
    sets.extend_from_slice(&[0, 0, 0, 1]);
    sets.extend_from_slice(set);
    Some(at + 2 + len)
}

/// An AAC stream's configuration, out of its AudioSpecificConfig.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioConfig {
    /// The object type less one, which is what ADTS calls the profile.
    profile: u8,
    rate_index: u8,
    channels: u8,
}

impl AudioConfig {
    pub fn from_asc(asc: &[u8]) -> Option<AudioConfig> {
        let (a, b) = (*asc.first()?, *asc.get(1)?);
        let object = a >> 3;
        Some(AudioConfig {
            profile: object.saturating_sub(1).min(3),
            rate_index: ((a & 7) << 1) | (b >> 7),
            channels: (b >> 3) & 0x0f,
        })
    }

    #[cfg(test)]
    pub fn sample_rate(&self) -> u32 {
        const RATES: [u32; 13] = [96_000, 88_200, 64_000, 48_000, 44_100, 32_000, 24_000, 22_050, 16_000, 12_000, 11_025, 8_000, 7_350];
        RATES.get(usize::from(self.rate_index)).copied().unwrap_or(48_000)
    }

    /// The seven byte ADTS header for a frame of `len` bytes.
    pub fn adts(&self, len: usize) -> [u8; 7] {
        let full = len + 7;
        [
            0xff,
            0xf1,
            (self.profile << 6) | (self.rate_index << 2) | ((self.channels >> 2) & 1),
            ((self.channels & 3) << 6) | ((full >> 11) as u8 & 3),
            (full >> 3) as u8,
            (((full & 7) as u8) << 5) | 0x1f,
            0xfc,
        ]
    }
}

#[cfg(test)]
#[path = "es_tests.rs"]
mod tests;
