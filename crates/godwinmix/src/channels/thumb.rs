//! A channel stream's picture: `channel.thumbnail`, and the JPEG route
//! `GET /api/v1/channels/{id}/streams/{stream}/thumbnail.jpg` built on it.
//!
//! The listener holds the stream, so the listener makes the picture: the
//! ingest plugin's vitals put a tap on the stream at the first ask, decode
//! its keyframes alone, about one a second, scale to 320 wide and keep the
//! newest JPEG. Each ask keeps that going for ten seconds; with no ask the
//! tap and its decode go. Nothing here runs on a timer, and a channel nobody
//! looks at is never decoded.

use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use godwinmix_protocol::channels::ChannelThumbnailRequest;
use godwinmix_protocol::error::RpcError;
use serde_json::{json, Value};

use super::{net, Channels, PLUGIN};

/// How long the listener is given to answer before the asker is told to try
/// again. A picture is a glance, not something to queue behind.
const WAIT: Duration = Duration::from_secs(3);

impl Channels {
    /// `channel.thumbnail`: `{channel, stream, jpeg, width, height, at_ms}`
    /// with the JPEG in base64, or `{pending, retry_after_ms}` while the
    /// first keyframe is on its way. Blocks on the listener; call it on the
    /// blocking pool.
    pub fn thumbnail(&self, req: &ChannelThumbnailRequest) -> Result<Value, RpcError> {
        let record = self.records.lock().iter().find(|r| r.id == req.id).cloned().ok_or_else(|| self.not_found(&req.id))?;
        let stream = self.live_stream(&record.id, req.stream.as_deref())?;
        if !self.plugins.is_running(PLUGIN) {
            return Err(RpcError::not_in_state(format!("channel {} has no picture: {}", record.id, net::why_not_listening(PLUGIN)))
                .with("channel", record.id.as_str()));
        }
        let args = json!({"name": "channel.thumbnail", "arguments": {"app": record.app, "stream": stream, "width": req.width.unwrap_or(320)}});
        let answer = self.plugins.call_provide(PLUGIN, "discover", "tool.call", args).map_err(|e| {
            RpcError::not_in_state(format!("channel {} has no picture yet: the listener said {e:#}. Ask again in a second.", record.id))
                .with("channel", record.id.as_str())
                .with("retry_after_ms", 1000)
        })?;
        read_answer(&record.id, &stream, answer)
    }

    /// The stream asked for when it is live, or the first live one.
    fn live_stream(&self, id: &str, asked: Option<&str>) -> Result<String, RpcError> {
        let live = self.live.lock();
        let on: Vec<&super::Live> = live.iter().filter(|l| l.channel == id && l.state == "live").collect();
        let found = match asked {
            Some(name) => on.iter().find(|l| l.name == name),
            None => on.first(),
        };
        if let Some(l) = found {
            return Ok(l.name.clone());
        }
        let what = asked.map_or_else(|| "no stream".to_string(), |s| format!("no stream called {s}"));
        let live_names: Vec<String> = on.iter().map(|l| l.name.clone()).collect();
        Err(RpcError::not_in_state(format!(
            "channel {id} has {what} live, so there is no picture. The picture comes once an encoder publishes to it; channel.get {id} has the address and the key."
        ))
        .with("channel", id)
        .with("state", "idle")
        .with("live", live_names))
    }
}

fn read_answer(id: &str, stream: &str, mut answer: Value) -> Result<Value, RpcError> {
    if answer["jpeg"].is_string() {
        answer["channel"] = json!(id);
        answer["stream"] = json!(stream);
        return Ok(answer);
    }
    if answer["status"] == 404 {
        let why = answer["why"].as_str().unwrap_or("nothing is publishing to it");
        return Err(RpcError::not_in_state(format!(
            "channel {id} has no picture of {stream}: {why}. The picture comes once an encoder publishes to it."
        ))
        .with("channel", id)
        .with("stream", stream)
        .with("state", "idle"));
    }
    Ok(json!({"channel": id, "stream": stream, "pending": true, "retry_after_ms": 1000}))
}

/// The JPEG itself, for an `<img>`: the work on the blocking pool, given
/// `WAIT` at most, and a picture still on its way said as a 409 with
/// `retry_after_ms` like any other state that passes.
pub async fn jpeg(channels: Arc<Channels>, req: ChannelThumbnailRequest) -> Result<Vec<u8>, RpcError> {
    let id = req.id.clone();
    let asked = tokio::task::spawn_blocking(move || channels.thumbnail(&req));
    let answer = match tokio::time::timeout(WAIT, asked).await {
        Ok(Ok(answer)) => answer?,
        _ => {
            return Err(RpcError::not_in_state(format!("channel {id} has no picture yet: the listener did not answer in time. Ask again in a second."))
                .with("channel", id.as_str())
                .with("retry_after_ms", 1000))
        }
    };
    let Some(b64) = answer["jpeg"].as_str() else {
        return Err(RpcError::not_in_state(format!("channel {id} has no picture yet: the first keyframe is on its way. Ask again in a second."))
            .with("channel", id.as_str())
            .with("retry_after_ms", 1000));
    };
    base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|_| RpcError::internal(format!("the listener sent a picture of channel {id} that would not decode")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_names_its_stream_and_a_refusal_says_what_brings_one() {
        let ok = read_answer("church", "main", json!({"jpeg": "/9j/", "width": 320, "height": 180, "at_ms": 1})).unwrap();
        assert_eq!((ok["channel"].as_str(), ok["stream"].as_str()), (Some("church"), Some("main")));
        let pending = read_answer("church", "main", json!({"pending": true})).unwrap();
        assert_eq!(pending["retry_after_ms"], 1000);
        let gone = read_answer("church", "main", json!({"status": 404, "why": "nothing is publishing to church/main"})).unwrap_err();
        assert!(gone.message.contains("comes once an encoder publishes"), "{}", gone.message);
        assert_eq!(gone.data["state"], "idle");
    }
}
