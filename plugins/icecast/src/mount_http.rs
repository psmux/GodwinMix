//! The source client's one HTTP exchange: the request libshout would send,
//! and the answer's status read through to the end of its headers.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use super::{content_type, STALL};
use crate::settings::Settings;

/// How long a server gets to answer the request before it is called down.
const ANSWER_WITHIN: Duration = Duration::from_secs(10);
/// How long a connect may take before the server counts as unreachable.
const CONNECT_WITHIN: Duration = Duration::from_secs(5);

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

/// Open the mount: the request, and the answer read up to the end of its
/// headers. A refusal comes back as a sentence that says what to change.
pub(super) fn dial(s: &Settings) -> Result<TcpStream, String> {
    let address = format!("{}:{}", s.host, s.port);
    let unreachable = |e: String| format!("could not reach the Icecast server at {address}: {e}. Check the host and port, and that the server is running.");
    let addrs = address.to_socket_addrs().map_err(|e| unreachable(e.to_string()))?;
    let mut last = String::from("the name has no address");
    let mut stream = None;
    for a in addrs {
        match TcpStream::connect_timeout(&a, CONNECT_WITHIN) {
            Ok(s) => {
                stream = Some(s);
                break;
            }
            Err(e) => last = e.to_string(),
        }
    }
    let mut stream = stream.ok_or_else(|| unreachable(last))?;
    let _ = stream.set_nodelay(true);
    stream.set_write_timeout(Some(STALL)).map_err(|e| e.to_string())?;
    stream.write_all(request(s).as_bytes()).map_err(|e| format!("the Icecast server at {address} closed the connection: {e}"))?;
    stream.set_read_timeout(Some(ANSWER_WITHIN)).map_err(|e| e.to_string())?;
    let status = read_answer(&stream).map_err(|e| format!("the Icecast server at {address} did not answer the source login: {e}"))?;
    stream.set_read_timeout(None).map_err(|e| e.to_string())?;
    match status {
        100 | 200 => Ok(stream),
        401 | 403 => Err(format!("the Icecast server at {address} refused the source login (HTTP {status}). Check the user and password; the source password is in the server's icecast.xml.")),
        400..=499 => Err(format!("the Icecast server at {address} refused the mount {} (HTTP {status}). It may be in use by another source, or not allowed by the server's settings.", s.mount)),
        other => Err(format!("the Icecast server at {address} answered HTTP {other} to the source login.")),
    }
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

    /// A pulled cable never closes the socket, so the mount's socket has to
    /// give up on a write by itself.
    #[test]
    fn a_mount_that_answers_has_a_socket_that_gives_up_on_a_stalled_write() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (mut c, _) = listener.accept().unwrap();
            let _ = c.write_all(b"HTTP/1.1 200 OK\r\n\r\n");
            std::thread::sleep(Duration::from_secs(5));
        });
        let s = Settings::from_params(&serde_json::json!({"host": "127.0.0.1", "port": port, "password": "pw"})).expect("settings");
        let stream = dial(&s).expect("the mount answered 200");
        assert_eq!(stream.write_timeout().unwrap(), Some(STALL));
    }
}
