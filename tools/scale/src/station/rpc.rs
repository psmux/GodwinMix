//! A small HTTP/1.1 client for the station's `/api/v1`, routing each method
//! by the table `core.api` publishes, so a method that lands later (as
//! `show.add_many` does) is found without a change here.

use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

pub struct Client {
    pub addr: String,
    token: Option<String>,
    routes: HashMap<String, (String, String)>,
}

/// What came back: the HTTP status and the JSON body (Null when there was none).
pub struct Answer {
    pub status: u16,
    pub body: Value,
}

impl Answer {
    pub fn ok(&self) -> bool {
        (200..300).contains(&self.status) && self.body.get("error").is_none()
    }

    pub fn why(&self) -> String {
        let e = &self.body["error"];
        e["message"].as_str().map(String::from).unwrap_or_else(|| format!("HTTP {}: {}", self.status, self.body))
    }
}

impl Client {
    pub fn connect(addr: &str, token: Option<String>) -> Result<Client, String> {
        let mut c = Client { addr: addr.trim_start_matches("http://").trim_end_matches('/').to_string(), token, routes: HashMap::new() };
        let api = c.http("GET", "/api/v1/core/api", None, 30)?;
        for m in api.body["methods"].as_array().into_iter().flatten() {
            if let (Some(name), Some(verb), Some(path)) = (m["name"].as_str(), m["rest"]["method"].as_str(), m["rest"]["path"].as_str()) {
                c.routes.insert(name.into(), (verb.into(), path.into()));
            }
        }
        if c.routes.is_empty() {
            return Err(format!("{} answered core.api with no methods. Is it a GodwinMix station?", c.addr));
        }
        Ok(c)
    }

    pub fn has(&self, method: &str) -> bool {
        self.routes.contains_key(method)
    }

    /// Calls `method` with `params`, on the show `show` when one is named.
    pub fn call(&self, method: &str, params: Value, show: Option<&str>) -> Result<Answer, String> {
        let (verb, path) = self.routes.get(method).ok_or(format!("this station has no method {method}"))?;
        let mut params = if params.is_null() { json!({}) } else { params };
        let mut path = path.clone();
        while let (Some(a), Some(b)) = (path.find('{'), path.find('}')) {
            let key = path[a + 1..b].to_string();
            let v = params.as_object_mut().and_then(|o| o.remove(&key)).ok_or(format!("{method} needs {key}"))?;
            path.replace_range(a..=b, v.as_str().map(String::from).unwrap_or(v.to_string()).as_str());
        }
        let mut query: Vec<String> = show.map(|s| format!("show={s}")).into_iter().collect();
        let body = if verb == "GET" {
            for (k, v) in params.as_object().into_iter().flatten() {
                query.push(format!("{k}={}", v.as_str().map(String::from).unwrap_or(v.to_string())));
            }
            None
        } else {
            Some(params)
        };
        if !query.is_empty() {
            path = format!("{path}?{}", query.join("&"));
        }
        self.http(verb, &path, body.as_ref(), 120)
    }

    fn http(&self, verb: &str, path: &str, body: Option<&Value>, timeout_s: u64) -> Result<Answer, String> {
        let mut s = TcpStream::connect(&self.addr).map_err(|e| format!("could not reach the station at {}: {e}", self.addr))?;
        s.set_read_timeout(Some(Duration::from_secs(timeout_s))).ok();
        let payload = body.map(|b| b.to_string()).unwrap_or_default();
        let auth = self.token.as_ref().map(|t| format!("Authorization: Bearer {t}\r\n")).unwrap_or_default();
        let head = format!(
            "{verb} {path} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{auth}\r\n",
            self.addr,
            payload.len()
        );
        s.write_all(head.as_bytes()).and_then(|_| s.write_all(payload.as_bytes())).map_err(|e| format!("could not send {verb} {path}: {e}"))?;
        let mut raw = Vec::new();
        s.read_to_end(&mut raw).map_err(|e| format!("no answer to {verb} {path} in {timeout_s} s: {e}"))?;
        parse(&raw).ok_or(format!("{verb} {path} answered something that is not HTTP"))
    }
}

/// Status line, headers, and a body that may be chunked.
pub fn parse(raw: &[u8]) -> Option<Answer> {
    let split = raw.windows(4).position(|w| w == b"\r\n\r\n")?;
    let head = String::from_utf8_lossy(&raw[..split]);
    let status = head.split_whitespace().nth(1)?.parse().ok()?;
    let mut body = raw[split + 4..].to_vec();
    if head.to_ascii_lowercase().contains("transfer-encoding: chunked") {
        body = unchunk(&body);
    }
    let body = serde_json::from_slice(&body).unwrap_or(Value::Null);
    Some(Answer { status, body })
}

fn unchunk(mut b: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    while let Some(eol) = b.windows(2).position(|w| w == b"\r\n") {
        let size = usize::from_str_radix(String::from_utf8_lossy(&b[..eol]).trim(), 16).unwrap_or(0);
        if size == 0 || eol + 2 + size > b.len() {
            break;
        }
        out.extend_from_slice(&b[eol + 2..eol + 2 + size]);
        b = &b[(eol + 4 + size).min(b.len())..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_plain_and_chunked_answers() {
        let a = parse(b"HTTP/1.1 200 OK\r\ncontent-length: 11\r\n\r\n{\"ok\":true}").unwrap();
        assert!(a.ok() && a.body["ok"] == true);
        let c = parse(b"HTTP/1.1 404 Not Found\r\ntransfer-encoding: chunked\r\n\r\n5\r\n{\"err\r\n14\r\nor\":{\"message\":\"x\"}}\r\n0\r\n\r\n").unwrap();
        assert_eq!(c.status, 404);
        assert_eq!(c.why(), "x");
    }
}
