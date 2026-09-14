//! The wire types: JSON-RPC 2.0 plus the GodwinMix handshake and method bodies.
//!
//! Everything that crosses the pipe lives here and nowhere else. When the crate
//! split lands and `godwinmix-protocol` exists, this module is the thing that
//! gets deleted and re-exported instead. Nothing outside `wire` constructs a
//! JSON-RPC envelope.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A JSON-RPC id. Ids are per direction: the core's ids and the plugin's ids
/// are separate spaces and may collide without ambiguity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Id {
    Num(i64),
    Str(String),
}

/// One request or notification. A notification has no `id`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub jsonrpc: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<Id>,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl Request {
    pub fn call(id: Id, method: &str, params: Value) -> Self {
        Request {
            jsonrpc: "2.0".into(),
            id: Some(id),
            method: method.into(),
            params: Some(params),
        }
    }

    pub fn notify(method: &str, params: Value) -> Self {
        Request {
            jsonrpc: "2.0".into(),
            id: None,
            method: method.into(),
            params: Some(params),
        }
    }

    pub fn is_notification(&self) -> bool {
        self.id.is_none()
    }
}

/// One response. Exactly one of `result` and `error` is present.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub jsonrpc: String,
    pub id: Option<Id>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

impl Response {
    pub fn ok(id: Id, result: Value) -> Self {
        Response {
            jsonrpc: "2.0".into(),
            id: Some(id),
            result: Some(result),
            error: None,
        }
    }

    pub fn err(id: Option<Id>, error: RpcError) -> Self {
        Response {
            jsonrpc: "2.0".into(),
            id,
            result: None,
            error: Some(error),
        }
    }
}

/// One error. The message names the current state and the next step; `data`
/// carries what a client can act on.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl RpcError {
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        RpcError {
            code,
            message: message.into(),
            data: None,
        }
    }

    pub fn with_data(mut self, data: Value) -> Self {
        self.data = Some(data);
        self
    }
}

impl std::fmt::Display for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.code)
    }
}

impl std::error::Error for RpcError {}

/// The error codes of 03 section 6. Nothing in the SDK invents a code outside
/// this list.
pub mod codes {
    pub const PARSE_ERROR: i32 = -32700;
    pub const INVALID_REQUEST: i32 = -32600;
    pub const METHOD_NOT_FOUND: i32 = -32601;
    pub const INVALID_PARAMS: i32 = -32602;
    pub const INTERNAL_ERROR: i32 = -32603;
    /// Not in a state that allows this. Retryable after the named event.
    pub const WRONG_STATE: i32 = -32001;
    /// Refused by scope.
    pub const REFUSED_BY_SCOPE: i32 = -32002;
    /// Refused by safety; `data.retry_after_ms`.
    pub const REFUSED_BY_SAFETY: i32 = -32003;
    /// Not found (id, plugin, node).
    pub const NOT_FOUND: i32 = -32004;
    /// Placement not declared by the plugin; `data.placements`.
    pub const PLACEMENT_NOT_DECLARED: i32 = -32005;
    /// The plugin died during the call.
    pub const PLUGIN_DIED: i32 = -32010;
    /// A line exceeded the 4 MiB limit.
    pub const LINE_TOO_LONG: i32 = -32011;
    /// `configure` cannot apply this without a restart. Call `plugin.reload`.
    pub const RESTART_REQUIRED: i32 = -32012;
    /// Confirmation required; `data.confirm_token`.
    pub const CONFIRMATION_REQUIRED: i32 = -32020;
}

/// One parsed line from the peer.
#[derive(Debug, Clone)]
pub enum Message {
    /// A request carrying an id, which wants a response.
    Request(Request),
    /// A notification, which wants nothing back.
    Notification(Request),
    /// A response to something we sent.
    Response(Response),
    /// A line that was not a JSON object. The core forwards these to its log at
    /// `info`; the SDK surfaces them so a plugin talking to a third party can
    /// do the same.
    NonJson(String),
}

// ---------------------------------------------------------------------------
// The shared half
// ---------------------------------------------------------------------------

// The handshake and every method body live in `godwinmix-protocol`, because
// the core reads them with exactly the code a plugin writes them with. They
// are re-exported here under the names they always had, so
// `use godwinmix_sdk::wire::Canvas;` is unchanged.
pub use godwinmix_protocol::plugin::wire::{
    AudioLayers, AudioSet, AudioState, Candidate, Canvas, Configure, Discover, Discovered,
    Health, HealthState, InitializeResult, Initialize as InitializeParams, InstanceState,
    LogLevel, MediaReport, Position, Ready, Render, Seek, Shutdown, StartParams, StartResult,
    ToolCall, ToolResult, Transport, MAX_LINE_BYTES,
};

/// What the plugin sends first, before anything else.
///
/// The SDK's own spelling, kept because a plugin builds one and the protocol
/// crate's is the shape the core parses. The two are the same JSON.
pub type Initialize = godwinmix_protocol::plugin::wire::Initialize;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_round_trips() {
        let line = r#"{"jsonrpc":"2.0","id":7,"method":"health","params":{}}"#;
        let r: Request = serde_json::from_str(line).unwrap();
        assert_eq!(r.method, "health");
        assert_eq!(r.id, Some(Id::Num(7)));
        assert!(!r.is_notification());
    }

    #[test]
    fn string_ids_survive() {
        let line = r#"{"jsonrpc":"2.0","id":"a-1","method":"stop"}"#;
        let r: Request = serde_json::from_str(line).unwrap();
        assert_eq!(r.id, Some(Id::Str("a-1".into())));
        assert!(r.params.is_none());
    }

    #[test]
    fn notification_has_no_id_on_the_wire() {
        let n = Request::notify("log", serde_json::json!({"level": "info"}));
        let s = serde_json::to_string(&n).unwrap();
        assert!(!s.contains("\"id\""), "{s}");
    }

    #[test]
    fn canvas_frame_sizes() {
        let c = Canvas::new(1280, 720, 30);
        assert_eq!(c.i420_frame_bytes(), 1280 * 720 * 3 / 2);
        assert_eq!(c.ayuv_frame_bytes(), 1280 * 720 * 4);
        assert_eq!(c.frame_duration_ns(), 33_333_333);
        // Odd sizes round the chroma planes up rather than losing a column.
        let odd = Canvas::new(3, 3, 25);
        assert_eq!(odd.i420_frame_bytes(), 9 + 2 * 4);
    }

    #[test]
    fn ready_parses_the_documented_handshake_answer() {
        let line = r#"{"core":"godwinmix","version":"0.2.0","api_level":1,"api_compatible":1,
            "canvas":{"width":1920,"height":1080,"fps":30},
            "transport":"unixfd","media":"/run/gmx/cam1.sock",
            "instance":"cam1","provide":"source","params":{"timezone":"Europe/London"}}"#;
        let r: Ready = serde_json::from_str(line).unwrap();
        assert_eq!(r.transport, Transport::Unixfd);
        assert_eq!(r.canvas.height, 1080);
        assert_eq!(r.params["timezone"], "Europe/London");
    }
}
