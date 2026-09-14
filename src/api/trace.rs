//! One trace id per call, from the client when it has one.
//!
//! 10 section 2 asks that every call carry a trace id so a refusal in a log, a
//! span in a trace and the answer a client holds can all be lined up. The id
//! is taken from the W3C `traceparent` header, or from an explicit
//! `trace_id`, or generated. It comes back in the response body, in the
//! `X-Trace-Id` header, and in the tracing span for the call.
//!
//! The id itself is `observe::TraceId` and there is no second implementation:
//! this module is the two header names and the shape the control plane wants,
//! which is a `String` in a JSON body. `observe::trace` mints and parses, puts
//! the id in a task local so every log line inside a call carries it, and
//! writes the `traceparent` for a call the core makes onwards.

pub use crate::observe::trace::{incoming, TraceId};

/// The header a client sets when it is already tracing.
pub const TRACEPARENT: &str = "traceparent";
/// The header the answer carries, for a client that is not.
pub const TRACE_ID_HEADER: &str = "x-trace-id";

/// A fresh id: 32 lowercase hex characters, as W3C writes a trace id.
pub fn new_id() -> String {
    TraceId::new().to_string()
}

/// The trace id in force for a call, as it goes in the answer.
///
/// `traceparent` is `version-traceid-parentid-flags`, so the id is the second
/// field. A header that is not that shape is ignored rather than refused: a
/// broken trace header must never turn into a failed take.
pub fn from_parts(traceparent: Option<&str>, explicit: Option<&str>) -> String {
    incoming(traceparent, explicit).to_string()
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
        assert_eq!(from_parts(Some(header), None), "0af7651916cd43dd8448eb211c80319c");
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
        assert_eq!(TraceId::from_traceparent("nonsense"), None);
        assert_eq!(
            TraceId::from_traceparent("00-00000000000000000000000000000000-b7-01"),
            None
        );
        assert_eq!(TraceId::from_traceparent("00-short-b7-01"), None);
        let made = from_parts(Some("nonsense"), None);
        assert_eq!(made.len(), 32);
        assert_eq!(from_parts(None, None).len(), 32);
        assert_eq!(from_parts(None, Some("not-a-trace-id")).len(), 32);
    }

    /// The id an answer carries and the id the logs carry are the same
    /// characters, or correlating one against the other is guesswork.
    #[test]
    fn the_answer_and_the_log_spell_the_same_id() {
        let header = "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01";
        let id = incoming(Some(header), None);
        assert_eq!(from_parts(Some(header), None), id.to_string());
        assert!(id.to_traceparent().contains(&id.to_string()));
    }

    #[test]
    fn a_clients_own_label_is_kept_if_it_is_sane() {
        assert_eq!(sanitise_client_id("take-cam1-7").as_deref(), Some("take-cam1-7"));
        assert_eq!(sanitise_client_id("  "), None);
        assert_eq!(sanitise_client_id(&"x".repeat(65)), None);
        assert_eq!(sanitise_client_id("drop table;"), None);
    }
}
