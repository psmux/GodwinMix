//! The destinations the core's channel table asks for.

use serde_json::Value;

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
        for d in c.get("destinations").and_then(Value::as_array).into_iter().flatten() {
            let w = Wanted {
                channel: channel.clone(),
                app: if app.is_empty() { channel.clone() } else { app.clone() },
                id: text(d, "id"),
                platform: text(d, "platform"),
                url: text(d, "url"),
                stream: Some(text(d, "stream")).filter(|s| !s.is_empty()).unwrap_or_else(|| "*".into()),
            };
            let on = d.get("enabled").and_then(Value::as_bool).unwrap_or(true);
            if enabled && on && !w.channel.is_empty() && !w.id.is_empty() && !w.url.is_empty() {
                out.push(w);
            }
        }
    }
    out
}
