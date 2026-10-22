//! MPEG-2 video to `mp2v` tags, laid out by `crate::mp2v::tag`: one
//! picture per tag, from the first header that belongs to it (sequence
//! header, GOP header, user data) to the last of its slices, as
//! `mpegvideoparse` frames it.
//!
//! An ATSC encoder puts one picture in each PES packet, so a packet is cut
//! only where a second picture's headers follow the first one's slices.
//! Every picture cut out of one packet takes the packet's times.

use super::{cts_ms, Parser, Pes, Said, Unit};

#[derive(Default)]
pub struct Mpeg2;

impl Parser for Mpeg2 {
    fn pes(&mut self, pes: &Pes, out: &mut Vec<Unit>) {
        let Some(at) = pes.at() else { return };
        for picture in pictures(pes.data) {
            let tag = crate::mp2v::tag(picture, 0, cts_ms(pes) as i32);
            out.push(Unit { at, body: tag.payload.to_vec(), keyframe: tag.keyframe, header: false });
        }
    }

    fn said(&self) -> Said {
        Said { codec: "mpeg2", ..Said::default() }
    }
}

/// The pictures in `data`, each from its first header to its last slice.
pub fn pictures(data: &[u8]) -> Vec<&[u8]> {
    let mut cuts = Vec::new();
    let mut first = None;
    let mut slices = false;
    let mut i = 0;
    while i + 3 < data.len() {
        if data[i] != 0 || data[i + 1] != 0 || data[i + 2] != 1 {
            i += 1;
            continue;
        }
        let code = data[i + 3];
        first.get_or_insert(i);
        match code {
            0x01..=0xAF => slices = true,
            0x00 | 0xB2 | 0xB3 | 0xB5 | 0xB8 if slices => {
                cuts.push(i);
                slices = false;
            }
            _ => {}
        }
        i += 4;
    }
    let Some(first) = first else { return Vec::new() };
    let mut out = Vec::new();
    let mut from = first;
    for cut in cuts {
        out.push(&data[from..cut]);
        from = cut;
    }
    out.push(&data[from..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_pictures_in_one_packet_are_cut_at_the_second_ones_headers() {
        let one = [0, 0, 1, 0xB3, 9, 0, 0, 1, 0x00, 8, 0, 0, 1, 0x01, 7];
        let two = [0, 0, 1, 0x00, 6, 0, 0, 1, 0x01, 5];
        let data = [&one[..], &two[..]].concat();
        assert_eq!(pictures(&data), vec![&one[..], &two[..]]);
    }
}
