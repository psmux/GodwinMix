//! Live data on the wire: feeds the mixer fetches and the bindings that carry
//! a value from one into a source's params, a graphic's field or a scene
//! parameter. `feed.*` answers with these.
//!
//! A feed is fetched only by the mixer and only from an address an operator
//! gave it. Header values are sealed in the secret store and never come back:
//! a read answers the sentinel `"__secret__"` in their place, and writing the
//! sentinel back leaves the stored value alone.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// What a header value reads as once it is stored.
pub const SECRET_SENTINEL: &str = "__secret__";
/// The shortest gap between two fetches of a polled feed, in seconds.
pub const MIN_INTERVAL_S: f64 = 5.0;
/// The interval a polled feed gets when none is given.
pub const DEFAULT_INTERVAL_S: f64 = 30.0;
/// How long one fetch may take when no timeout is given.
pub const DEFAULT_TIMEOUT_S: f64 = 10.0;
/// The most one response, message or event may carry.
pub const MAX_BYTES: usize = 4 * 1024 * 1024;

/// How the body is read. `auto` decides from the content type and the first
/// byte; `sse` reads an `http(s)` address as a Server-Sent Events stream.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum FeedFormat {
    #[default]
    Auto,
    Json,
    /// RSS 2.0 or Atom, read as `{title, link, items: [{title, link, summary, published, id, author}]}`.
    Rss,
    /// Comma separated with a header row, read as `{columns, rows: [{column: value}]}`.
    Csv,
    /// Plain text, read as `{text, lines}`.
    Text,
    /// Server-Sent Events: each event's `data` is one document.
    Sse,
}

/// One feed as an operator describes it, and as `feed.add` takes it.
///
/// Unknown fields are refused by `feed.add`, which reads the same fields
/// through [`FeedAddRequest`]; this shape is also flattened into
/// [`FeedStatus`], where refusing them would refuse the status itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FeedSpec {
    /// A slug: lower case letters, digits and dashes. Never changes.
    pub id: String,
    /// `http://`, `https://`, `ws://` or `wss://`. Nothing else is fetched.
    pub address: String,
    #[serde(default)]
    pub format: FeedFormat,
    /// Seconds between fetches of a polled feed. At least 5; 30 when absent.
    /// Not used by a websocket or an event stream, which push.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interval_s: Option<f64>,
    /// Seconds one fetch may take before it counts as failed. 10 when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_s: Option<f64>,
    /// Sent with every request, for an API key. Sealed once stored: read
    /// back as `"__secret__"`, and `"__secret__"` written back keeps it.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
    /// Stopped by `feed.pause`: nothing is fetched and nothing is written.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub paused: bool,
}

/// Where a binding writes. One of three shapes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum BindingTarget {
    /// A source's param through `source.set`, by a dotted path that starts
    /// at `params`: `params.text`, `params.items`, `params.fields.headline`.
    Source { source: String, path: String },
    /// A field of an OGraf graphic on a scene, through `scene.apply_graphic`
    /// with no play, which is the graphic's update action.
    Graphic {
        graphic: String,
        field: String,
        /// The item's name, when the graphic is on the canvas twice.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        item: Option<String>,
    },
    /// A scene parameter through `scene.params.set`, so every `{{name}}`
    /// in the collection follows it.
    SceneParam { scene_param: String },
}

/// What a binding picks out of a feed and how it turns it into a value.
///
/// The same four fields are on [`BindingSpec`] and [`FeedTestRequest`];
/// `selection()` on each gathers them.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Selection {
    /// A path into the fetched document: `items[].title`, `data.home.score`,
    /// `rows[0].Name`, or a JSON pointer starting `/`. Empty is the whole
    /// document. `[]` takes every element of a list.
    #[serde(default)]
    pub select: String,
    /// Words with `{path}` holes filled from what `select` picked (from each
    /// element, for a list): `{home} {home_score} : {away_score} {away}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    /// For a list: keep the first this many.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// For a list: join it into one string with this between the elements.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub join: Option<String>,
}

/// One binding as an operator describes it, and as `feed.binding.add` takes it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BindingSpec {
    /// A slug. Never changes.
    pub id: String,
    /// The feed it reads.
    pub feed: String,
    /// A path into the fetched document: `items[].title`, `data.home.score`,
    /// `rows[0].Name`, or a JSON pointer starting `/`. Empty is the whole
    /// document. `[]` takes every element of a list.
    #[serde(default)]
    pub select: String,
    /// Words with `{path}` holes filled from what `select` picked (from each
    /// element, for a list): `{home} {home_score} : {away_score} {away}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    /// For a list: keep the first this many.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// For a list: join it into one string with this between the elements.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub join: Option<String>,
    pub to: BindingTarget,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub paused: bool,
}

impl BindingSpec {
    pub fn selection(&self) -> Selection {
        Selection {
            select: self.select.clone(),
            template: self.template.clone(),
            limit: self.limit,
            join: self.join.clone(),
        }
    }
}

/// `feed.add`: the fields of [`FeedSpec`], with anything else refused.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FeedAddRequest {
    /// A slug: lower case letters, digits and dashes. Never changes.
    pub id: String,
    /// `http://`, `https://`, `ws://` or `wss://`. Nothing else is fetched.
    pub address: String,
    #[serde(default)]
    pub format: FeedFormat,
    /// Seconds between fetches of a polled feed. At least 5; 30 when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interval_s: Option<f64>,
    /// Seconds one fetch may take. 10 when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_s: Option<f64>,
    /// Sent with every request, for an API key. Sealed once stored.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub paused: bool,
}

impl From<FeedAddRequest> for FeedSpec {
    fn from(r: FeedAddRequest) -> Self {
        FeedSpec {
            id: r.id,
            address: r.address,
            format: r.format,
            interval_s: r.interval_s,
            timeout_s: r.timeout_s,
            headers: r.headers,
            paused: r.paused,
        }
    }
}

/// `feed.binding.add`: the fields of [`BindingSpec`], with anything else refused.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BindingAddRequest {
    /// A slug. Never changes. Made from the target when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// The feed it reads.
    pub feed: String,
    /// A path into the fetched document. See [`BindingSpec`].
    #[serde(default)]
    pub select: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub join: Option<String>,
    pub to: BindingTarget,
    #[serde(default)]
    pub paused: bool,
}

/// Where a feed is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum FeedState {
    /// Not fetched yet, or connecting.
    Starting,
    /// The last fetch, message or event was read.
    Ok,
    /// The last attempt failed; it is tried again with a growing gap.
    Failing,
    Paused,
}

/// A feed with what it has been doing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FeedStatus {
    #[serde(flatten)]
    pub spec: FeedSpec,
    pub state: FeedState,
    /// `polled`, `websocket` or `sse`.
    pub kind: String,
    /// When something was last read, RFC 3339.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_fetch: Option<String>,
    /// When what was read last differed from what came before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_change: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    /// Attempts in a row that failed.
    pub failures: u32,
    /// Fetches or messages read since the core started.
    pub fetches: u64,
    /// Fetches the server answered `304 Not Modified`.
    pub not_modified: u64,
    /// Size of the last body read, in bytes.
    pub bytes: u64,
}

/// A binding with what it last wrote.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BindingStatus {
    #[serde(flatten)]
    pub spec: BindingSpec,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_write: Option<String>,
    /// Writes since the core started. A feed that has not changed adds none.
    pub writes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

/// `feed.list`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FeedList {
    pub feeds: Vec<FeedStatus>,
    pub bindings: Vec<BindingStatus>,
}

/// `feed.set`: only what is named changes.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FeedSetRequest {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<FeedFormat>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interval_s: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_s: Option<f64>,
    /// Replaces the headers. A value of `"__secret__"` keeps what is stored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub headers: Option<BTreeMap<String, String>>,
}

/// `feed.binding.set`: only what is named changes.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BindingSetRequest {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub select: Option<String>,
    /// An empty string takes the template away.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    /// 0 takes the limit away.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// An empty string takes the join away.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub join: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<BindingTarget>,
}

/// `feed.pause` and `feed.binding.pause`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PauseRequest {
    pub id: String,
    /// False starts it again. True when absent.
    #[serde(default = "yes")]
    pub paused: bool,
}

fn yes() -> bool {
    true
}

/// `feed.remove` and `feed.binding.remove`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FeedIdRequest {
    pub id: String,
}

/// `feed.test`: fetch once and show what a selection picks.
///
/// Give `id` for a feed that exists, or `address` (with `format`, `headers`
/// and `timeout_s` if it needs them) for one that does not yet. Nothing is
/// stored and nothing is written.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FeedTestRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<FeedFormat>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_s: Option<f64>,
    /// A path to try. See [`BindingSpec`].
    #[serde(default)]
    pub select: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub join: Option<String>,
    /// Fetch again even when the feed has a document already.
    #[serde(default)]
    pub fresh: bool,
}

impl FeedTestRequest {
    pub fn selection(&self) -> Selection {
        Selection {
            select: self.select.clone(),
            template: self.template.clone(),
            limit: self.limit,
            join: self.join.clone(),
        }
    }
}

/// What `feed.test` found.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FeedTestResult {
    /// The format it was read as.
    pub format: FeedFormat,
    pub bytes: u64,
    pub took_ms: u64,
    /// The keys at the top of the document.
    pub keys: Vec<String>,
    /// The document, with long lists cut to their first few elements and
    /// long strings shortened, for a person or an agent to read paths from.
    pub preview: serde_json::Value,
    /// Every path in the document down to a few levels, with `[]` for a
    /// list, each with an example value: the paths `select` takes.
    pub paths: Vec<PathExample>,
    /// What `select` picked, when one was given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected: Option<serde_json::Value>,
    /// What a binding with this selection would write.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
}

/// One path `select` would take, with what it picks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PathExample {
    pub path: String,
    pub example: serde_json::Value,
}

/// `event/feed.failed` and `event/feed.recovered`.
pub fn events() -> Vec<crate::protocol::EventDef> {
    vec![
        crate::protocol::EventDef {
            name: "feed.failed",
            since: "1",
            summary: "A feed could not be read (a refused connection, a timeout, a body that \
                      would not parse, a response over 4 MB), or one of its bindings could \
                      not write what it read. `binding` names the binding when it was the \
                      write. Sent on the first failure in a row, not on every retry; what \
                      was last written stays on air.",
            ext: None,
            legacy: None,
            payload: |_| failure_schema(true),
        },
        crate::protocol::EventDef {
            name: "feed.recovered",
            since: "1",
            summary: "A feed or a binding that was failing works again. `failures` is how \
                      many attempts in a row failed before this one.",
            ext: None,
            legacy: None,
            payload: |_| failure_schema(false),
        },
    ]
}

fn failure_schema(with_error: bool) -> serde_json::Value {
    let mut props = serde_json::json!({
        "id": { "type": "string", "description": "The feed." },
        "binding": { "type": ["string", "null"], "description": "The binding, when it was a write that failed." },
        "failures": { "type": "integer", "minimum": 0 }
    });
    let mut required = vec!["id", "failures"];
    if with_error {
        props["error"] = serde_json::json!({ "type": "string", "description": "What went wrong and what to do about it. Never carries a header value." });
        required.push("error");
    }
    serde_json::json!({ "type": "object", "properties": props, "required": required })
}
