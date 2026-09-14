//! Addresses, built the same way in all three client libraries.
//!
//! A token goes in the query rather than a header because an `<img>` tag, a
//! `WebSocketPeer` and a WHEP player cannot set headers. `GET` only: a token in
//! the URL of a POST ends up in more logs than it should.

/// `ws://host/rpc?token=`, from an http, https, ws or wss address.
pub fn rpc(base: &str, token: Option<&str>) -> String {
    let host = strip_scheme(base);
    let scheme = if is_secure(base) { "wss" } else { "ws" };
    format!("{scheme}://{host}/rpc{}", query(&[("token", token)]))
}

/// `GET /api/v1/snapshot/{name}`: one JPEG. `name` is "sheet", "program" or a
/// source id.
pub fn snapshot(base: &str, name: &str, width: Option<u32>, token: Option<&str>) -> String {
    let w = width.map(|w| w.to_string());
    format!(
        "{}/api/v1/snapshot/{}{}",
        http_base(base),
        escape(name),
        query(&[("width", w.as_deref()), ("token", token)])
    )
}

/// `GET /mjpeg/{name}`: `multipart/x-mixed-replace`, one JPEG per part.
pub fn mjpeg(base: &str, name: &str, width: Option<u32>, token: Option<&str>) -> String {
    let w = width.map(|w| w.to_string());
    format!(
        "{}/mjpeg/{}{}",
        http_base(base),
        escape(name),
        query(&[("width", w.as_deref()), ("token", token)])
    )
}

/// `POST /whep/{name}`: the WebRTC offer endpoint, for audio and low latency.
pub fn whep(base: &str, name: &str, token: Option<&str>) -> String {
    format!("{}/whep/{}{}", http_base(base), escape(name), query(&[("token", token)]))
}

/// The core's address as http or https, whatever scheme was handed in.
pub fn http_base(base: &str) -> String {
    let scheme = if is_secure(base) { "https" } else { "http" };
    format!("{scheme}://{}", strip_scheme(base))
}

fn is_secure(base: &str) -> bool {
    base.starts_with("https://") || base.starts_with("wss://")
}

fn strip_scheme(base: &str) -> &str {
    let rest = base
        .strip_prefix("https://")
        .or_else(|| base.strip_prefix("http://"))
        .or_else(|| base.strip_prefix("wss://"))
        .or_else(|| base.strip_prefix("ws://"))
        .unwrap_or(base);
    rest.trim_end_matches('/')
}

fn query(pairs: &[(&str, Option<&str>)]) -> String {
    let mut parts = Vec::new();
    for (key, value) in pairs {
        if let Some(v) = value {
            parts.push(format!("{key}={}", escape(v)));
        }
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!("?{}", parts.join("&"))
    }
}

/// Percent encoding for the few characters an id or a token can hold. Ids are
/// slugs and tokens are opaque, so this is a short list on purpose rather than
/// a whole URL crate.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schemes_map_across() {
        assert_eq!(rpc("http://box:8080", None), "ws://box:8080/rpc");
        assert_eq!(rpc("https://box/", Some("t")), "wss://box/rpc?token=t");
        assert_eq!(rpc("ws://box", None), "ws://box/rpc");
        assert_eq!(http_base("wss://box"), "https://box");
    }

    #[test]
    fn ids_and_tokens_are_escaped() {
        assert_eq!(
            snapshot("http://box", "cam 1", Some(320), Some("a/b")),
            "http://box/api/v1/snapshot/cam%201?width=320&token=a%2Fb"
        );
        assert_eq!(mjpeg("http://box", "program", None, None), "http://box/mjpeg/program");
        assert_eq!(whep("http://box", "program", None), "http://box/whep/program");
    }
}
