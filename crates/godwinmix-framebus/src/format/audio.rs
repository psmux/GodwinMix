//! Sound on the bus: interleaved samples, a chunk per slot.
//!
//! A picture is one frame per slot and a reader wants only the newest. Sound
//! is a stream a reader wants all of, in order, so an audio region differs in
//! two ways that the ring checks the format for: the owner overwrites the
//! oldest free chunk rather than the lowest numbered one, and a reader takes
//! the next chunk after the one it had rather than the newest. A reader that
//! falls far behind still skips ahead, because old sound is worth as little
//! as an old picture.
//!
//! The layout reuses the picture's fields: `width` is the channel count,
//! `height` the sample rate, `strides[0]` the bytes of one sample for every
//! channel, and `size` the largest chunk a slot holds. Each slot says how many
//! of those bytes it filled.

use super::{Format, Layout};
use crate::Error;

/// The longest chunk one slot holds. Longer buffers are cut into several.
pub const MAX_CHUNK_MS: u32 = 100;

impl Format {
    /// The sound formats, as GStreamer names them. Interleaved only.
    pub const AUDIO: [Format; 2] = [Format::F32, Format::S16];

    pub fn is_audio(self) -> bool {
        matches!(self, Format::F32 | Format::S16)
    }

    /// Bytes in one sample of one channel.
    pub fn sample_bytes(self) -> u32 {
        match self {
            Format::F32 => 4,
            Format::S16 => 2,
            _ => 0,
        }
    }
}

impl Layout {
    /// Chunks of up to [`MAX_CHUNK_MS`] of `channels` interleaved at `rate`.
    pub fn audio(format: Format, rate: u32, channels: u32) -> Result<Layout, Error> {
        if !format.is_audio() || !(1..=64).contains(&channels) || !(1..=768_000).contains(&rate) {
            return Err(Error::BadLayout(format!(
                "{} at {rate} Hz with {channels} channels is not sound the bus carries. \
                 Put an audioconvert before it, to F32LE interleaved",
                format.name()
            )));
        }
        let frame = format.sample_bytes() * channels;
        let per_chunk = (rate * MAX_CHUNK_MS).div_ceil(1000);
        let mut offsets = [0; 4];
        offsets[0] = 0;
        Ok(Layout {
            format,
            width: channels,
            height: rate,
            n_planes: 1,
            offsets,
            strides: [frame, 0, 0, 0],
            size: u64::from(per_chunk * frame),
            fps_n: 0,
            fps_d: 1,
        })
    }

    pub fn is_audio(&self) -> bool {
        self.format.is_audio()
    }

    pub fn channels(&self) -> u32 {
        self.width
    }

    pub fn rate(&self) -> u32 {
        self.height
    }

    /// Bytes of one sample for every channel.
    pub fn frame_bytes(&self) -> u32 {
        self.strides[0]
    }

    /// Nanoseconds that `bytes` of this sound last.
    pub fn duration_ns(&self, bytes: usize) -> u64 {
        let frames = bytes as u64 / u64::from(self.frame_bytes().max(1));
        frames * 1_000_000_000 / u64::from(self.rate().max(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chunk_is_a_tenth_of_a_second_and_times_itself() {
        let l = Layout::audio(Format::F32, 48_000, 2).unwrap();
        assert!(l.is_audio());
        assert_eq!(l.frame_bytes(), 8);
        assert_eq!(l.size, 4800 * 8);
        assert_eq!(l.duration_ns(480 * 8), 10_000_000);
        assert!(Layout::audio(Format::Nv12, 48_000, 2).is_err());
        assert!(Layout::audio(Format::S16, 48_000, 0).is_err());
        assert!(Layout::new(Format::F32, 2, 48_000).is_err(), "a picture of sound is refused");
    }

    #[test]
    fn sound_formats_round_trip() {
        for f in Format::AUDIO {
            assert_eq!(Format::from_name(f.name()), Some(f));
            assert_eq!(Format::from_code(f as u32), Some(f));
        }
    }
}
