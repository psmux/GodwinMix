//! HTTP logins for Canon's cameras: Basic, or Digest (RFC 2617, MD5, `qop`
//! auth), whichever the camera's HTTP server is set to. The camera says which
//! in its 401, so the first request goes without and the answer decides.

use base64::Engine;

use super::md5::hex;

/// What a camera's `WWW-Authenticate` asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Challenge {
    Basic,
    Digest { realm: String, nonce: String, opaque: Option<String>, qop: bool },
}

/// Read `WWW-Authenticate` out of a response's headers.
pub fn challenge(headers: &str) -> Option<Challenge> {
    let line = headers.lines().find_map(|l| {
        let (k, v) = l.split_once(':')?;
        k.trim().eq_ignore_ascii_case("www-authenticate").then(|| v.trim())
    })?;
    let (scheme, params) = line.split_once(' ').unwrap_or((line, ""));
    if scheme.eq_ignore_ascii_case("basic") {
        return Some(Challenge::Basic);
    }
    if !scheme.eq_ignore_ascii_case("digest") {
        return None;
    }
    let get = |key: &str| param(params, key);
    Some(Challenge::Digest {
        realm: get("realm").unwrap_or_default(),
        nonce: get("nonce")?,
        opaque: get("opaque"),
        qop: get("qop").is_some_and(|q| q.split(',').any(|v| v.trim() == "auth")),
    })
}

fn param(params: &str, key: &str) -> Option<String> {
    let mut rest = params;
    while let Some(at) = rest.find(key) {
        let before = rest[..at].chars().last();
        let after = &rest[at + key.len()..];
        if before.is_none_or(|c| c == ',' || c.is_whitespace()) && after.trim_start().starts_with('=') {
            let value = after.trim_start()[1..].trim_start();
            return Some(match value.strip_prefix('"') {
                Some(q) => q[..q.find('"').unwrap_or(q.len())].to_string(),
                None => value.split(',').next().unwrap_or("").trim().to_string(),
            });
        }
        rest = after;
    }
    None
}

/// The `Authorization` header's value for one request.
pub fn authorization(c: &Challenge, user: &str, password: &str, method: &str, uri: &str, nc: u32, cnonce: &str) -> String {
    match c {
        Challenge::Basic => format!("Basic {}", base64::engine::general_purpose::STANDARD.encode(format!("{user}:{password}"))),
        Challenge::Digest { realm, nonce, opaque, qop } => {
            let ha1 = hex(&format!("{user}:{realm}:{password}"));
            let ha2 = hex(&format!("{method}:{uri}"));
            let nc = format!("{nc:08x}");
            let response = if *qop { hex(&format!("{ha1}:{nonce}:{nc}:{cnonce}:auth:{ha2}")) } else { hex(&format!("{ha1}:{nonce}:{ha2}")) };
            let mut out = format!("Digest username=\"{user}\", realm=\"{realm}\", nonce=\"{nonce}\", uri=\"{uri}\", response=\"{response}\"");
            if *qop {
                out.push_str(&format!(", qop=auth, nc={nc}, cnonce=\"{cnonce}\""));
            }
            if let Some(o) = opaque {
                out.push_str(&format!(", opaque=\"{o}\""));
            }
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rfc_2617_worked_example_gives_its_response() {
        let headers = "HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: Digest realm=\"testrealm@host.com\", qop=\"auth,auth-int\", nonce=\"dcd98b7102dd2f0e8b11d0f600bfb0c093\", opaque=\"5ccc069c403ebaf9f0171e9517f40e41\"";
        let c = challenge(headers).unwrap();
        let header = authorization(&c, "Mufasa", "Circle Of Life", "GET", "/dir/index.html", 1, "0a4f113b");
        assert!(header.contains("response=\"6629fae49393a05397450978507c4ef1\""), "{header}");
        assert!(header.contains("opaque=\"5ccc069c403ebaf9f0171e9517f40e41\""));
    }

    #[test]
    fn basic_is_the_login_in_base64() {
        let c = challenge("WWW-Authenticate: Basic realm=\"cam\"").unwrap();
        assert_eq!(authorization(&c, "Aladdin", "open sesame", "GET", "/", 1, ""), "Basic QWxhZGRpbjpvcGVuIHNlc2FtZQ==");
    }
}
