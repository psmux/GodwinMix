//! A stream on a channel, as the core keeps it between the listener's
//! events.

use godwinmix_protocol::channels::{ChannelStream, StreamAudio, StreamVideo};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct Live {
    pub channel: String,
    pub app: String,
    pub name: String,
    /// `live` or `idle`.
    pub state: String,
    pub since_ms: u64,
    pub from: String,
    pub key: Option<String>,
    /// `rtmp`, `rtmps`, `srt` or `whip`.
    pub protocol: Option<String>,
    pub video: Option<StreamVideo>,
    pub audio: Option<StreamAudio>,
    pub dropped_gops: u64,
    pub source: Option<String>,
    /// Where a source reads it: the listener's own port on 127.0.0.1.
    pub relay: String,
}

fn text(v: &Value, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or_default().to_string()
}

fn number(v: &Value, key: &str) -> u64 {
    v.get(key).and_then(Value::as_u64).unwrap_or(0)
}

impl Live {
    /// From a `channel.stream` event with `state: "live"`, or a row of the
    /// `streams` tool.
    pub fn from_plugin(v: &Value) -> Option<Live> {
        let name = text(v, "stream");
        let app = text(v, "app");
        if name.is_empty() || app.is_empty() {
            return None;
        }
        let mut live = Live {
            channel: text(v, "channel"),
            app,
            name,
            state: "live".into(),
            since_ms: 0,
            from: String::new(),
            key: None,
            protocol: None,
            video: None,
            audio: None,
            dropped_gops: 0,
            source: None,
            relay: text(v, "relay"),
        };
        live.absorb(v);
        Some(live)
    }

    /// Take what the plugin now says about the stream, keeping what only the
    /// core knows (its source).
    pub fn absorb(&mut self, v: &Value) {
        self.state = "live".into();
        self.since_ms = number(v, "since_ms").max(self.since_ms);
        let from = text(v, "from");
        if !from.is_empty() {
            self.from = from;
        }
        if let Some(key) = v.get("key").and_then(Value::as_str) {
            self.key = Some(key.to_string());
        }
        if let Some(via) = v.get("protocol").and_then(Value::as_str) {
            self.protocol = Some(via.to_string());
        }
        let relay = text(v, "relay");
        if !relay.is_empty() {
            self.relay = relay;
        }
        self.video = video(&v["video"]).or(self.video.take());
        self.audio = audio(&v["audio"]).or(self.audio.take());
        self.dropped_gops = number(v, "dropped_gops");
    }

    pub fn view(&self) -> ChannelStream {
        ChannelStream {
            name: self.name.clone(),
            state: self.state.clone(),
            since_ms: self.since_ms,
            from: self.from.clone(),
            key: self.key.clone(),
            protocol: self.protocol.clone(),
            video: self.video.clone(),
            audio: self.audio.clone(),
            source: self.source.clone(),
            dropped_gops: self.dropped_gops,
        }
    }
}

fn video(v: &Value) -> Option<StreamVideo> {
    v.as_object()?;
    Some(StreamVideo {
        codec: text(v, "codec"),
        width: number(v, "width") as u32,
        height: number(v, "height") as u32,
        fps: v.get("fps").and_then(Value::as_f64).unwrap_or(0.0),
        kbps: number(v, "kbps") as u32,
    })
}

fn audio(v: &Value) -> Option<StreamAudio> {
    v.as_object()?;
    Some(StreamAudio {
        codec: text(v, "codec"),
        channels: number(v, "channels") as u32,
        sample_rate: number(v, "sample_rate") as u32,
        kbps: number(v, "kbps") as u32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_live_event_becomes_a_stream_with_its_codecs() {
        let live = Live::from_plugin(&json!({
            "channel": "church", "app": "church", "stream": "main", "state": "live",
            "since_ms": 1_700_000_000_000u64, "from": "10.0.0.9:5000", "key": "obs",
            "relay": "127.0.0.1:1935",
            "video": {"codec": "h264", "width": 1920, "height": 1080, "fps": 29.97, "kbps": 5980},
            "audio": {"codec": "aac", "channels": 2, "sample_rate": 48000, "kbps": 128},
            "dropped_gops": 0
        }))
        .expect("a stream");
        let view = live.view();
        assert_eq!(view.name, "main");
        assert_eq!(view.video.as_ref().map(|v| v.width), Some(1920));
        assert_eq!(view.audio.as_ref().map(|a| a.sample_rate), Some(48000));
        assert_eq!(view.key.as_deref(), Some("obs"));
        assert_eq!(live.relay, "127.0.0.1:1935");
    }

    #[test]
    fn a_row_without_codecs_keeps_the_ones_already_known() {
        let mut live = Live::from_plugin(&json!({
            "app": "church", "stream": "main",
            "video": {"codec": "h264", "width": 640, "height": 360, "fps": 30.0, "kbps": 800}
        }))
        .unwrap();
        live.absorb(&json!({"app": "church", "stream": "main", "video": null}));
        assert_eq!(live.video.map(|v| v.width), Some(640));
    }
}
