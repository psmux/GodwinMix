//! The two places the direct host hands the vitals something: the direct
//! table each time it arrives, and the `direct.thumbnail` call the station
//! makes from its control port.

use serde_json::{json, Value};

use super::Vitals;

/// The JPEG travels inside the JSON lines channel as base64. At 320 pixels
/// across a thumbnail is 8 to 15 KB, so this is about 20 KB a second for a
/// show somebody is looking at, and nothing for one nobody is.
const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn base64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, b)| n | (u32::from(*b) << (16 - 8 * i)));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(B64[(n >> (18 - 6 * i)) as usize & 63] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

impl Vitals {
    /// Watch every row of the direct table, and nothing else. `app` names
    /// the hub app a show's input is published under, which is the host's
    /// choice; the stream is always `main`.
    pub fn apply_table(&self, rows: &[Value], app: impl Fn(&str) -> String) {
        let mut ids = Vec::new();
        for row in rows {
            let Some(id) = row["id"].as_str() else { continue };
            self.watch(id, &app(id), "main", &row["monitor"]);
            ids.push(id);
        }
        self.keep(&ids);
    }

    /// `direct.thumbnail {show, width?}`: `{jpeg, width, height, at_ms}` with the
    /// JPEG in base64, or `{pending: true}` while the first keyframe is on
    /// its way, or `{status: 404, why}` for a show this host does not run.
    /// `None` for any other call, which is somebody else's.
    pub fn call(&self, name: &str, params: &Value) -> Option<Value> {
        if name != "direct.thumbnail" {
            return None;
        }
        let show = params["show"].as_str().unwrap_or_default();
        let width = params["width"].as_u64().map(|w| w as u32);
        Some(match self.thumbnail(show, width) {
            Ok(Some(t)) => json!({"jpeg": base64(&t.jpeg), "width": t.width, "height": t.height, "at_ms": t.at_ms}),
            Ok(None) => json!({"pending": true}),
            Err(why) => json!({"status": 404, "why": why}),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_standard_alphabet_and_padding() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(&[0xff, 0xd8, 0xff, 0xe0]), "/9j/4A==");
    }
}
