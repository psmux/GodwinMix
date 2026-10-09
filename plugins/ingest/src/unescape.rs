//! What an encoder escaped in its address, undone before the gate compares.
//!
//! A channel's application name may have a space in it (Livebox allowed one,
//! and a channel moved from it keeps its address), and a key a person typed
//! may too. OBS and ffmpeg send the space as typed. An encoder that escapes
//! its URL sends `%20`, and one that treats the query as a form sends `+`.
//! All three mean the same channel and the same key.
//!
//! `+` is a space only in a query: a key made by the mixer never has one, and
//! the core refuses one in a key a person typed, so nothing real is lost.

/// `%XX` decoded. Anything that is not a well formed escape, or decodes to
/// something that is not UTF-8, is left as it came.
pub fn percent(raw: &str) -> String {
    if !raw.contains('%') {
        return raw.to_string();
    }
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes.get(i + 1..i + 3).and_then(|h| std::str::from_utf8(h).ok()).and_then(|h| u8::from_str_radix(h, 16).ok());
        match (bytes[i], hex) {
            (b'%', Some(b)) => {
                out.push(b);
                i += 3;
            }
            (b, _) => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| raw.to_string())
}

/// A query value: `+` as a space, then `%XX` decoded.
pub fn query_value(raw: &str) -> String {
    percent(&raw.replace('+', " "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_way_an_encoder_sends_a_space_is_a_space() {
        assert_eq!(percent("Youth%20Hall"), "Youth Hall");
        assert_eq!(percent("Youth Hall"), "Youth Hall");
        assert_eq!(query_value("my+pass"), "my pass");
        assert_eq!(query_value("my%20pass"), "my pass");
    }

    #[test]
    fn what_is_not_an_escape_is_left_alone() {
        assert_eq!(percent("100%"), "100%");
        assert_eq!(percent("a%zzb"), "a%zzb");
        assert_eq!(percent("%ff%fe"), "%ff%fe", "not UTF-8, so not decoded");
        assert_eq!(percent("church"), "church");
    }
}
