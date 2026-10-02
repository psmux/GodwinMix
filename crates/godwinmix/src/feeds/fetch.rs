//! One GET, with a timeout, a size cap and the validators that let an
//! unchanged feed cost a `304` and no body.
//!
//! reqwest is already in the binary for the station's relay, so this is one
//! more client on it and nothing new to compile. One client for every feed,
//! made the first time a feed is fetched: a core with no feeds never builds
//! it.

use godwinmix_protocol::feeds::MAX_BYTES;
use std::sync::OnceLock;
use std::time::Duration;

/// What came back.
pub struct Fetched {
    /// `None` when the server said `304 Not Modified`.
    pub body: Option<Vec<u8>>,
    pub content_type: Option<String>,
    pub validators: Validators,
}

/// `ETag` and `Last-Modified`, sent back as `If-None-Match` and
/// `If-Modified-Since`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Validators {
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

pub fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent(concat!("GodwinMix/", env!("CARGO_PKG_VERSION"), " live data"))
            .redirect(reqwest::redirect::Policy::custom(redirect))
            .connect_timeout(Duration::from_secs(10))
            .pool_max_idle_per_host(1)
            .build()
            .unwrap_or_default()
    })
}

/// Five hops, and only to `http` or `https`. A public feed that sends the
/// mixer to its own loopback is refused: that would read the control port.
fn redirect(attempt: reqwest::redirect::Attempt) -> reqwest::redirect::Action {
    let url = attempt.url();
    let first_local = attempt.previous().first().map(is_loopback).unwrap_or(false);
    if attempt.previous().len() >= 5 {
        attempt.error("more than five redirects")
    } else if !matches!(url.scheme(), "http" | "https") {
        attempt.error("a redirect to an address that is not http or https")
    } else if is_loopback(url) && !first_local {
        attempt.error("a redirect from the network to this machine")
    } else {
        attempt.follow()
    }
}

fn is_loopback(url: &reqwest::Url) -> bool {
    let host = url.host_str().unwrap_or_default().trim_matches(|c| c == '[' || c == ']');
    host.eq_ignore_ascii_case("localhost")
        || host.parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback() || ip.is_unspecified())
}

/// GET `address`, sending `headers` and the validators from last time.
pub async fn get(address: &str, headers: &[(String, String)], timeout: Duration, since: &Validators) -> Result<Fetched, String> {
    let mut req = client().get(address).timeout(timeout);
    for (k, v) in headers {
        req = req.header(k.as_str(), v.as_str());
    }
    if let Some(etag) = &since.etag {
        req = req.header(reqwest::header::IF_NONE_MATCH, etag.as_str());
    }
    if let Some(lm) = &since.last_modified {
        req = req.header(reqwest::header::IF_MODIFIED_SINCE, lm.as_str());
    }
    let mut resp = req.send().await.map_err(|e| sentence(e, timeout))?;
    let status = resp.status();
    let header = |name| resp.headers().get(name).and_then(|v: &reqwest::header::HeaderValue| v.to_str().ok()).map(str::to_string);
    let validators = Validators { etag: header(reqwest::header::ETAG), last_modified: header(reqwest::header::LAST_MODIFIED) };
    let content_type = header(reqwest::header::CONTENT_TYPE);
    if status == reqwest::StatusCode::NOT_MODIFIED {
        return Ok(Fetched { body: None, content_type, validators: since.clone() });
    }
    if !status.is_success() {
        return Err(refused(status));
    }
    if resp.content_length().is_some_and(|n| n as usize > MAX_BYTES) {
        return Err(too_big());
    }
    let mut body = Vec::new();
    while let Some(chunk) = resp.chunk().await.map_err(|e| sentence(e, timeout))? {
        if body.len() + chunk.len() > MAX_BYTES {
            return Err(too_big());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(Fetched { body: Some(body), content_type, validators })
}

pub fn too_big() -> String {
    format!(
        "the response is more than {} MB, which is as much as a feed may be. Point the feed at a smaller \
         document, or ask the server for fewer items",
        MAX_BYTES / (1024 * 1024)
    )
}

fn refused(status: reqwest::StatusCode) -> String {
    let next = match status.as_u16() {
        401 | 403 => "check the API key in the feed's headers",
        404 | 410 => "check the address in a browser",
        429 => "the server wants fewer requests: make the interval longer",
        500..=599 => "the server has a problem of its own; it is tried again",
        _ => "check the address and the headers",
    };
    format!("the server answered {status}, so {next}")
}

/// reqwest's error without its URL, which can carry a key in its query.
pub fn sentence(e: reqwest::Error, timeout: Duration) -> String {
    if e.is_timeout() {
        return format!("no answer within {} s; the server is slow or the address is wrong", timeout.as_secs_f64());
    }
    if e.is_connect() {
        return "could not connect: the server is down, the name does not resolve, or this machine is offline".into();
    }
    let e = e.without_url();
    let mut s = e.to_string();
    let mut src = std::error::Error::source(&e);
    while let Some(inner) = src {
        s = format!("{s}: {inner}");
        src = inner.source();
    }
    s
}
