//! Cutting a sound stream into its frames, for the codecs whose frames say
//! how long they are: ADTS, AC-3 and MPEG audio.
//!
//! A frame may run on into the next PES packet, so what is left over waits
//! for it. The first frame that starts in a packet takes the packet's time
//! and each frame after it is a frame's length later, as GStreamer's
//! parsers stamp them. Bytes that do not start a frame are skipped one at a
//! time until one does, which is how a parser finds its feet after loss.

use super::rate::Clock;
use super::{Ns, Pes};

/// A codec's frame header reader.
pub trait Split {
    /// How long the frame at the start of `f` is, or `None` when `f` does
    /// not start a frame (or is too short to say).
    fn frame_len(&self, f: &[u8]) -> Option<usize>;
}

/// The longest a frame can be: an E-AC-3 frame of 2048 words.
const MOST: usize = 4096;
/// Bytes a header needs before a length can be read.
const HEAD: usize = 8;

#[derive(Default)]
pub struct Framer {
    rest: Vec<u8>,
    pub clock: Clock,
}

impl Framer {
    /// Cut `pes` (after anything left over) into whole frames, each with
    /// the packet's time if it is the first to start in this packet.
    pub fn push(&mut self, split: &dyn Split, pes: &Pes, out: &mut Vec<(Option<Ns>, Vec<u8>)>) {
        let boundary = self.rest.len();
        self.rest.extend_from_slice(pes.data);
        let mut stamp = pes.pts;
        let mut at = 0;
        while self.rest.len() - at >= HEAD {
            let Some(len) = split.frame_len(&self.rest[at..]).filter(|&l| l <= MOST) else {
                at += 1;
                continue;
            };
            if self.rest.len() - at < len {
                break;
            }
            let time = if at >= boundary { stamp.take() } else { None };
            out.push((time, self.rest[at..at + len].to_vec()));
            at += len;
        }
        self.rest.drain(..at);
        if self.rest.len() > MOST {
            self.rest.clear();
        }
    }

    /// Loss: what was left over belongs to a frame that is now broken.
    pub fn reset(&mut self) {
        self.rest.clear();
    }
}
