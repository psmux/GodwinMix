//! The session cookie, and when a request may use it.
//!
//! HttpOnly, so no script on the page can read it; SameSite=Strict, so no
//! other site's page can make a browser send it; Secure when the page is on
//! HTTPS. Named after the port, because a browser keeps cookies per host and
//! not per port, and two mixers on one machine must not sign each other out.
//!
//! The cookie is only a way to carry a session's secret. Once read it is the
//! same as `Authorization: Bearer <secret>`, so every route that takes a
//! token takes a sign in, with nothing else to change.
//!
//! Forgery: SameSite=Strict already keeps the cookie off a request another
//! site starts. On top of that a request carrying it is refused the cookie
//! when the browser says it came from another site (`Sec-Fetch-Site`) or
//! from another origin (`Origin` not matching `Host`). A bearer token is not
//! affected: a page on another site cannot set that header without the
//! token, which is the point of it.

use axum::http::{header, HeaderMap, HeaderValue};

use super::SESSION_DAYS;

const PREFIX: &str = "gmx_session_";

/// The port a request was sent to, from its `Host`: 80 when none is named.
/// `[::1]:8080` and `example.com:8080` both give 8080.
fn port_of(headers: &HeaderMap) -> String {
    let host = headers.get(header::HOST).and_then(|v| v.to_str().ok()).unwrap_or_default();
    let after = match host.rfind(']') {
        Some(end) => &host[end + 1..],
        None if host.matches(':').count() == 1 => &host[host.find(':').unwrap_or(0)..],
        None => "",
    };
    let port = after.trim_start_matches(':');
    match !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()) {
        true => port.to_string(),
        false => "80".into(),
    }
}

/// The cookie's name for the port this request reached.
pub fn name(headers: &HeaderMap) -> String {
    format!("{PREFIX}{}", port_of(headers))
}

/// The session secret in the request's cookie, if there is one and the
/// request may use it. See the module note for "may".
pub fn session(headers: &HeaderMap) -> Option<String> {
    let wanted = name(headers);
    let value = headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(k, _)| *k == wanted)
        .map(|(_, v)| v.trim().to_string())
        .filter(|v| !v.is_empty())?;
    same_origin(headers).then_some(value)
}

/// False when the browser says this request came from somewhere else.
pub fn same_origin(headers: &HeaderMap) -> bool {
    let text = |h: &str| headers.get(h).and_then(|v| v.to_str().ok()).map(str::trim);
    if let Some(site) = text("sec-fetch-site") {
        if !matches!(site, "same-origin" | "none") {
            return false;
        }
    }
    let Some(origin) = text("origin") else { return true };
    let Some(host) = text("host") else { return false };
    let origin_host = origin.split_once("://").map(|(_, rest)| rest).unwrap_or(origin);
    origin_host.trim_end_matches('/').eq_ignore_ascii_case(host)
}

/// True when the page that sent this is on HTTPS, so the cookie is marked
/// Secure. The browser's own `Origin` says, or a proxy in front says.
fn secure(headers: &HeaderMap) -> bool {
    let text = |h: &str| headers.get(h).and_then(|v| v.to_str().ok()).unwrap_or_default().to_ascii_lowercase();
    text("origin").starts_with("https://") || text("x-forwarded-proto") == "https"
}

/// `Set-Cookie` for a new session.
pub fn set(headers: &HeaderMap, secret: &str) -> HeaderValue {
    let secure = if secure(headers) { "; Secure" } else { "" };
    let max_age = SESSION_DAYS * 86_400;
    let text = format!("{}={secret}; Path=/; HttpOnly; SameSite=Strict; Max-Age={max_age}{secure}", name(headers));
    HeaderValue::from_str(&text).unwrap_or_else(|_| HeaderValue::from_static(""))
}

/// `Set-Cookie` that removes it.
pub fn clear(headers: &HeaderMap) -> HeaderValue {
    let secure = if secure(headers) { "; Secure" } else { "" };
    let text = format!("{}=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0{secure}", name(headers));
    HeaderValue::from_str(&text).unwrap_or_else(|_| HeaderValue::from_static(""))
}

/// The session as an `Authorization` header, for a request a station passes
/// on to a show. The show sees its own loopback address as the host, so it
/// could not check the origin itself; the station checks it here, at the
/// door the browser used. None when the request has a header already or no
/// usable cookie.
pub fn as_bearer(headers: &HeaderMap) -> Option<HeaderValue> {
    if headers.contains_key(header::AUTHORIZATION) {
        return None;
    }
    HeaderValue::from_str(&format!("Bearer {}", session(headers)?)).ok()
}
