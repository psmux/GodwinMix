//! Reading fields out of a body. Every reader answers `None` when the body
//! is too short, so a truncated block is skipped rather than misread.

pub fn u8_at(body: &[u8], at: usize) -> Option<u8> {
    body.get(at).copied()
}

pub fn bool_at(body: &[u8], at: usize) -> Option<bool> {
    u8_at(body, at).map(|v| v != 0)
}

pub fn u16_at(body: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*body.get(at)?, *body.get(at + 1)?]))
}

/// A fixed width text field, ended by the first zero byte.
pub fn text_at(body: &[u8], at: usize, width: usize) -> Option<String> {
    let field = body.get(at..at + width)?;
    let end = field.iter().position(|&b| b == 0).unwrap_or(width);
    Some(String::from_utf8_lossy(&field[..end]).into_owned())
}

/// Text of a given length, not zero ended.
pub fn text_len(body: &[u8], at: usize, len: usize) -> Option<String> {
    let field = body.get(at..at + len)?;
    Some(String::from_utf8_lossy(field).into_owned())
}
