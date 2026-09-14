//! One correlation id, from the call that caused something to the log line
//! that describes it.
//!
//! The id is a W3C trace id: sixteen bytes, written as thirty two lowercase
//! hex characters. That spelling is chosen so a caller who already speaks
//! OpenTelemetry can hand us a `traceparent` header and get its id back out of
//! our logs unchanged, and so a later OTLP export needs no translation.
//!
//! The current id lives in a Tokio task local. Every async handler that wants
//! its log lines correlated runs its body inside `with_trace_id`, and anything
//! deeper in the call, including synchronous code, reads it back with
//! `current_trace_id` without being passed an argument. A task local rather
//! than a thread local because a handler can move between worker threads
//! between two awaits, and a thread local would be read by the wrong task.

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

tokio::task_local! {
    static CURRENT: TraceId;
}

/// The trace id of the call this code is running inside, if there is one.
///
/// `None` outside a call: the supervisor's own decisions, a bus message, a
/// watchdog tick. Those lines carry no `trace_id` and that is correct, they
/// were not caused by anybody.
pub fn current_trace_id() -> Option<TraceId> {
    CURRENT.try_with(|t| *t).ok()
}

/// Run a future with `id` as the current trace id.
pub async fn with_trace_id<F: std::future::Future>(id: TraceId, fut: F) -> F::Output {
    CURRENT.scope(id, fut).await
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

    #[tokio::test]
    async fn the_current_id_is_visible_inside_the_call_and_not_outside_it() {
        assert_eq!(current_trace_id(), None);
        let id = TraceId::new();
        let seen = with_trace_id(id, async {
            tokio::task::yield_now().await;
            current_trace_id()
        })
        .await;
        assert_eq!(seen, Some(id));
        assert_eq!(current_trace_id(), None);
    }
}
