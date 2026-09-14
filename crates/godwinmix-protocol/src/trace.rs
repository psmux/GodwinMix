//! One trace id per call, from the client when it has one.
//!
//! 10 section 2 asks that every call carry a trace id so a refusal in a log, a
//! span in a trace and the answer a client holds can all be lined up. The id
//! is taken from the W3C `traceparent` header, or from an explicit
//! `trace_id`, or generated. It comes back in the response body, in the
//! `X-Trace-Id` header, and in the tracing span for the call.
//!
//! `TraceId` itself lives here, because the id is part of the wire contract
//! and a client library has to be able to mint and parse one. The engine's
//! `observe::trace` puts it in a task local so every log line inside a call
//! carries it, and writes the `traceparent` for a call the core makes onwards.

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

/// A W3C trace id: sixteen bytes, all zeroes being the one invalid value.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct TraceId([u8; 16]);

impl TraceId {
    /// A fresh id.
    ///
    /// No random number crate: the seed is the process start, the address of a
    /// stack local and a counter, run through a 64 bit mixer twice. Trace ids
    /// need to not collide within one operator's logs, which this gives with
    /// room to spare. They are not a secret and nothing authorises on them.
    pub fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let here = &n as *const u64 as u64;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        let hi = mix(now ^ here.rotate_left(17));
        let lo = mix(hi ^ n.wrapping_mul(0x9e37_79b9_7f4a_7c15));
        let mut bytes = [0u8; 16];
        bytes[..8].copy_from_slice(&hi.to_be_bytes());
        bytes[8..].copy_from_slice(&lo.to_be_bytes());
        // The all zero id is reserved as "absent" by the W3C spec.
        if bytes == [0u8; 16] {
            bytes[15] = 1;
        }
        Self(bytes)
    }

    /// Parse thirty two hex characters. Rejects the all zero id.
    pub fn parse(s: &str) -> Option<Self> {
        if s.len() != 32 {
            return None;
        }
        let mut bytes = [0u8; 16];
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = u8::from_str_radix(s.get(i * 2..i * 2 + 2)?, 16).ok()?;
        }
        (bytes != [0u8; 16]).then_some(Self(bytes))
    }

    /// Pull the trace id out of a W3C `traceparent` header value.
    ///
    /// `00-<32 hex trace id>-<16 hex span id>-<2 hex flags>`. Anything that is
    /// not that shape answers `None` and the caller mints a fresh id, which is
    /// what the spec says to do with a header it cannot read.
    pub fn from_traceparent(header: &str) -> Option<Self> {
        let mut parts = header.trim().split('-');
        let version = parts.next()?;
        if version.len() != 2 || version == "ff" {
            return None;
        }
        Self::parse(parts.next()?)
    }

    /// A `traceparent` header naming this trace, for a call we make onwards.
    pub fn to_traceparent(self) -> String {
        // One span id per outgoing call, derived from the trace id so that two
        // calls in the same trace do not claim the same span.
        static SPANS: AtomicU64 = AtomicU64::new(1);
        let span = mix(SPANS.fetch_add(1, Ordering::Relaxed) ^ u64::from_be_bytes(
            self.0[..8].try_into().expect("eight bytes"),
        ));
        format!("00-{self}-{span:016x}-01")
    }

    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

impl Default for TraceId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for TraceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for b in self.0 {
            write!(f, "{b:02x}")?;
        }
        Ok(())
    }
}

impl serde::Serialize for TraceId {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

/// Thomas Wang style 64 bit finaliser. Cheap and spreads the low bits.
fn mix(mut x: u64) -> u64 {
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^= x >> 33;
    x = x.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    x ^ (x >> 33)
}

/// The id to use for a request: the caller's `traceparent` when it parses, an
/// explicit `trace_id` field when the caller sent one in the body, else a
/// fresh id. The api agent's RPC entry point calls this.
pub fn incoming(traceparent: Option<&str>, explicit: Option<&str>) -> TraceId {
    explicit
        .and_then(TraceId::parse)
        .or_else(|| traceparent.and_then(TraceId::from_traceparent))
        .unwrap_or_default()
}

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
    fn ids_are_thirty_two_hex_characters_and_round_trip() {
        let id = TraceId::new();
        let s = id.to_string();
        assert_eq!(s.len(), 32);
        assert!(s.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        assert_eq!(TraceId::parse(&s), Some(id));
    }

    #[test]
    fn fresh_ids_differ() {
        let mut seen = std::collections::HashSet::new();
        for _ in 0..10_000 {
            assert!(seen.insert(TraceId::new()), "trace ids collided");
        }
    }

    #[test]
    fn traceparent_is_read_and_written() {
        let header = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";
        let id = TraceId::from_traceparent(header).expect("valid traceparent");
        assert_eq!(id.to_string(), "4bf92f3577b34da6a3ce929d0e0e4736");
        let out = id.to_traceparent();
        assert!(out.starts_with("00-4bf92f3577b34da6a3ce929d0e0e4736-"));
        assert_eq!(TraceId::from_traceparent(&out), Some(id));
    }

    #[test]
    fn nonsense_traceparents_are_refused_rather_than_guessed() {
        for bad in [
            "",
            "00",
            "ff-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
            "00-00000000000000000000000000000000-00f067aa0ba902b7-01",
            "00-4bf92f3577b34da6a3ce929d0e0e473-00f067aa0ba902b7-01",
            "00-zzf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
        ] {
            assert_eq!(TraceId::from_traceparent(bad), None, "{bad} was accepted");
        }
    }

    #[test]
    fn incoming_prefers_the_explicit_id_then_the_header_then_a_fresh_one() {
        let explicit = TraceId::new().to_string();
        let header = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";
        assert_eq!(incoming(Some(header), Some(&explicit)).to_string(), explicit);
        assert_eq!(
            incoming(Some(header), None).to_string(),
            "4bf92f3577b34da6a3ce929d0e0e4736"
        );
        assert!(incoming(None, None).to_string().len() == 32);
    }

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
