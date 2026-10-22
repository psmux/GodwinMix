//! Annex B start codes: where each NAL unit (or MPEG-2 header) begins.

/// Every unit in `data` between start codes, without the start code and
/// without the zero bytes that pad the end of a unit before the next one,
/// as GStreamer's parsers trim them.
pub fn units(data: &[u8]) -> impl Iterator<Item = &[u8]> + '_ {
    let mut at = next_code(data, 0);
    std::iter::from_fn(move || {
        let start = at?;
        let end = next_code(data, start).map(|n| n - 3).unwrap_or(data.len());
        at = next_code(data, start);
        let mut unit = &data[start..end.max(start)];
        while let Some((&0, rest)) = unit.split_last() {
            unit = rest;
        }
        Some(unit)
    })
    .filter(|u| !u.is_empty())
}

/// The first byte after the next `00 00 01` at or after `from`.
fn next_code(data: &[u8], from: usize) -> Option<usize> {
    let mut i = from;
    while i + 2 < data.len() {
        // A skip of three when the third byte cannot end a start code.
        if data[i + 2] > 1 {
            i += 3;
        } else if data[i + 2] == 1 && data[i] == 0 && data[i + 1] == 0 {
            return Some(i + 3);
        } else {
            i += 1;
        }
    }
    None
}

/// Append `unit` with a four byte big endian length in front.
pub fn push_sized(out: &mut Vec<u8>, unit: &[u8]) {
    out.extend_from_slice(&(unit.len() as u32).to_be_bytes());
    out.extend_from_slice(unit);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_and_four_byte_start_codes_both_split_and_padding_is_trimmed() {
        let data = [0, 0, 0, 1, 0x09, 0xF0, 0, 0, 1, 0x67, 1, 2, 0, 0, 0, 0, 1, 0x68, 3];
        let got: Vec<&[u8]> = units(&data).collect();
        assert_eq!(got, vec![&[0x09, 0xF0][..], &[0x67, 1, 2][..], &[0x68, 3][..]]);
        assert_eq!(units(&[1, 2, 3]).count(), 0, "no start code, no unit");
    }
}
