//! What a feed or a binding has to be before it is kept.
//!
//! Every refusal says what was wrong and what would be right, with the field
//! in `data` so a form can point at it.

use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::feeds::{BindingTarget, FeedFormat, FeedSpec, DEFAULT_INTERVAL_S, DEFAULT_TIMEOUT_S, MIN_INTERVAL_S};
use std::time::Duration;

/// How a feed is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Polled,
    WebSocket,
    Sse,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Polled => "polled",
            Kind::WebSocket => "websocket",
            Kind::Sse => "sse",
        }
    }
}

pub fn kind(spec: &FeedSpec) -> Kind {
    let lower = spec.address.to_ascii_lowercase();
    if lower.starts_with("ws://") || lower.starts_with("wss://") {
        Kind::WebSocket
    } else if spec.format == FeedFormat::Sse {
        Kind::Sse
    } else {
        Kind::Polled
    }
}

pub fn interval(spec: &FeedSpec) -> Duration {
    Duration::from_secs_f64(spec.interval_s.unwrap_or(DEFAULT_INTERVAL_S).max(MIN_INTERVAL_S))
}

pub fn timeout(spec: &FeedSpec) -> Duration {
    Duration::from_secs_f64(spec.timeout_s.unwrap_or(DEFAULT_TIMEOUT_S).clamp(1.0, 60.0))
}

/// A slug: lower case letters, digits and dashes, starting with a letter.
pub fn slug(kind: &str, id: &str) -> Result<(), RpcError> {
    let ok = id.len() <= 64
        && id.starts_with(|c: char| c.is_ascii_lowercase())
        && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if ok {
        return Ok(());
    }
    let suggest = crate::channels::keys::slug(id);
    Err(RpcError::invalid_params(format!(
        "'{id}' is not a {kind} id: an id is lower case letters, digits and dashes, starting with a letter. '{suggest}' would do."
    ))
    .with("field", "id")
    .with("suggest", suggest))
}

/// The address, the interval, the timeout and the headers.
pub fn feed(spec: &FeedSpec) -> Result<(), RpcError> {
    slug("feed", &spec.id)?;
    address(&spec.address, spec.format)?;
    if let Some(s) = spec.interval_s {
        if !(MIN_INTERVAL_S..=86_400.0).contains(&s) {
            return Err(field(
                "interval_s",
                format!("an interval of {s} s is refused: it is at least {MIN_INTERVAL_S} s, so no server is asked too often, and at most a day. Ask again with interval_s between 5 and 86400."),
            ));
        }
    }
    if let Some(s) = spec.timeout_s {
        if !(1.0..=60.0).contains(&s) {
            return Err(field("timeout_s", format!("a timeout of {s} s is refused. Give timeout_s between 1 and 60.")));
        }
    }
    for (name, value) in &spec.headers {
        let ok = reqwest::header::HeaderName::from_bytes(name.as_bytes()).is_ok() && reqwest::header::HeaderValue::from_str(value).is_ok();
        if !ok {
            return Err(field("headers", format!("the header '{name}' is not one HTTP can send: a name is letters, digits and dashes, and a value is one line of text.")));
        }
    }
    Ok(())
}

/// Only network addresses, and an event stream only over http(s).
pub fn address(address: &str, format: FeedFormat) -> Result<(), RpcError> {
    let parsed = reqwest::Url::parse(address).map_err(|e| {
        field("address", format!("'{address}' is not an address ({e}). Give a whole one, starting https://, http://, wss:// or ws://."))
    })?;
    let scheme = parsed.scheme();
    if !matches!(scheme, "http" | "https" | "ws" | "wss") {
        return Err(field(
            "address",
            format!("a feed is fetched over the network only, so a {scheme}: address is refused. Give an https://, http://, wss:// or ws:// address."),
        ));
    }
    if parsed.host_str().unwrap_or_default().is_empty() {
        return Err(field("address", format!("'{address}' names no server. Give one, like https://example.com/feed.json.")));
    }
    let socket = matches!(scheme, "ws" | "wss");
    if socket && matches!(format, FeedFormat::Sse | FeedFormat::Rss | FeedFormat::Csv) {
        return Err(field("format", "a websocket's messages are read as JSON or as text. Leave format as auto, or give json or text.".to_string()));
    }
    Ok(())
}

/// A binding's target, before anything is written to it.
pub fn target(to: &BindingTarget) -> Result<(), RpcError> {
    match to {
        BindingTarget::Source { source, path } => {
            if source.trim().is_empty() {
                return Err(field("to.source", "the binding names no source. Give the source's id in to.source.".into()));
            }
            param_keys(path).map(|_| ())
        }
        BindingTarget::Graphic { graphic, field: f, .. } if graphic.trim().is_empty() || f.trim().is_empty() => Err(field(
            "to.graphic",
            "a graphic target needs both graphic (like ograf/lower-third) and field (one of its schema's names, from scene.item.schema).".into(),
        )),
        BindingTarget::SceneParam { scene_param } if scene_param.trim().is_empty() => {
            Err(field("to.scene_param", "the binding names no scene parameter. Give the name used as {{name}} in the scenes.".into()))
        }
        _ => Ok(()),
    }
}

/// `params.fields.headline` into `["fields", "headline"]`.
pub fn param_keys(path: &str) -> Result<Vec<String>, RpcError> {
    let rest = path.trim().strip_prefix("params.").unwrap_or_default();
    let keys: Vec<String> = rest.split('.').map(str::trim).map(str::to_string).collect();
    if rest.is_empty() || keys.iter().any(String::is_empty) {
        return Err(field(
            "to.path",
            format!("'{path}' is not a param path: it starts params. and names the param, like params.text, params.items or params.fields.headline."),
        ));
    }
    Ok(keys)
}

pub fn field(name: &str, message: String) -> RpcError {
    RpcError::invalid_params(message).with("field", name)
}
