//! The destinations the core's channel table asks for.

use serde_json::Value;

/// Where a destination's tags come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Feed {
    /// The stream as it arrives, from the hub. What every destination was
    /// before renditions, and what one the plan copies still is.
    Copy,
    /// A converted pair from the renditions hub: the node its video comes
    /// from and the node its sound comes from. Both `None` while the plan
    /// has nothing for it yet (its stream is not live): it waits, and never
    /// sends the stream as it is in the meantime.
    Rendition { video: Option<String>, audio: Option<String> },
}

/// One destination as the core asked for it.
#[derive(Debug, Clone, PartialEq)]
pub struct Wanted {
    pub channel: String,
    pub app: String,
    pub id: String,
    pub platform: String,
    /// The whole address, key and all.
    pub url: String,
    /// A stream name, or `*` for the first live one.
    pub stream: String,
    pub feed: Feed,
}

impl Wanted {
    /// The name its tags are read under: the stream on the hub for a copy,
    /// the pair on the renditions hub for a rendition. Empty for a rendition
    /// with no pair yet, which nothing ever publishes.
    pub fn reads(&self) -> String {
        match &self.feed {
            Feed::Copy => self.stream.clone(),
            Feed::Rendition { video: None, audio: None } => String::new(),
            Feed::Rendition { video, audio } => crate::transcode::output_key(&self.stream, video.as_deref(), audio.as_deref()),
        }
    }
}

/// Every destination in the channel table, skipping any that is not well
/// formed. The core only hands over the ones that are on.
pub fn wanted(params: &Value) -> Vec<Wanted> {
    let channels = params.get("channels").and_then(Value::as_array).cloned().unwrap_or_default();
    let text = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
    let mut out = Vec::new();
    for c in &channels {
        let (channel, app) = (text(c, "id"), text(c, "app"));
        let enabled = c.get("enabled").and_then(Value::as_bool).unwrap_or(true);
        let app = if app.is_empty() { channel.clone() } else { app.clone() };
        for d in c.get("destinations").and_then(Value::as_array).into_iter().flatten() {
            if let Some(w) = destination(&channel, &app, d).filter(|_| enabled) {
                out.push(w);
            }
        }
    }
    out
}

/// One destination row, read as a destination of `channel` reading `app`.
/// `None` when it is switched off or is missing its id or its address. The
/// direct host reads its shows' outputs with this, since each is a row of
/// exactly this shape.
pub fn destination(channel: &str, app: &str, d: &Value) -> Option<Wanted> {
    let text = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
    let some = |v: &Value, k: &str| Some(text(v, k)).filter(|s| !s.is_empty());
    let rendition = d.get("rendition").and_then(Value::as_bool).unwrap_or(false);
    let w = Wanted {
        channel: channel.to_string(),
        app: app.to_string(),
        id: text(d, "id"),
        platform: text(d, "platform"),
        url: text(d, "url"),
        stream: some(d, "stream").unwrap_or_else(|| "*".into()),
        feed: if rendition { Feed::Rendition { video: some(d, "video"), audio: some(d, "audio") } } else { Feed::Copy },
    };
    let on = d.get("enabled").and_then(Value::as_bool).unwrap_or(true);
    (on && !w.channel.is_empty() && !w.id.is_empty() && !w.url.is_empty()).then_some(w)
}
