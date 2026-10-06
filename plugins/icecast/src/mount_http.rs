//! The source client's one HTTP exchange: the request libshout would send,
//! and the answer's status read through to the end of its headers.

use std::io::{BufRead, BufReader};
use std::net::TcpStream;

use super::content_type;
use crate::settings::Settings;

/// The source request, as libshout writes it for HTTP.
pub(super) fn request(s: &Settings) -> String {
    let mount = if s.mount.starts_with('/') { s.mount.clone() } else { format!("/{}", s.mount) };
    let login = base64(&format!("{}:{}", s.user, s.password));
    format!(
        "PUT {mount} HTTP/1.1\r\nHost: {}:{}\r\nAuthorization: Basic {login}\r\nUser-Agent: GodwinMix/{}\r\n\
         Content-Type: {}\r\nIce-Name: {}\r\nIce-Public: {}\r\nIce-Bitrate: {}\r\nExpect: 100-continue\r\n\r\n",
        s.host,
        s.port,
        env!("CARGO_PKG_VERSION"),
        content_type(s.format),
        s.name,
        u8::from(s.public),
        s.bitrate_kbps
    )
}

/// The status of the answer, reading through to the blank line after its
/// headers so the stream starts clean.
pub(super) fn read_answer(stream: &TcpStream) -> std::io::Result<u16> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let status = line.split_whitespace().nth(1).and_then(|c| c.parse::<u16>().ok()).unwrap_or(0);
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 || header.trim().is_empty() {
            break;
        }
    }
    Ok(status)
}

/// Standard base64, for the one header that needs it.
fn base64(text: &str) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = text.as_bytes();
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_login_is_base64_as_http_wants_it() {
        assert_eq!(base64("source:hackme"), "c291cmNlOmhhY2ttZQ==");
        assert_eq!(base64("ab"), "YWI=");
        assert_eq!(base64("abc"), "YWJj");
    }
}
