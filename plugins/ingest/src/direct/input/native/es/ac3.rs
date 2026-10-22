//! AC-3 and E-AC-3 to the bodies `ac3parse` and `frames::ac3_sink` make:
//! one syncframe per tag behind the five byte enhanced RTMP prefix, with no
//! sequence header (each frame says what it is).
//!
//! The frame sizes are ATSC A/52's: table 5.18 for AC-3 by `fscod` and
//! `frmsizecod`, and `frmsiz` in the E-AC-3 header (annex E).

use super::frames::{Framer, Split};
use super::rate::samples_ns;
use super::{Parser, Pes, Said, Unit};
use crate::exaudio;

/// AC-3 bit rates in kbit/s by `frmsizecod / 2`.
const KBPS: [u32; 19] = [32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 384, 448, 512, 576, 640];

#[derive(Default)]
pub struct Ac3 {
    framer: Framer,
    said: Said,
}

/// `(frame length in bytes, samples it lasts, E-AC-3)` for the syncframe
/// at the start of `f`.
fn size(f: &[u8]) -> Option<(usize, u32, bool)> {
    if f.len() < 6 || f[0] != 0x0B || f[1] != 0x77 {
        return None;
    }
    let bsid = f[5] >> 3;
    if bsid > 10 && bsid <= 16 {
        let words = ((usize::from(f[2] & 7) << 8) | usize::from(f[3])) + 1;
        // A dependent substream belongs to the frame before it, in time.
        let dependent = f[2] >> 6 == 1;
        let blocks = if f[4] >> 6 == 3 { 6 } else { [1, 2, 3, 6][usize::from((f[4] >> 4) & 3)] };
        return Some((words * 2, if dependent { 0 } else { blocks * 256 }, true));
    }
    if bsid > 10 {
        return None;
    }
    let (fscod, code) = (f[4] >> 6, usize::from(f[4] & 0x3F));
    let kbps = *KBPS.get(code / 2)?;
    let words = match fscod {
        0 => kbps * 2,
        1 => kbps * 2 * 48_000 / 44_100 + (code as u32 & 1),
        2 => kbps * 3,
        _ => return None,
    };
    Some((words as usize * 2, 1536, false))
}

impl Split for Ac3 {
    fn frame_len(&self, f: &[u8]) -> Option<usize> {
        size(f).map(|s| s.0)
    }
}

impl Parser for Ac3 {
    fn pes(&mut self, pes: &Pes, out: &mut Vec<Unit>) {
        let mut frames = Vec::new();
        let mut framer = std::mem::take(&mut self.framer);
        framer.push(self, pes, &mut frames);
        self.framer = framer;
        for (stamped, f) in frames {
            let (Some((_, samples, eac3)), Some(read)) = (size(&f), exaudio::ac3(&f)) else { continue };
            let Some(at) = self.framer.clock.frame(stamped, samples_ns(samples, read.sample_rate)) else { continue };
            self.said = Said { codec: read.codec, channels: read.channels, sample_rate: read.sample_rate };
            let mut body = exaudio::prefix(if eac3 { exaudio::EAC3 } else { exaudio::AC3 }).to_vec();
            body.extend_from_slice(&f);
            out.push(Unit { at, body, keyframe: false, header: false });
        }
    }

    fn said(&self) -> Said {
        self.said.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_sizes_follow_the_table() {
        // 448 kbit/s at 48 kHz is code 30: 896 words.
        assert_eq!(size(&[0x0B, 0x77, 0, 0, 30, 0x40]), Some((1792, 1536, false)));
        // 32 kbit/s at 44.1 kHz, odd code: 70 words.
        assert_eq!(size(&[0x0B, 0x77, 0, 0, 0x41, 0x40]), Some((140, 1536, false)));
        // E-AC-3, frmsiz 0x2FF, six blocks.
        assert_eq!(size(&[0x0B, 0x77, 0x02, 0xFF, 0x3F, 0x80]), Some((1536, 1536, true)));
    }
}
