//! One trace id per call, from the client when it has one.
//!
//! 10 section 2 asks that every call carry a trace id so a refusal in a log, a
//! span in a trace and the answer a client holds can all be lined up. The id
//! is taken from the W3C `traceparent` header, or from an explicit
//! `trace_id`, or generated. It comes back in the response body, in the
//! `X-Trace-Id` header, and in the tracing span for the call.
//!
//! No crate for this. A W3C trace id is 16 random bytes written as hex, and
//! the process clock plus a counter gives ids that do not collide inside one
//! core, which is as far as this core's promise goes.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// The header a client sets when it is already tracing.
pub const TRACEPARENT: &str = "traceparent";
/// The header the answer carries, for a client that is not.
pub const TRACE_ID_HEADER: &str = "x-trace-id";

/// A fresh id: 32 lowercase hex characters, as W3C writes a trace id.
pub fn new_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    // Two 64 bit halves: the clock, and a counter mixed with the process id so
    // two cores on one machine do not produce the same id in the same
    // nanosecond.
    let pid = std::process::id() as u64;
    let low = n
        .wrapping_mul(0x9e37_79b9_7f4a_7c15)
        .rotate_left(17)
        ^ pid.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    format!("{nanos:016x}{low:016x}")
}

/// The trace id in force for a call.
///
/// `traceparent` is `version-traceid-parentid-flags`, so the id is the second
/// field. A header that is not that shape is ignored rather than refused: a
/// broken trace header must never turn into a failed take.
pub fn from_parts(traceparent: Option<&str>, explicit: Option<&str>) -> String {
    if let Some(id) = explicit.map(str::trim).filter(|s| is_trace_id(s)) {
        return id.to_ascii_lowercase();
    }
    if let Some(id) = traceparent.and_then(parse_traceparent) {
        return id;
    }
    new_id()
}

/// The trace id out of a `traceparent` header, if it has one.
pub fn parse_traceparent(header: &str) -> Option<String> {
    let mut fields = header.trim().split('-');
    let _version = fields.next()?;
    let id = fields.next()?;
    // An all zero id is the specification's way of saying "invalid".
    if !is_trace_id(id) || id.bytes().all(|b| b == b'0') {
        return None;
    }
    Some(id.to_ascii_lowercase())
}

/// 32 hex characters. Anything else came from a client inventing its own
/// format, and is accepted as an opaque id only if it is short and printable.
fn is_trace_id(s: &str) -> bool {
    s.len() == 32 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// A client's own id, kept as it is when it is not a W3C one but is sane.
///
/// A UI that labels a call "take-cam1-7" should see that label in the log,
/// because that is what makes a bug report readable. Length and character set
/// are pinned so that nothing unbounded reaches a log line.
pub fn sanitise_client_id(s: &str) -> Option<String> {
    let s = s.trim();
    let ok = !s.is_empty()
        && s.len() <= 64
        && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.');
    ok.then(|| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_generated_id_is_a_w3c_trace_id_and_does_not_repeat() {
        let a = new_id();
        assert_eq!(a.len(), 32);
        assert!(a.bytes().all(|b| b.is_ascii_hexdigit()), "{a}");
        let ids: std::collections::HashSet<String> = (0..1000).map(|_| new_id()).collect();
        assert_eq!(ids.len(), 1000, "ids collided inside one process");
    }

    #[test]
    fn a_traceparent_header_wins_over_a_generated_id() {
        let header = "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01";
        assert_eq!(
            from_parts(Some(header), None),
            "0af7651916cd43dd8448eb211c80319c"
        );
        // An explicit trace_id in the params beats the header, because that is
        // the one the caller will be looking for in the answer.
        assert_eq!(
            from_parts(Some(header), Some("4bf92f3577b34da6a3ce929d0e0e4736")),
            "4bf92f3577b34da6a3ce929d0e0e4736"
        );
    }

    /// A broken trace header must never turn into a failed call. It is dropped
    /// and a fresh id is generated.
    #[test]
    fn a_broken_trace_header_is_ignored_rather_than_refused() {
        assert_eq!(parse_traceparent("nonsense"), None);
        assert_eq!(parse_traceparent("00-00000000000000000000000000000000-b7-01"), None);
        assert_eq!(parse_traceparent("00-short-b7-01"), None);
        let made = from_parts(Some("nonsense"), None);
        assert_eq!(made.len(), 32);
        assert_eq!(from_parts(None, None).len(), 32);
        assert_eq!(from_parts(None, Some("not-a-trace-id")).len(), 32);
    }

    #[test]
    fn a_clients_own_label_is_kept_if_it_is_sane() {
        assert_eq!(sanitise_client_id("take-cam1-7").as_deref(), Some("take-cam1-7"));
        assert_eq!(sanitise_client_id("  "), None);
        assert_eq!(sanitise_client_id(&"x".repeat(65)), None);
        assert_eq!(sanitise_client_id("drop table;"), None);
    }
}
