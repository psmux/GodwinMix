//! What crosses the pipe between the core and a tier 2 plugin.
//!
//! JSON-RPC 2.0, one object per line, UTF-8, at most 4 MiB a line. The core
//! writes on the plugin's stdin and reads its stderr; stdout carries media in
//! container mode. Both directions may have several requests in flight, and
//! ids are per direction: the core's ids and the plugin's ids are separate
//! spaces and may collide without ambiguity.
//!
//! The envelope parser here is deliberately its own rather than
//! `crate::rpc::parse`: that one reads a request off a WebSocket and refuses
//! anything else, where a plugin channel carries requests, notifications,
//! responses and, for a Python traceback or a library warning, lines that are
//! not JSON at all.

use crate::error::ErrorCode;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// The maximum length of one protocol line. A longer one is `-32011`, the
/// channel closes and the instance goes to `failed`.
pub const MAX_LINE_BYTES: usize = 4 * 1024 * 1024;

/// One parsed line from the peer.
#[derive(Debug, Clone, PartialEq)]
pub enum Frame {
    /// A request carrying an id, which wants a response.
    Request { id: Value, method: String, params: Value },
    /// A notification, which wants nothing back.
    Notification { method: String, params: Value },
    /// An answer to something we sent.
    Response { id: Value, result: Result<Value, WireError> },
    /// A line that was not a JSON object. The core forwards these to its log
    /// at `info`, tagged with the instance, so a traceback lands in the log
    /// rather than in the protocol.
    NonJson(String),
}

/// One error as it travels on the plugin channel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WireError {
    pub code: i64,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl WireError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self { code: code.number(), message: message.into(), data: None }
    }
}

impl std::fmt::Display for WireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.code)
    }
}

impl std::error::Error for WireError {}

/// Read one line off the channel.
///
/// Never fails: a line that is not an object is a `NonJson` frame, because the
/// alternative is dropping a plugin for printing a warning.
pub fn parse_line(line: &str) -> Frame {
    let trimmed = line.trim_end_matches(['\r', '\n']);
    let Ok(Value::Object(map)) = serde_json::from_str::<Value>(trimmed) else {
        return Frame::NonJson(trimmed.to_string());
    };
    let id = map.get("id").cloned().filter(|v| !v.is_null());
    if let Some(method) = map.get("method").and_then(Value::as_str) {
        let params = match map.get("params") {
            None | Some(Value::Null) => json!({}),
            Some(v) => v.clone(),
        };
        return match id {
            Some(id) => Frame::Request { id, method: method.to_string(), params },
            None => Frame::Notification { method: method.to_string(), params },
        };
    }
    let Some(id) = id else {
        return Frame::NonJson(trimmed.to_string());
    };
    if let Some(error) = map.get("error") {
        let error = serde_json::from_value(error.clone()).unwrap_or(WireError {
            code: ErrorCode::InternalError.number(),
            message: error.to_string(),
            data: None,
        });
        return Frame::Response { id, result: Err(error) };
    }
    Frame::Response { id, result: Ok(map.get("result").cloned().unwrap_or(Value::Null)) }
}

/// One request the core sends a plugin, as a line with its newline already on.
pub fn request_line(id: i64, method: &str, params: Value) -> String {
    let mut line = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
        .to_string();
    line.push('\n');
    line
}

/// One notification the core sends a plugin.
pub fn notification_line(method: &str, params: Value) -> String {
    let mut line = json!({"jsonrpc": "2.0", "method": method, "params": params}).to_string();
    line.push('\n');
    line
}

/// The core's answer to a request a plugin made of it.
pub fn response_line(id: &Value, result: Result<Value, WireError>) -> String {
    let mut line = match result {
        Ok(value) => json!({"jsonrpc": "2.0", "id": id, "result": value}),
        Err(e) => json!({"jsonrpc": "2.0", "id": id, "error": e}),
    }
    .to_string();
    line.push('\n');
    line
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
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
    pub const fn as_str(self) -> &'static str {
        match self {
            Transport::Unixfd => "unixfd",
            Transport::Shm => "shm",
            Transport::Container => "container",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "unixfd" => Some(Transport::Unixfd),
            "shm" => Some(Transport::Shm),
            "container" => Some(Transport::Container),
            _ => None,
        }
    }

    /// Whether this build can carry media on it. A pipe works everywhere; the
    /// two socket transports need `unixfdsrc` and `shmsrc`, which are Unix
    /// only, so Windows gets the container and an error that says so.
    pub const fn available_here(self) -> bool {
        match self {
            Transport::Container => true,
            Transport::Unixfd | Transport::Shm => cfg!(unix),
        }
    }

    /// The core's preference, highest first. Zero copy, then one copy, then
    /// the pipe that always works.
    pub const ORDER: [Transport; 3] = [Transport::Unixfd, Transport::Shm, Transport::Container];
}

impl std::fmt::Display for Transport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What the plugin sends first, before anything else.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Initialize {
    pub plugin: String,
    pub version: String,
    pub api: u32,
    #[serde(default)]
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

// ---------------------------------------------------------------------------
// Method bodies
// ---------------------------------------------------------------------------

/// Params of `start`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartParams {
    pub canvas: Canvas,
    pub transport: Transport,
    #[serde(default)]
    pub media: String,
}

/// Result of `start` and of `initialize`.
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
        Configure { applied: true, restart_required: None, reason: None }
    }

    pub fn restart_required(reason: impl Into<String>) -> Self {
        Configure {
            applied: false,
            restart_required: Some(true),
            reason: Some(reason.into()),
        }
    }
}

/// The three health states the `health` method answers with.
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
        Health { state: HealthState::Ok, detail: None, latency_ms: None }
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

impl Ready {
    /// A `Ready` good enough for an offline test or a template's own harness.
    pub fn for_test(canvas: Canvas) -> Self {
        Ready {
            core: "godwinmix".into(),
            version: "0.0.0".into(),
            api_level: crate::API_LEVEL,
            api_compatible: crate::API_COMPATIBLE,
            canvas,
            transport: Transport::Container,
            media: String::new(),
            instance: "test".into(),
            provide: "source".into(),
            params: Value::Object(Default::default()),
        }
    }
}

/// The result of `initialize` as a plugin returns it: its measured latency.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InitializeResult {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u32>,
}

/// Params of `seek`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Seek {
    pub position_ms: u64,
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

/// Params of `render`, for a transition provide.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Render {
    pub from: Vec<String>,
    pub to: Vec<String>,
    pub progress: f64,
    pub running_time_ns: u64,
}

/// Result of `position`, and of `seek`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub position_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}

/// One thing a `device` provide found.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candidate {
    /// A provide id such as `ndi/source`.
    #[serde(rename = "type")]
    pub kind: String,
    pub name: String,
    /// Ready for `source.add`.
    #[serde(default)]
    pub params: Value,
    #[serde(default)]
    pub confidence: f64,
}

/// Result of `discover`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Discovered {
    #[serde(default)]
    pub candidates: Vec<Candidate>,
}

/// What a plugin reports about the media it found, on `media.report`.
///
/// The browser sidecar's `[browser] media` log line, promoted to a
/// notification every plugin may send.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MediaReport {
    #[serde(default)]
    pub items: Vec<Value>,
    #[serde(flatten, default)]
    pub extra: serde_json::Map<String, Value>,
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
    pub const fn as_str(self) -> &'static str {
        match self {
            LogLevel::Trace => "trace",
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Warn => "warn",
            LogLevel::Error => "error",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "trace" => Some(LogLevel::Trace),
            "debug" => Some(LogLevel::Debug),
            "info" => Some(LogLevel::Info),
            "warn" | "warning" => Some(LogLevel::Warn),
            "error" => Some(LogLevel::Error),
            _ => None,
        }
    }
}

/// The lifecycle states of 03 section 7, plus the one the budget sampler
/// raises. One list for `event/plugin.state`, `plugin.list` and the harness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InstanceState {
    Starting,
    Ready,
    Running,
    Stalled,
    Degraded,
    Stopped,
    Failed,
    /// Over `max_rss_mb` or `max_cpu_percent`. Not in the state diagram: it is
    /// a report about a running instance, and what happens next is the
    /// `on_over_budget` policy.
    OverBudget,
}

impl InstanceState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Ready => "ready",
            Self::Running => "running",
            Self::Stalled => "stalled",
            Self::Degraded => "degraded",
            Self::Stopped => "stopped",
            Self::Failed => "failed",
            Self::OverBudget => "over-budget",
        }
    }

    /// What the core may call in this state (03 section 6, "legal call order").
    pub fn may_call(self, method: &str) -> bool {
        match self {
            Self::Starting => false,
            Self::Ready | Self::Stopped => {
                // `render` is here as well as in `running` because a
                // transition never starts: it has no media to start, so its
                // instance sits in `ready` for its whole life and is asked for
                // curves from there. A source that is asked to render before
                // it starts is asking for something that does not exist, and
                // gets `-32601` from the plugin rather than `-32001` from the
                // core, which is the right half of the contract to answer it.
                matches!(
                    method,
                    "configure" | "start" | "health" | "discover" | "render" | "tool.call" | "shutdown"
                )
            }
            Self::Running | Self::Stalled | Self::Degraded | Self::OverBudget => matches!(
                method,
                "configure"
                    | "stop"
                    | "health"
                    | "seek"
                    | "position"
                    | "keyframe"
                    | "audio.set"
                    | "render"
                    | "tool.call"
                    | "shutdown"
            ),
            Self::Failed => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_a_notification_and_a_response_are_told_apart() {
        let request = parse_line(r#"{"jsonrpc":"2.0","id":7,"method":"health","params":{}}"#);
        assert!(matches!(request, Frame::Request { ref method, .. } if method == "health"));
        let note = parse_line(r#"{"jsonrpc":"2.0","method":"log","params":{"level":"info"}}"#);
        assert!(matches!(note, Frame::Notification { ref method, .. } if method == "log"));
        let answer = parse_line(r#"{"jsonrpc":"2.0","id":0,"result":{"latency_ms":8}}"#);
        match answer {
            Frame::Response { result: Ok(v), .. } => assert_eq!(v["latency_ms"], 8),
            other => panic!("a result is a response, got {other:?}"),
        }
    }

    #[test]
    fn a_traceback_is_a_line_not_a_protocol_error() {
        let frame = parse_line("Traceback (most recent call last):");
        assert!(matches!(frame, Frame::NonJson(_)));
        // A JSON array is not an object either, and is not a batch we accept.
        assert!(matches!(parse_line("[1,2,3]"), Frame::NonJson(_)));
    }

    #[test]
    fn an_error_response_carries_its_code() {
        let frame = parse_line(
            r#"{"jsonrpc":"2.0","id":3,"error":{"code":-32012,"message":"needs a restart"}}"#,
        );
        match frame {
            Frame::Response { result: Err(e), .. } => {
                assert_eq!(e.code, ErrorCode::RestartRequired.number());
                assert!(e.message.contains("restart"));
            }
            other => panic!("an error is a response, got {other:?}"),
        }
    }

    #[test]
    fn the_documented_handshake_answer_round_trips() {
        let line = r#"{"core":"godwinmix","version":"0.2.0","api_level":1,"api_compatible":1,
            "canvas":{"width":1920,"height":1080,"fps":30},
            "transport":"unixfd","media":"/run/gmx/cam1.sock",
            "instance":"cam1","provide":"source","params":{"timezone":"Europe/London"}}"#;
        let ready: Ready = serde_json::from_str(line).expect("the documented shape parses");
        assert_eq!(ready.transport, Transport::Unixfd);
        assert_eq!(ready.canvas.height, 1080);
        assert_eq!(ready.params["timezone"], "Europe/London");
    }

    #[test]
    fn canvas_frame_sizes_round_the_chroma_planes_up() {
        let c = Canvas::new(1280, 720, 30);
        assert_eq!(c.i420_frame_bytes(), 1280 * 720 * 3 / 2);
        assert_eq!(c.ayuv_frame_bytes(), 1280 * 720 * 4);
        assert_eq!(c.frame_duration_ns(), 33_333_333);
        assert_eq!(Canvas::new(3, 3, 25).i420_frame_bytes(), 9 + 2 * 4);
    }

    #[test]
    fn the_call_order_table_refuses_start_on_a_running_instance() {
        assert!(InstanceState::Ready.may_call("start"));
        assert!(!InstanceState::Running.may_call("start"));
        assert!(InstanceState::Running.may_call("stop"));
        assert!(!InstanceState::Failed.may_call("health"));
        assert!(InstanceState::OverBudget.may_call("shutdown"));
    }
}
