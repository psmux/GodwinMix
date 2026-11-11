//! A tiny HTTP server on loopback for the fake ONVIF and Canon cameras: one
//! thread, one request per connection, the handler's answer back.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::time::Duration;

/// What a handler answers: status, extra header lines, body.
pub struct Reply {
    pub status: u16,
    pub headers: Vec<String>,
    pub body: String,
}

impl Reply {
    pub fn ok(body: impl Into<String>) -> Reply {
        Reply { status: 200, headers: Vec::new(), body: body.into() }
    }
}

/// Head and body, however many reads they take.
pub fn read_request(conn: &mut TcpStream) -> String {
    let (mut raw, mut buf) = (Vec::new(), vec![0u8; 16_384]);
    loop {
        let text = String::from_utf8_lossy(&raw).to_string();
        if let Some((head, body)) = text.split_once("\r\n\r\n") {
            let want = head.lines().find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse().unwrap_or(0))).unwrap_or(0);
            if body.len() >= want {
                return text;
            }
        }
        match conn.read(&mut buf) {
            Ok(0) | Err(_) => return text,
            Ok(n) => raw.extend_from_slice(&buf[..n]),
        }
    }
}

/// Serve `handle` on a loopback port and answer the port's address.
pub fn serve(handle: impl Fn(&str) -> Reply + Send + 'static) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let at = listener.local_addr().expect("bound");
    std::thread::spawn(move || {
        for conn in listener.incoming().flatten() {
            let mut conn = conn;
            conn.set_read_timeout(Some(Duration::from_secs(2))).ok();
            let request = read_request(&mut conn);
            let r = handle(&request);
            let reason = match r.status {
                200 => "OK",
                401 => "Unauthorized",
                _ => "Error",
            };
            let mut head = format!("HTTP/1.1 {} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n", r.status, r.body.len());
            for h in &r.headers {
                head.push_str(h);
                head.push_str("\r\n");
            }
            let _ = write!(conn, "{head}\r\n{}", r.body);
        }
    });
    at
}
