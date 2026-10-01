//! The two tables a receiver reads first: the PAT says where the program's
//! map is, and the PMT says which PID carries which codec.

/// The PIDs this muxer writes. Fixed, as a single program feed's are.
pub const PMT_PID: u16 = 0x1000;
pub const VIDEO_PID: u16 = 0x0100;
pub const AUDIO_PID: u16 = 0x0101;
const PROGRAM: u16 = 1;

/// MPEG-2's CRC32: polynomial 0x04C11DB7, not reflected, no final xor.
pub fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &b in bytes {
        crc ^= u32::from(b) << 24;
        for _ in 0..8 {
            crc = if crc & 0x8000_0000 != 0 { (crc << 1) ^ 0x04c1_1db7 } else { crc << 1 };
        }
    }
    crc
}

/// A long form section: the header, `body`, and the CRC.
fn section(table_id: u8, id: u16, body: &[u8]) -> Vec<u8> {
    let length = 5 + body.len() + 4;
    let mut s = vec![table_id, 0xb0 | ((length >> 8) as u8 & 0x0f), length as u8];
    s.extend_from_slice(&id.to_be_bytes());
    s.extend_from_slice(&[0xc1, 0x00, 0x00]); // version 0, current, section 0 of 0
    s.extend_from_slice(body);
    let crc = crc32(&s);
    s.extend_from_slice(&crc.to_be_bytes());
    s
}

/// The program association table: one program, its map at `PMT_PID`.
pub fn pat() -> Vec<u8> {
    let mut body = PROGRAM.to_be_bytes().to_vec();
    body.extend_from_slice(&(0xe000 | PMT_PID).to_be_bytes());
    section(0x00, 1, &body)
}

/// The program map: each elementary stream's type and PID, and the PID the
/// clock rides on.
pub fn pmt(video: Option<u8>, audio: Option<u8>) -> Vec<u8> {
    let pcr = if video.is_some() { VIDEO_PID } else { AUDIO_PID };
    let mut body = (0xe000 | pcr).to_be_bytes().to_vec();
    body.extend_from_slice(&[0xf0, 0x00]); // no program descriptors
    for (kind, pid) in [(video, VIDEO_PID), (audio, AUDIO_PID)] {
        if let Some(kind) = kind {
            body.push(kind);
            body.extend_from_slice(&(0xe000 | pid).to_be_bytes());
            body.extend_from_slice(&[0xf0, 0x00]);
        }
    }
    section(0x02, PROGRAM, &body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_crc_is_mpeg_twos() {
        // The PAT every single program muxer writes for program 1 at 0x1000.
        assert_eq!(pat(), [0x00, 0xb0, 0x0d, 0x00, 0x01, 0xc1, 0x00, 0x00, 0x00, 0x01, 0xf0, 0x00, 0x2a, 0xb1, 0x04, 0xb2]);
        // A section with its CRC appended checks to zero.
        assert_eq!(crc32(&pmt(Some(0x1b), Some(0x0f))), 0);
    }
}
