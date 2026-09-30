//! A SOAP POST over plain HTTP, which is how ONVIF device services are
//! reached on a LAN. No client crate: one request, one answer, a deadline.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

#[derive(Debug, PartialEq, Eq)]
pub enum Failure {
    /// The camera wants a user name and password, or refused the ones given.
    Unauthorized,
    Other(String),
}

/// `http://host[:port]/path` as its three parts.
pub fn split(url: &str) -> Result<(String, u16, String), Failure> {
    let rest = url.strip_prefix("http://").ok_or_else(|| Failure::Other(format!("{url} is not an http:// address")))?;
    let (hostport, path) = rest.split_once('/').map(|(h, p)| (h, format!("/{p}"))).unwrap_or((rest, "/".into()));
    let bad = || Failure::Other(format!("bad port in {url}"));
    let (host, port) = if let Some(v6) = hostport.strip_prefix('[') {
        let (h, after) = v6.split_once(']').ok_or_else(bad)?;
        (h.to_string(), after.strip_prefix(':').map(|p| p.parse().map_err(|_| bad())).transpose()?.unwrap_or(80))
    } else {
        match hostport.rsplit_once(':') {
            Some((h, p)) => (h.to_string(), p.parse().map_err(|_| bad())?),
            None => (hostport.to_string(), 80),
        }
    };
    Ok((host, port, path))
}

/// POST `body` to `url`; the answer's body when it is 200.
pub fn post(url: &str, body: &str, timeout: Duration) -> Result<String, Failure> {
    let (host, port, path) = split(url)?;
    let addr = (host.as_str(), port)
        .to_socket_addrs()
        .ok()
        .and_then(|mut a| a.next())
        .ok_or_else(|| Failure::Other(format!("cannot find {host}")))?;
    let mut s = TcpStream::connect_timeout(&addr, timeout).map_err(|e| Failure::Other(format!("{url}: {e}")))?;
    s.set_read_timeout(Some(timeout)).ok();
    s.set_write_timeout(Some(timeout)).ok();
    let head = format!(
        "POST {path} HTTP/1.1\r\nHost: {host}:{port}\r\nContent-Type: application/soap+xml; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    // One write: some cameras read the request in a single recv and answer
    // a head without its body as a malformed call.
    s.write_all(format!("{head}{body}").as_bytes()).map_err(|e| Failure::Other(e.to_string()))?;
    let mut raw = Vec::new();
    let _ = s.read_to_end(&mut raw);
    let text = String::from_utf8_lossy(&raw).to_string();
    let (headers, payload) = text.split_once("\r\n\r\n").ok_or_else(|| Failure::Other(format!("{url} gave no HTTP answer")))?;
    let status: u16 = headers.split_whitespace().nth(1).and_then(|c| c.parse().ok()).unwrap_or(0);
    let payload = if headers.to_ascii_lowercase().contains("transfer-encoding: chunked") { dechunk(payload) } else { payload.to_string() };
    if status == 401 || payload.contains("NotAuthorized") {
        return Err(Failure::Unauthorized);
    }
    if status != 200 {
        return Err(Failure::Other(format!("{url} answered {status}")));
    }
    Ok(payload)
}

fn dechunk(mut s: &str) -> String {
    let mut out = String::new();
    while let Some((size, rest)) = s.split_once("\r\n") {
        let n = usize::from_str_radix(size.trim(), 16).unwrap_or(0);
        if n == 0 || rest.len() < n {
            break;
        }
        out.push_str(&rest[..n]);
        s = rest[n..].trim_start_matches("\r\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_splits_and_a_chunked_body_joins() {
        assert_eq!(split("http://10.0.0.64/onvif/device_service").unwrap(), ("10.0.0.64".into(), 80, "/onvif/device_service".into()));
        assert_eq!(split("http://10.0.0.64:8000/x").unwrap(), ("10.0.0.64".into(), 8000, "/x".into()));
        assert!(split("https://x/").is_err());
        assert_eq!(dechunk("4\r\nabcd\r\n2\r\nef\r\n0\r\n\r\n"), "abcdef");
    }
}
