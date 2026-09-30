//! The SDT: what each program is called, and by whom.

use super::{body, descriptors};

/// `(service id, provider, name)` for every service an SDT describes.
pub fn parse_sdt(section: &[u8]) -> Vec<(u16, String, String)> {
    let mut out = Vec::new();
    let b = body(section);
    let mut rest = b.get(3..).unwrap_or_default();
    while rest.len() >= 5 {
        let id = u16::from_be_bytes([rest[0], rest[1]]);
        let len = (usize::from(rest[3] & 0x0F) << 8) | usize::from(rest[4]);
        let Some(loop_bytes) = rest.get(5..5 + len) else { break };
        for (tag, d) in descriptors(loop_bytes) {
            if tag == 0x48 && d.len() >= 2 {
                let (provider, after) = text(&d[1..]);
                let (name, _) = text(after);
                out.push((id, provider, name));
            }
        }
        rest = &rest[5 + len..];
    }
    out
}

/// A DVB length prefixed string. A leading byte under 0x20 names a character
/// table; it is skipped and the rest read as near enough to UTF-8.
fn text(d: &[u8]) -> (String, &[u8]) {
    let len = d.first().copied().unwrap_or(0) as usize;
    let Some(raw) = d.get(1..1 + len) else { return (String::new(), &[]) };
    let raw = if raw.first().is_some_and(|&b| b < 0x20) { &raw[1..] } else { raw };
    (String::from_utf8_lossy(raw).trim().to_string(), &d[1 + len..])
}
