//! JSON-RPC 2.0 on the wire, and the event stream that rides beside it.
//!
//! One message per text frame on `/rpc`. Requests carry an `id` and get a
//! response; notifications carry none and get nothing back, which is what the
//! specification says and what a client that sends `initialized` expects.
//!
//! Events are notifications from the core, named `event/<name>`. Every one
//! carries `seq`, every batch ends with `event/flush`, and a client that falls
//! behind is told `event/resync` rather than being left with a hole it cannot
//! see.

use crate::error::{ErrorCode, RpcError};
use crate::requests::{Ext, Meters};
use crate::types::Event;
use serde_json::{json, Map, Value};

/// One parsed request off the wire.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    /// Absent on a notification, which gets no answer.
    pub id: Option<Value>,
    pub method: String,
    pub params: Value,
}

/// What went wrong before a method was even looked up.
#[derive(Debug, Clone, PartialEq)]
pub struct Malformed {
    pub id: Value,
    pub error: RpcError,
}

/// Parse one text frame.
///
/// Batches are not accepted: they complicate ordering against an event stream
/// for no gain a UI ever asked for, and saying so plainly beats half
/// supporting them.
pub fn parse(line: &str) -> Result<Request, Malformed> {
    let value: Value = serde_json::from_str(line).map_err(|e| Malformed {
        id: Value::Null,
        error: RpcError::new(ErrorCode::ParseError, format!("the frame was not JSON: {e}")),
    })?;
    if value.is_array() {
        return Err(Malformed {
            id: Value::Null,
            error: RpcError::new(
                ErrorCode::InvalidRequest,
                "batches are not accepted on /rpc. Send one request per frame.",
            ),
        });
    }
    let id = value.get("id").cloned().filter(|v| !v.is_null());
    let Some(method) = value.get("method").and_then(Value::as_str) else {
        return Err(Malformed {
            id: id.unwrap_or(Value::Null),
            error: RpcError::new(
                ErrorCode::InvalidRequest,
                "a request needs a \"method\". Call core.api for the list.",
            ),
        });
    };
    let params = match value.get("params") {
        None | Some(Value::Null) => json!({}),
        Some(v) if v.is_object() => v.clone(),
        Some(_) => {
            return Err(Malformed {
                id: id.unwrap_or(Value::Null),
                error: RpcError::new(
                    ErrorCode::InvalidParams,
                    "params must be an object. Positional params are not accepted.",
                ),
            })
        }
    };
    Ok(Request { id, method: method.to_string(), params })
}

pub fn result_frame(id: &Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

pub fn error_frame(id: &Value, error: &RpcError, trace_id: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": error.code,
            "message": error.message,
            "data": error.data,
            "trace_id": trace_id,
        }
    })
}

/// A core to client notification: an event, or anything else with no id.
pub fn notification(method: &str, params: Value) -> Value {
    json!({ "jsonrpc": "2.0", "method": method, "params": params })
}

/// The envelope keys the dispatcher handles for every method, pulled off the
/// params before they reach a handler.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CallEnvelope {
    pub trace_id: Option<String>,
    pub idempotency_key: Option<String>,
    pub dry_run: bool,
    pub confirm: Option<String>,
}

impl CallEnvelope {
    /// Read the envelope. The keys stay in the params: a handler ignores what
    /// it does not know, and leaving them means a replay hashes the same body
    /// the client sent.
    pub fn read(params: &Value) -> Self {
        let s = |k: &str| {
            params.get(k).and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty()).map(String::from)
        };
        Self {
            trace_id: s("trace_id"),
            idempotency_key: s("idempotency_key"),
            dry_run: params.get("dry_run").and_then(Value::as_bool).unwrap_or(false),
            confirm: s("confirm"),
        }
    }
}

/// What a client asked to be sent.
#[derive(Debug, Clone, Default)]
pub struct Subscription {
    /// Patterns against the part after `event/`. Empty means everything.
    pub patterns: Vec<String>,
    pub ext: Ext,
}

impl Subscription {
    /// Does this client want `event/<name>`?
    ///
    /// An expensive stream is governed by its `ext` key alone. Asking for
    /// `ext.meters` is asking for meters, and the attach written out in 05
    /// section 2 does exactly that without naming `meters` among its event
    /// patterns; making a client say it twice would be a trap. The patterns
    /// govern everything the core publishes anyway, and a client that asks for
    /// none of the ext keys costs the core nothing, which is the point of the
    /// table.
    pub fn wants(&self, name: &str) -> bool {
        match name {
            "meters" => self.ext.meters,
            "tally" => self.ext.tally,
            "source.position" => self.ext.positions,
            "multiview.layout" | "multiview.frame" => self.ext.wants_multiview(),
            // The stream's own bookkeeping is never filtered out: a client
            // that missed a flush would never render.
            "snapshot" | "flush" | "resync" => true,
            other => self.matches(other),
        }
    }

    fn matches(&self, name: &str) -> bool {
        if self.patterns.is_empty() {
            return true;
        }
        self.patterns.iter().any(|p| pattern_matches(p, name))
    }
}

/// `program.*` matches `program.took`. `*` on its own matches everything. A
/// pattern with no star matches exactly.
pub fn pattern_matches(pattern: &str, name: &str) -> bool {
    match pattern.split_once('*') {
        None => pattern == name,
        Some((head, tail)) => {
            name.len() >= head.len() + tail.len()
                && name.starts_with(head)
                && name.ends_with(tail)
        }
    }
}

/// The `event/<name>` a legacy `Event` is published under, and its payload.
///
/// The two meter variants are deliberately absent: they are coalesced into one
/// `event/meters` per batch by the connection, because 03 section 6 has one
/// meters event carrying the programme and every source rather than one
/// message per meter.
pub fn event_name_and_payload(event: &Event) -> Option<(&'static str, Value)> {
    let payload = |v: Value| v;
    Some(match event {
        Event::Status(_) => return None,
        Event::Took { source, scene, at_running_time_ms } => (
            "program.took",
            payload(json!({
                "source": source,
                // A bare source id is shorthand for a one item full canvas
                // scene, so a client reading `scene` gets an answer either way.
                "scene": scene.clone().or_else(|| source.clone()),
                "transition": "cut",
                "duration_ms": 0,
                "at_running_time_ms": at_running_time_ms,
            })),
        ),
        Event::PreviewChanged { scene } => {
            ("preview.changed", payload(json!({ "scene": scene })))
        }
        Event::SourceStateChanged { source, state } => (
            "source.state",
            payload(json!({ "source": source, "state": state, "detail": Value::Null })),
        ),
        Event::OutputStateChanged { output, state, reconnects } => (
            "output.state",
            payload(json!({ "output": output, "state": state, "reconnects": reconnects })),
        ),
        Event::AdBreakChanged { ad } => ("adbreak.changed", payload(json!({ "ad": ad }))),
        Event::UiChanged { ui } => ("ui.changed", payload(json!({ "ui": ui }))),
        Event::HookBlocked { hook, plugin, reason } => (
            "hook.blocked",
            payload(json!({ "hook": hook, "plugin": plugin, "reason": reason })),
        ),
        Event::SourcePosition { source, position_ms, duration_ms } => (
            "source.position",
            payload(json!({
                "source": source, "position_ms": position_ms, "duration_ms": duration_ms
            })),
        ),
        Event::Alert { severity, message } => (
            "alert",
            payload(json!({ "severity": severity, "message": message })),
        ),
        Event::MediaChanged { name, conversion } => (
            "media.changed",
            payload(json!({ "name": name, "conversion": conversion })),
        ),
        Event::AudioLevel { .. } | Event::SourceAudioLevel { .. } => return None,
    })
}

/// Meters, gathered across a batch so that one message carries the programme
/// and every source rather than one message per meter.
#[derive(Debug, Default)]
pub struct MeterBatch {
    program: Option<Vec<f64>>,
    sources: Map<String, Value>,
}

impl MeterBatch {
    pub fn absorb(&mut self, event: &Event) -> bool {
        match event {
            Event::AudioLevel { peak_db } => {
                self.program = Some(peak_db.clone());
                true
            }
            Event::SourceAudioLevel { source, peak_db } => {
                self.sources.insert(source.clone(), json!(peak_db));
                true
            }
            _ => false,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.program.is_none() && self.sources.is_empty()
    }

    /// Take what has gathered, leaving the batch empty.
    pub fn take(&mut self) -> Option<Meters> {
        if self.is_empty() {
            return None;
        }
        Some(Meters {
            program: self.program.take().unwrap_or_default(),
            sources: std::mem::take(&mut self.sources),
        })
    }
}

/// Bytes in front of every mosaic JPEG on `/rpc`.
///
/// Sixteen bytes, little endian, as 03 section 6 writes it: the event sequence
/// number, the layout id, and the programme running time in milliseconds. The
/// audit's complaint was that a raw JPEG with no header leaves a slow client
/// unable to tell which frame matched which layout; the layout id is the
/// answer to exactly that.
pub const FRAME_HEADER_BYTES: usize = 16;

pub fn frame_header(seq: u32, layout: u32, running_time_ms: u64) -> [u8; FRAME_HEADER_BYTES] {
    let mut out = [0u8; FRAME_HEADER_BYTES];
    out[0..4].copy_from_slice(&seq.to_le_bytes());
    out[4..8].copy_from_slice(&layout.to_le_bytes());
    out[8..16].copy_from_slice(&running_time_ms.to_le_bytes());
    out
}

/// Read a header back. Here so that a client library in any language has a
/// reference implementation to check itself against, and so the test below is
/// testing the pair rather than a restatement of the writer.
pub fn read_frame_header(bytes: &[u8]) -> Option<(u32, u32, u64)> {
    if bytes.len() < FRAME_HEADER_BYTES {
        return None;
    }
    let seq = u32::from_le_bytes(bytes[0..4].try_into().ok()?);
    let layout = u32::from_le_bytes(bytes[4..8].try_into().ok()?);
    let time = u64::from_le_bytes(bytes[8..16].try_into().ok()?);
    Some((seq, layout, time))
}

/// A layout id that every client works out the same way: a hash of the cells.
///
/// Derived rather than counted so two connections that joined at different
/// moments agree about which layout a frame belongs to, with no shared state
/// between them.
pub fn layout_id(cells: &[crate::types::CellAssignment]) -> u32 {
    // FNV-1a over the cell geometry. Small, stable across runs, and no crate.
    let mut hash: u32 = 0x811c_9dc5;
    let mut eat = |byte: u8| {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(0x0100_0193);
    };
    for c in cells {
        for n in [c.index as i64, c.x as i64, c.y as i64, c.w as i64, c.h as i64] {
            for byte in n.to_le_bytes() {
                eat(byte);
            }
        }
        for byte in c.source.as_deref().unwrap_or("").as_bytes() {
            eat(*byte);
        }
        eat(0);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{CellAssignment, Severity, SourceState};

    #[test]
    fn a_request_and_a_notification_are_told_apart_by_the_id() {
        let r = parse(r#"{"jsonrpc":"2.0","id":1,"method":"program.take","params":{"source":"cam1"}}"#)
            .unwrap();
        assert_eq!(r.id, Some(json!(1)));
        assert_eq!(r.method, "program.take");
        assert_eq!(r.params["source"], "cam1");

        let n = parse(r#"{"jsonrpc":"2.0","method":"initialized"}"#).unwrap();
        assert_eq!(n.id, None);
        assert_eq!(n.params, json!({}));

        // A null id is a notification, not an id of null.
        assert_eq!(parse(r#"{"jsonrpc":"2.0","id":null,"method":"x"}"#).unwrap().id, None);
    }

    #[test]
    fn malformed_frames_name_the_next_step() {
        let e = parse("{not json").unwrap_err();
        assert_eq!(e.error.code, ErrorCode::ParseError.number());
        let e = parse(r#"{"jsonrpc":"2.0","id":3}"#).unwrap_err();
        assert_eq!(e.error.code, ErrorCode::InvalidRequest.number());
        assert_eq!(e.id, json!(3), "the id has to come back so a client can match it");
        assert!(e.error.message.contains("core.api"), "{}", e.error.message);
        // Positional params and batches, both refused in words.
        let e = parse(r#"{"jsonrpc":"2.0","id":4,"method":"x","params":[1]}"#).unwrap_err();
        assert_eq!(e.error.code, ErrorCode::InvalidParams.number());
        let e = parse(r#"[{"jsonrpc":"2.0","id":5,"method":"x"}]"#).unwrap_err();
        assert!(e.error.message.contains("one request per frame"), "{}", e.error.message);
    }

    #[test]
    fn the_envelope_keys_are_read_off_any_method() {
        let e = CallEnvelope::read(&json!({
            "source": "cam1", "trace_id": "abc", "idempotency_key": "k1",
            "dry_run": true, "confirm": "cfm-1"
        }));
        assert_eq!(e.trace_id.as_deref(), Some("abc"));
        assert_eq!(e.idempotency_key.as_deref(), Some("k1"));
        assert!(e.dry_run);
        assert_eq!(e.confirm.as_deref(), Some("cfm-1"));
        // Empty strings count as absent, so a client filling a template in
        // does not send a key of "".
        let e = CallEnvelope::read(&json!({ "trace_id": "  ", "idempotency_key": "" }));
        assert_eq!(e, CallEnvelope::default());
    }

    #[test]
    fn patterns_match_the_part_after_the_slash() {
        assert!(pattern_matches("program.*", "program.took"));
        assert!(!pattern_matches("program.*", "source.state"));
        assert!(pattern_matches("*", "anything.at.all"));
        assert!(pattern_matches("alert", "alert"));
        assert!(!pattern_matches("alert", "alerts"));
        assert!(pattern_matches("*.state", "source.state"));
    }

    /// The whole point of the ext table: a client that subscribed to
    /// everything still gets nothing expensive unless it asked by name.
    #[test]
    fn ext_gates_the_expensive_streams_even_under_a_star() {
        let everything = Subscription { patterns: vec!["*".into()], ext: Ext::default() };
        assert!(everything.wants("program.took"));
        assert!(!everything.wants("meters"));
        assert!(!everything.wants("tally"));
        assert!(!everything.wants("source.position"));
        assert!(!everything.wants("multiview.frame"));
        // Bookkeeping is never filtered, or a client would never render.
        assert!(everything.wants("flush"));
        assert!(everything.wants("snapshot"));
        assert!(everything.wants("resync"));

        // A Stream Deck: tally and nothing else. It never names `tally` among
        // its patterns, because asking for the ext key is asking for the
        // stream.
        let deck = Subscription {
            patterns: vec!["program.*".into()],
            ext: Ext { tally: true, ..Ext::default() },
        };
        assert!(deck.wants("tally"));
        assert!(deck.wants("program.took"));
        assert!(!deck.wants("meters"));
        assert!(!deck.wants("source.state"));

        // The attach written out in 05 section 2, which names no ext key among
        // its patterns and must still get its meters.
        let ui = Subscription {
            patterns: vec![
                "program.*".into(),
                "source.*".into(),
                "output.*".into(),
                "alert".into(),
            ],
            ext: Ext { meters: true, tally: true, ..Ext::default() },
        };
        assert!(ui.wants("meters"), "the attach in 05 section 2 has to get its meters");
        assert!(ui.wants("tally"));
        assert!(ui.wants("program.took"));
        assert!(!ui.wants("multiview.frame"));
        // `source.*` matches the position event's name, and it still takes the
        // ext key to turn that stream on.
        assert!(!ui.wants("source.position"));
    }

    #[test]
    fn legacy_events_are_renamed_onto_the_published_table() {
        let name = |e: Event| event_name_and_payload(&e).map(|(n, _)| n);
        assert_eq!(
            name(Event::Took { source: None, scene: None, at_running_time_ms: 1 }),
            Some("program.took")
        );
        assert_eq!(
            name(Event::SourceStateChanged { source: "cam1".into(), state: SourceState::Live }),
            Some("source.state")
        );
        assert_eq!(
            name(Event::Alert { severity: Severity::Warning, message: "x".into() }),
            Some("alert")
        );
        // Meters are coalesced by the connection, not published one per meter.
        assert_eq!(name(Event::AudioLevel { peak_db: vec![-6.0] }), None);
        assert_eq!(
            name(Event::SourceAudioLevel { source: "cam1".into(), peak_db: vec![-6.0] }),
            None
        );

        let (_, payload) =
            event_name_and_payload(&Event::Took {
                source: Some("cam1".into()),
                scene: None,
                at_running_time_ms: 42,
            })
                .unwrap();
        assert_eq!(payload["source"], "cam1");
        assert_eq!(payload["at_running_time_ms"], 42);
    }

    #[test]
    fn a_batch_of_meters_becomes_one_message() {
        let mut batch = MeterBatch::default();
        assert!(batch.take().is_none());
        assert!(batch.absorb(&Event::AudioLevel { peak_db: vec![-6.0, -6.5] }));
        assert!(batch.absorb(&Event::SourceAudioLevel {
            source: "cam1".into(),
            peak_db: vec![-12.0]
        }));
        assert!(!batch.absorb(&Event::Took { source: None, scene: None, at_running_time_ms: 0 }));
        let m = batch.take().unwrap();
        assert_eq!(m.program, vec![-6.0, -6.5]);
        assert_eq!(m.sources["cam1"][0], -12.0);
        // Taking empties it, so the next batch starts clean.
        assert!(batch.take().is_none());
    }

    #[test]
    fn the_frame_header_round_trips() {
        let h = frame_header(4821, 7, 1_234_567);
        assert_eq!(h.len(), FRAME_HEADER_BYTES);
        assert_eq!(read_frame_header(&h), Some((4821, 7, 1_234_567)));
        // Little endian, so the low byte comes first. A client decoding this
        // in JavaScript passes `true` to `getUint32`.
        assert_eq!(h[0], (4821u32 & 0xff) as u8);
        assert!(read_frame_header(&h[..15]).is_none());
    }

    /// Two clients that joined at different moments have to agree on the
    /// layout id, so it is derived from the cells rather than counted.
    #[test]
    fn the_layout_id_comes_from_the_cells() {
        let cell = |i: u32, s: Option<&str>| CellAssignment {
            index: i,
            source: s.map(String::from),
            x: 0,
            y: 0,
            w: 480,
            h: 270,
        };
        let a = vec![cell(0, None), cell(1, Some("cam1"))];
        let b = vec![cell(0, None), cell(1, Some("cam1"))];
        assert_eq!(layout_id(&a), layout_id(&b));
        let c = vec![cell(0, None), cell(1, Some("cam2"))];
        assert_ne!(layout_id(&a), layout_id(&c));
        let d = vec![cell(0, None)];
        assert_ne!(layout_id(&a), layout_id(&d));
        assert_eq!(layout_id(&[]), layout_id(&[]));
    }
}
