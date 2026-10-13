//! What gives a channel its picture: this mixer's programme, an encoder
//! that publishes to it, or a stream it pulls from a URL. A channel then
//! sends that picture on to its destinations.
//!
//! The pull address may carry a key, so it is write only, like a
//! destination's: a channel shows only `scheme://host:port` of it.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The three ways a channel is fed, as `channel.add` and `channel.set`
/// take them: `{"kind": "programme"}`, `{"kind": "encoder"}` or
/// `{"kind": "pull", "url": "rtmp://..."}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum FeedRequest {
    /// This mixer's programme. Under a station, `show` names whose; the
    /// first show when it is left out.
    Programme {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        show: Option<String>,
    },
    /// OBS, a phone or a camera publishes to the channel's address with one
    /// of its keys. What every channel did before feeds existed.
    Encoder,
    /// The channel pulls this stream and takes it again by itself whenever
    /// it drops: `rtmp://`, `rtmps://`, `srt://`, `rtsp://`, or an
    /// `http(s)://` HLS playlist. Never read back.
    Pull { url: String },
}

impl FeedRequest {
    pub fn kind(&self) -> FeedKind {
        match self {
            FeedRequest::Programme { .. } => FeedKind::Programme,
            FeedRequest::Encoder => FeedKind::Encoder,
            FeedRequest::Pull { .. } => FeedKind::Pull,
        }
    }
}

/// Which of the three a channel has.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum FeedKind {
    Programme,
    #[default]
    Encoder,
    Pull,
}

/// How the feed is doing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum FeedState {
    /// A picture is arriving.
    Live,
    /// Nothing is arriving yet: no encoder has published, or the programme
    /// has not reached the channel yet.
    #[default]
    Waiting,
    /// Opening the pull address, or waiting for its first frame.
    Connecting,
    /// The pull dropped or failed and is tried again by itself; `error`
    /// says why and `retry_in_ms` when.
    Reconnecting,
    /// The channel is switched off.
    Off,
}

/// A channel's feed as a list shows it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ChannelFeed {
    pub kind: FeedKind,
    /// The show whose programme feeds it, for `programme`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show: Option<String>,
    /// The output in that show that publishes the programme here, for
    /// `programme`: `to-channel-<channel id>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    /// Scheme, host and port of the pull address, for `pull`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uri_host: Option<String>,
    pub state: FeedState,
    /// What went wrong last, in words, while it is not live.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// When a pull that dropped is tried next, in milliseconds from now.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_in_ms: Option<u64>,
}

/// The schemes a pull takes.
pub const PULL_SCHEMES: &[&str] = &["rtmp", "rtmps", "srt", "rtsp", "rtsps", "http", "https"];

/// A pull address checked: a scheme it takes and a host. The error never
/// repeats the address, which may hold a key.
pub fn check_pull_url(url: &str) -> Result<String, String> {
    let url = url.trim();
    let Some((scheme, rest)) = url.split_once("://") else {
        return Err("a pull address starts with its scheme, such as rtmp://, srt://, rtsp:// or https:// for HLS".into());
    };
    let scheme = scheme.to_ascii_lowercase();
    if !PULL_SCHEMES.contains(&scheme.as_str()) {
        return Err(format!(
            "a channel cannot pull {scheme}://. It pulls rtmp://, rtmps://, srt://, rtsp:// and http(s):// HLS playlists"
        ));
    }
    let end = rest.find(['/', '?']).unwrap_or(rest.len());
    let host = rest[..end].rsplit('@').next().unwrap_or_default();
    if host.is_empty() || host.starts_with(':') {
        return Err(format!("the pull address has no host after {scheme}://. Write it as {scheme}://host:port/..."));
    }
    Ok(url.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_three_kinds_read_as_the_reference_writes_them() {
        let p: FeedRequest = serde_json::from_value(json!({"kind": "programme"})).unwrap();
        assert_eq!(p, FeedRequest::Programme { show: None });
        let e: FeedRequest = serde_json::from_value(json!({"kind": "encoder"})).unwrap();
        assert_eq!(e.kind(), FeedKind::Encoder);
        let u: FeedRequest = serde_json::from_value(json!({"kind": "pull", "url": "rtmp://a/b/c"})).unwrap();
        assert_eq!(u, FeedRequest::Pull { url: "rtmp://a/b/c".into() });
        assert!(serde_json::from_value::<FeedRequest>(json!({"kind": "pull"})).is_err(), "a pull needs its url");
        assert_eq!(serde_json::to_value(ChannelFeed::default()).unwrap(), json!({"kind": "encoder", "state": "waiting"}));
    }

    #[test]
    fn a_pull_address_needs_a_scheme_it_takes_and_a_host_and_is_never_repeated() {
        assert!(check_pull_url(" srt://cam.example:9000?streamid=x ").is_ok());
        assert!(check_pull_url("https://cdn.example/live/index.m3u8").is_ok());
        let no = check_pull_url("udp://239.0.0.1:5000").unwrap_err();
        assert!(no.contains("udp://") && no.contains("rtmp://"), "{no}");
        let no = check_pull_url("rtmp:///app/secret-key").unwrap_err();
        assert!(!no.contains("secret-key"), "{no}");
        assert!(check_pull_url("example.com/live").is_err());
    }
}
