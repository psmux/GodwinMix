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
// Handshake
// ---------------------------------------------------------------------------

/// The canvas the core is running. Every frame a source sends is at these caps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Canvas {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
}

impl Canvas {
    pub const fn new(width: u32, height: u32, fps: u32) -> Self {
        Canvas { width, height, fps }
    }

    /// Bytes in one I420 frame at these caps.
    pub fn i420_frame_bytes(&self) -> usize {
        let y = self.width as usize * self.height as usize;
        // Chroma planes are half resolution in both directions, rounded up.
        let cw = self.width.div_ceil(2) as usize;
        let ch = self.height.div_ceil(2) as usize;
        y + 2 * cw * ch
    }

    /// Bytes in one AYUV frame at these caps.
    pub fn ayuv_frame_bytes(&self) -> usize {
        self.width as usize * self.height as usize * 4
    }

    /// Nanoseconds between frames.
    pub fn frame_duration_ns(&self) -> u64 {
        if self.fps == 0 {
            0
        } else {
            1_000_000_000 / self.fps as u64
        }
    }
}

impl Default for Canvas {
    fn default() -> Self {
        Canvas::new(1920, 1080, 30)
    }
}

/// The transports of 03 section 5, in the core's order of preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Transport {
    /// `unixfdsink`/`unixfdsrc`, memfd or DMABUF backed. Linux and macOS.
    Unixfd,
    /// `shmsink`/`shmsrc`, one copy per frame. Linux and macOS.
    Shm,
    /// A container on stdout. Everywhere.
    Container,
}

impl Transport {
    pub fn as_str(&self) -> &'static str {
        match self {
            Transport::Unixfd => "unixfd",
            Transport::Shm => "shm",
            Transport::Container => "container",
        }
    }
}

/// What the plugin sends first, before anything else.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Initialize {
    pub plugin: String,
    pub version: String,
    pub api: u32,
    pub transports: Vec<Transport>,
    /// The `[[provides]]` blocks of the manifest, verbatim.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provides: Vec<Value>,
}

/// What the core answers with.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ready {
    pub core: String,
    pub version: String,
    pub api_level: u32,
    pub api_compatible: u32,
    pub canvas: Canvas,
    pub transport: Transport,
    /// The unixfd or shm address. Empty in container mode.
    #[serde(default)]
    pub media: String,
    pub instance: String,
    pub provide: String,
    /// Params already validated against the settings schema.
    #[serde(default)]
    pub params: Value,
}

impl Ready {
    /// A `Ready` good enough for an offline test or a template's own harness.
    pub fn for_test(canvas: Canvas) -> Self {
        Ready {
            core: "godwinmix".into(),
            version: "0.0.0".into(),
            api_level: 1,
            api_compatible: 1,
            canvas,
            transport: Transport::Container,
            media: String::new(),
            instance: "test".into(),
            provide: "source".into(),
            params: Value::Object(Default::default()),
        }
    }
}

// ---------------------------------------------------------------------------
// Method bodies
// ---------------------------------------------------------------------------

/// The result of `initialize` as the plugin returns it to the SDK. The SDK does
/// not send this; it is how a plugin declares its measured latency.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InitializeResult {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u32>,
}

/// Params of `start`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartParams {
    pub canvas: Canvas,
    pub transport: Transport,
    #[serde(default)]
    pub media: String,
}

/// Result of `start`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StartResult {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u32>,
}

/// Result of `configure`. Never a crash, always one of these two shapes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Configure {
    pub applied: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restart_required: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl Configure {
    pub fn applied() -> Self {
        Configure {
            applied: true,
            restart_required: None,
            reason: None,
        }
    }

    pub fn restart_required(reason: impl Into<String>) -> Self {
        Configure {
            applied: false,
            restart_required: Some(true),
            reason: Some(reason.into()),
        }
    }
}

/// The three health states of `event/plugin.state` and the `health` method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HealthState {
    Ok,
    Degraded,
    Failing,
}

/// Result of `health`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Health {
    pub state: HealthState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u32>,
}

impl Health {
    pub fn ok() -> Self {
        Health {
            state: HealthState::Ok,
            detail: None,
            latency_ms: None,
        }
    }

    pub fn degraded(detail: impl Into<String>) -> Self {
        Health {
            state: HealthState::Degraded,
            detail: Some(detail.into()),
            latency_ms: None,
        }
    }

    pub fn failing(detail: impl Into<String>) -> Self {
        Health {
            state: HealthState::Failing,
            detail: Some(detail.into()),
            latency_ms: None,
        }
    }
}

impl Default for Health {
    fn default() -> Self {
        Health::ok()
    }
}

/// Params of `seek`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Seek {
    pub position_ms: u64,
}

/// Result of `seek` and of `position`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub position_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}

/// Params of `audio.set`. Gain is decibels, 0 is unity.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AudioSet {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gain_db: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub muted: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layers: Option<AudioLayers>,
}

/// Page and media levels, for a plugin declaring `audio-layers`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AudioLayers {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media: Option<Vec<Option<f64>>>,
}

/// The full audio state, read back by `audio.set`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AudioState {
    pub gain_db: f64,
    pub muted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layers: Option<AudioLayers>,
}

/// Params of `shutdown`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Shutdown {
    #[serde(default)]
    pub reason: String,
}

/// Params of `tool.call`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub name: String,
    #[serde(default)]
    pub arguments: Value,
}

/// Result of `tool.call`, in MCP's shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub content: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structured_content: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

/// Params of `discover`, for a device provide.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Discover {
    pub timeout_ms: u64,
}

/// One thing a device provide found.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candidate {
    /// A provide id such as `ndi/source`.
    #[serde(rename = "type")]
    pub kind: String,
    pub name: String,
    /// Ready for `source.add`.
    pub params: Value,
    pub confidence: f64,
}

/// Result of `discover`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Discovered {
    pub candidates: Vec<Candidate>,
}

/// Params of `render`, for a transition provide.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Render {
    pub from: Vec<String>,
    pub to: Vec<String>,
    pub progress: f64,
    pub running_time_ns: u64,
}

/// The level of a `log` notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Trace => "trace",
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Warn => "warn",
            LogLevel::Error => "error",
        }
    }
}

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
