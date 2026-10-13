//! A stand in for the GitHub API and its download host, on loopback: one
//! release, `v1.0.0`, with one asset. For a test that installs a plugin
//! the way `plugin.add owner/repo` does, with no network.

use serde_json::json;
use std::io::{BufRead, BufReader, Write};

/// A stand in for the GitHub API and its download host, on loopback.
pub fn serve_release(asset: String, bytes: Vec<u8>) -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let release = json!({ "tag_name": "v1.0.0", "assets": [
        { "name": asset, "browser_download_url": format!("{base}/dl/{asset}") }
    ]})
    .to_string();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let (release, bytes) = (release.clone(), bytes.clone());
            std::thread::spawn(move || answer(stream, &release, &bytes));
        }
    });
    base
}

/// Answer every request on one connection, reading each request whole first,
/// since closing with a request half read resets the connection on Windows.
fn answer(mut stream: std::net::TcpStream, release: &str, asset: &[u8]) {
    let Ok(clone) = stream.try_clone() else { return };
    let mut reader = BufReader::new(clone);
    loop {
        let mut request = String::new();
        if reader.read_line(&mut request).unwrap_or(0) == 0 {
            return;
        }
        loop {
            let mut header = String::new();
            match reader.read_line(&mut header) {
                Ok(0) | Err(_) => return,
                Ok(_) if header.trim().is_empty() => break,
                Ok(_) => {}
            }
        }
        let path = request.split_whitespace().nth(1).unwrap_or("").to_string();
        let body: Option<&[u8]> = if path.ends_with("/releases/latest") {
            Some(release.as_bytes())
        } else if path.starts_with("/dl/") {
            Some(asset)
        } else {
            None
        };
        let head = match body {
            Some(b) => format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: keep-alive\r\n\r\n", b.len()),
            None => "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: keep-alive\r\n\r\n".to_string(),
        };
        if stream.write_all(head.as_bytes()).is_err() || stream.write_all(body.unwrap_or(&[])).is_err() {
            return;
        }
        let _ = stream.flush();
    }
}
