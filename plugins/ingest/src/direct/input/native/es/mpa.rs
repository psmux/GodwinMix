//! MPEG audio (ISO 11172-3 and 13818-3) to the bodies `mpegaudioparse` and
//! `frames::mpeg_sink` make: one frame per tag, MP3 under classic FLV's id
//! and layers I and II behind the enhanced RTMP `.mp3` prefix.

use super::frames::{Framer, Split};
use super::rate::samples_ns;
use super::{Parser, Pes, Said, Unit};
use crate::exaudio;

/// Bit rates in kbit/s by index: MPEG-1 layers I, II, III, then MPEG-2
/// layer I, and MPEG-2 layers II and III.
const KBPS: [[u16; 15]; 5] = [
    [0, 32, 64, 96, 128, 160, 192, 224, 256, 288, 320, 352, 384, 416, 448],
    [0, 32, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 384],
    [0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320],
    [0, 32, 48, 56, 64, 80, 96, 112, 128, 144, 160, 176, 192, 224, 256],
    [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160],
];

/// Classic FLV's MP3 flags, as `frames::mpeg_sink` writes them.
const FLV_MP3: u8 = 0x2F;

#[derive(Default)]
pub struct Mpa {
    framer: Framer,
    said: Said,
}

/// `(frame length in bytes, samples, layer)` for the frame at `f`.
fn size(f: &[u8]) -> Option<(usize, u32, u8)> {
    let read = exaudio::mpeg_audio(f)?;
    let mpeg1 = (f[1] >> 3) & 3 == 3;
    let layer = 4 - ((f[1] >> 1) & 3);
    let table = match (mpeg1, layer) {
        (true, l) => usize::from(l - 1),
        (false, 1) => 3,
        (false, _) => 4,
    };
    let kbps = u32::from(*KBPS[table].get(usize::from(f[2] >> 4))?);
    let (rate, pad) = (read.sample_rate, u32::from((f[2] >> 1) & 1));
    if kbps == 0 || rate == 0 {
        return None;
    }
    let (len, samples) = match layer {
        1 => ((12 * kbps * 1000 / rate + pad) * 4, 384),
        2 => (144 * kbps * 1000 / rate + pad, 1152),
        _ if mpeg1 => (144 * kbps * 1000 / rate + pad, 1152),
        _ => (72 * kbps * 1000 / rate + pad, 576),
    };
    Some((len as usize, samples, layer))
}

impl Split for Mpa {
    fn frame_len(&self, f: &[u8]) -> Option<usize> {
        size(f).map(|s| s.0)
    }
}

impl Parser for Mpa {
    fn pes(&mut self, pes: &Pes, out: &mut Vec<Unit>) {
        let mut frames = Vec::new();
        let mut framer = std::mem::take(&mut self.framer);
        framer.push(self, pes, &mut frames);
        self.framer = framer;
        for (stamped, f) in frames {
            let (Some((_, samples, layer)), Some(read)) = (size(&f), exaudio::mpeg_audio(&f)) else { continue };
            let Some(at) = self.framer.clock.frame(stamped, samples_ns(samples, read.sample_rate)) else { continue };
            self.said = Said { codec: read.codec, channels: read.channels, sample_rate: read.sample_rate };
            let mut body = if layer == 3 { vec![FLV_MP3] } else { exaudio::prefix(exaudio::MPEG).to_vec() };
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
    fn a_layer_two_frame_at_48k_and_192k_is_576_bytes() {
        // MPEG-1 layer II, 192 kbit/s (index 10), 48 kHz, no padding.
        assert_eq!(size(&[0xFF, 0xFD, 0xA4, 0x00]), Some((576, 1152, 2)));
        // MPEG-1 layer III, 128 kbit/s, 44.1 kHz, padded: 418 bytes.
        assert_eq!(size(&[0xFF, 0xFB, 0x92, 0x00]), Some((418, 1152, 3)));
    }
}
