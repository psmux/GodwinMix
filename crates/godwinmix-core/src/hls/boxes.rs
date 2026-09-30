//! The two numbers DASH needs from the MP4 boxes `cmafmux` wrote: a track's
//! timescale, from the init segment's `mdhd`, and a fragment's decode time,
//! from its `tfdt`. Read, never written, and only as deep as those two boxes.

/// The body of the first box of type `kind` in `data`, a run of boxes.
fn find<'a>(mut data: &'a [u8], kind: &[u8; 4]) -> Option<&'a [u8]> {
    while data.len() >= 8 {
        let size = u32::from_be_bytes(data[0..4].try_into().ok()?) as usize;
        let (header, size) = match size {
            1 => (16, u64::from_be_bytes(data.get(8..16)?.try_into().ok()?) as usize),
            0 => (8, data.len()),
            n => (8, n),
        };
        if size < header || size > data.len() {
            return None;
        }
        if &data[4..8] == kind {
            return Some(&data[header..size]);
        }
        data = &data[size..];
    }
    None
}

/// Walk down a path of boxes.
fn path<'a>(data: &'a [u8], kinds: &[&[u8; 4]]) -> Option<&'a [u8]> {
    kinds.iter().try_fold(data, |d, k| find(d, k))
}

/// A version 0 or 1 full box's field that is 32 bits in version 0 and 64 in
/// version 1, at `offset` bytes past the version and flags.
fn sized_field(body: &[u8], offset0: usize, offset1: usize) -> Option<u64> {
    match body.first()? {
        0 => Some(u64::from(u32::from_be_bytes(body.get(4 + offset0..8 + offset0)?.try_into().ok()?))),
        1 => Some(u64::from_be_bytes(body.get(4 + offset1..12 + offset1)?.try_into().ok()?)),
        _ => None,
    }
}

/// The media timescale of the first track in an init segment.
pub fn timescale(init: &[u8]) -> Option<u32> {
    let mdhd = path(init, &[b"moov", b"trak", b"mdia", b"mdhd"])?;
    // After version and flags: creation and modification times (4 or 8
    // bytes each), then the timescale, 4 bytes in either version.
    let at = match mdhd.first()? {
        0 => 4 + 8,
        1 => 4 + 16,
        _ => return None,
    };
    Some(u32::from_be_bytes(mdhd.get(at..at + 4)?.try_into().ok()?))
}

/// The base media decode time of the first track fragment in a `moof`.
pub fn decode_time(moof: &[u8]) -> Option<u64> {
    let tfdt = path(moof, &[b"moof", b"traf", b"tfdt"])?;
    sized_field(tfdt, 0, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boxed(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut v = ((body.len() + 8) as u32).to_be_bytes().to_vec();
        v.extend_from_slice(kind);
        v.extend_from_slice(body);
        v
    }

    fn mdhd(version: u8, timescale: u32) -> Vec<u8> {
        let mut body = vec![version, 0, 0, 0];
        body.extend(std::iter::repeat_n(0u8, if version == 0 { 8 } else { 16 }));
        body.extend_from_slice(&timescale.to_be_bytes());
        body.extend_from_slice(&[0; 8]);
        boxed(b"mdhd", &body)
    }

    #[test]
    fn the_timescale_is_found_in_either_version() {
        for version in [0, 1] {
            let init = [
                boxed(b"ftyp", b"cmfc"),
                boxed(b"moov", &[boxed(b"mvhd", &[0; 20]), boxed(b"trak", &boxed(b"mdia", &mdhd(version, 90_000)))].concat()),
            ]
            .concat();
            assert_eq!(timescale(&init), Some(90_000), "version {version}");
        }
    }

    #[test]
    fn the_decode_time_is_found_in_either_version() {
        let v0 = boxed(b"tfdt", &[&[0u8, 0, 0, 0][..], &7u32.to_be_bytes()].concat());
        let v1 = boxed(b"tfdt", &[&[1u8, 0, 0, 0][..], &(1u64 << 40).to_be_bytes()].concat());
        for (tfdt, want) in [(v0, 7u64), (v1, 1 << 40)] {
            let moof = boxed(b"moof", &[boxed(b"mfhd", &[0; 8]), boxed(b"traf", &[boxed(b"tfhd", &[0; 8]), tfdt].concat())].concat());
            assert_eq!(decode_time(&moof), Some(want));
        }
    }

    #[test]
    fn a_truncated_box_is_none_not_a_panic() {
        assert_eq!(timescale(&[0, 0, 0, 200, b'm', b'o', b'o', b'v', 1, 2]), None);
        assert_eq!(decode_time(b"garbage"), None);
        assert_eq!(decode_time(&[]), None);
    }
}
