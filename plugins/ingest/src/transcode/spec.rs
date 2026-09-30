//! What the core hands over to build, read out of the channel table.
//!
//! A channel row may carry `transcode`: one entry per live stream, each a
//! list of nodes in start order. A node is kept as the core described it, so
//! two descriptions compare equal exactly when the work is the same, and a
//! node whose description did not change is left running.

use serde_json::Value;

/// One node: decode, scale, encode, aconvert or aencode.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeSpec {
    pub id: String,
    pub kind: String,
    /// The node it reads from; empty for a decode, which reads the stream.
    pub input: String,
    /// Everything else the core said: elements, properties, sizes.
    pub raw: Value,
}

impl NodeSpec {
    pub fn text(&self, key: &str) -> Option<&str> {
        self.raw.get(key).and_then(Value::as_str).filter(|s| !s.is_empty())
    }

    pub fn number(&self, key: &str) -> u64 {
        self.raw.get(key).and_then(Value::as_u64).unwrap_or(0)
    }

    /// `[num, den]` as GStreamer's fraction.
    pub fn fps(&self) -> Option<(i32, i32)> {
        let pair = self.raw.get("fps")?.as_array()?;
        let num = i32::try_from(pair.first()?.as_u64()?).ok()?;
        let den = i32::try_from(pair.get(1)?.as_u64()?).ok()?;
        Some((num, den.max(1)))
    }

    /// The track a decode reads: `video` or `audio`.
    pub fn track(&self) -> &str {
        self.text("track").unwrap_or("video")
    }
}

/// What to build for one live stream.
#[derive(Debug, Clone, PartialEq)]
pub struct StreamSpec {
    pub app: String,
    pub stream: String,
    pub nodes: Vec<NodeSpec>,
}

/// Every stream the table asks to convert. Channels that are off are left
/// out, as their destinations are.
pub fn specs(params: &Value) -> Vec<StreamSpec> {
    let channels = params.get("channels").and_then(Value::as_array).cloned().unwrap_or_default();
    let text = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
    let mut out = Vec::new();
    for c in &channels {
        if !c.get("enabled").and_then(Value::as_bool).unwrap_or(true) {
            continue;
        }
        let app = Some(text(c, "app")).filter(|a| !a.is_empty()).unwrap_or_else(|| text(c, "id"));
        for s in c.get("transcode").and_then(Value::as_array).into_iter().flatten() {
            let nodes: Vec<NodeSpec> = s
                .get("nodes")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|n| {
                    let (id, kind) = (text(n, "id"), text(n, "kind"));
                    (!id.is_empty() && !kind.is_empty()).then(|| NodeSpec { id, kind, input: text(n, "input"), raw: n.clone() })
                })
                .collect();
            let stream = text(s, "stream");
            if !stream.is_empty() && !nodes.is_empty() {
                out.push(StreamSpec { app: app.clone(), stream, nodes });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_channel_row_s_transcode_becomes_one_spec_per_stream() {
        let params = json!({"channels": [
            {"id": "church", "app": "church", "enabled": true, "transcode": [
                {"stream": "main", "nodes": [
                    {"id": "decode:main:video", "kind": "decode", "track": "video", "element": "avdec_h264"},
                    {"id": "scale:main:1280x720p30", "kind": "scale", "input": "decode:main:video", "width": 1280, "height": 720, "fps": [30, 1]}
                ]}
            ]},
            {"id": "off", "enabled": false, "transcode": [{"stream": "x", "nodes": [{"id": "d", "kind": "decode"}]}]}
        ]});
        let s = specs(&params);
        assert_eq!(s.len(), 1);
        assert_eq!((s[0].app.as_str(), s[0].stream.as_str()), ("church", "main"));
        assert_eq!(s[0].nodes[1].input, "decode:main:video");
        assert_eq!(s[0].nodes[1].fps(), Some((30, 1)));
        assert_eq!(s[0].nodes[0].track(), "video");
    }
}
