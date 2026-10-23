//! A channel's destinations: where a published stream is sent on to, as it
//! arrives, by remuxing. The wire shapes, the stored shape and the platform
//! table. `dev/plans/channels-contract.md` is the contract these follow.

mod plan;
mod platforms;
mod requests;

pub use plan::*;
pub use platforms::*;
pub use requests::*;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::rendition::RenditionChoice;

/// Where a destination has got to.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum DestinationState {
    /// Switched off by the person.
    #[default]
    Off,
    /// Switched on, and waiting for the stream it sends to go live.
    Waiting,
    /// Dialling the far end for the first time.
    Connecting,
    /// Sending.
    Live,
    /// Lost the far end and dialling it again on the backoff.
    Reconnecting,
    /// The far end said no in a way retrying will not fix: the key was
    /// refused. `error` says what to change.
    Failed,
}

/// One destination as a client sees it. The key never appears: `has_key`
/// says whether there is one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Destination {
    /// A slug, unique within its channel: `youtube`, `youtube-2`.
    pub id: String,
    /// A platform id from the table: youtube, facebook, twitch, custom, srt,
    /// or the two that stay on this machine, file and hls.
    pub platform: String,
    pub label: String,
    /// The scheme, host and port, and nothing that could carry a key.
    pub uri_host: String,
    /// The server as it was given, without its query or a user and password,
    /// for an edit form to show. Only for a destination whose key is kept
    /// apart from the address; left out where the key may be the address's
    /// own path, as on a whole address pasted with no key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
    pub has_key: bool,
    /// The key's last four characters, as a channel key's `hint`, so a person
    /// can see which key is saved. Left out for a key under eight characters,
    /// where four would be half of it. Never the key itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_hint: Option<String>,
    /// Which of the channel's streams to send. `*` is the first live one.
    pub stream: String,
    pub enabled: bool,
    #[serde(flatten)]
    pub live: DestinationLive,
    /// What it asked to be converted to. Absent: sent as it arrives.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rendition: Option<RenditionChoice>,
    /// What the plan gave it, while its stream is live.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<DestinationPlan>,
    /// Why it is not sending what it asked for, and what would fit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refused: Option<DestinationRefusal>,
    /// Where a player opens it, for an output this machine serves as HLS.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub playback: Option<Playback>,
    /// What an `hls://` output of a show without compositing was given, as
    /// `show.output.set` takes it, less the viewer key. Read while it runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<crate::shows::HlsOutputParams>,
}

/// The links of an output served as HLS from the control port, each with
/// the output's viewer key on it. A channel's watch link is one too:
/// `/hls/channel/<channel>/<destination>/index.m3u8?key=...`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Playback {
    /// `/hls/viewers/master.m3u8?show=bbc-one&key=...`.
    pub master_url_path: String,
    /// The same segments as a DASH MPD.
    pub dash_url_path: String,
    /// Players that fetched something in the last two windows.
    pub viewers: u32,
}

/// What a running destination reports. The restreamer fills it in.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DestinationLive {
    pub state: DestinationState,
    /// Milliseconds since `state` last changed.
    pub since_ms: u64,
    /// What is going out, in kbit/s, sound and container included. A
    /// channel's destination averages it over the last four seconds, two
    /// keyframe intervals at 2 s; others over the last second.
    pub kbps: u32,
    /// Connections lost and made again since it was switched on.
    pub reconnects: u32,
    /// What went wrong last, in words a person can act on.
    pub error: Option<String>,
    /// The file a recording destination is writing, or wrote last.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<RecordingFile>,
}

/// The file a `file` destination writes: one per time the stream goes live.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RecordingFile {
    /// `sunday-service-main-20261009-103000.ts`.
    pub name: String,
    /// The whole path on the machine running the mixer.
    pub path: String,
    /// Bytes written so far.
    pub bytes: u64,
    /// How long it has been recording, or ran for once it has closed.
    pub duration_ms: u64,
    /// Whether it is still being written.
    pub open: bool,
}

/// A destination as it is kept, key and all. Never sent to a client; the
/// store persists this and [`StoredDestination::view`] is what goes out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoredDestination {
    pub id: String,
    pub platform: String,
    pub label: String,
    /// The ingest address. The platform's own when it has one and nobody gave
    /// another.
    pub server: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    pub stream: String,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rendition: Option<RenditionChoice>,
}

impl StoredDestination {
    /// The whole address the restreamer dials, key and all.
    pub fn url(&self) -> String {
        join_key(&self.server, self.key.as_deref().unwrap_or(""))
    }

    /// How hard to retry: the platform's, or `own` for one not on the table.
    pub fn policy(&self) -> RetryPolicy {
        platform(&self.platform).map(|p| p.policy).unwrap_or(RetryPolicy::Own)
    }

    /// Whether there is a key, or nothing that needs one.
    pub fn has_key(&self) -> bool {
        let keyed = self.key.as_deref().is_some_and(|k| {
            !k.trim().is_empty() && crate::types::uri_has_key(k)
        });
        match platform(&self.platform).map(|p| p.key) {
            Some(KeyRule::Required) => keyed,
            _ => keyed || crate::types::uri_has_key(&self.server),
        }
    }

    /// The key kept apart from the address, when there is one.
    fn separate_key(&self) -> Option<&str> {
        self.key.as_deref().map(str::trim).filter(|k| !k.is_empty())
    }

    /// The server for an edit form: see [`Destination::server`].
    pub fn server_shown(&self) -> Option<String> {
        self.separate_key()?;
        let (scheme, rest) = self.server.trim().split_once("://")?;
        let rest = rest.split(['?', '#']).next().unwrap_or(rest);
        let (authority, path) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
        let host = authority.rsplit('@').next().unwrap_or(authority);
        Some(format!("{}://{host}{path}", scheme.to_lowercase()))
    }

    /// The key's last four characters: see [`Destination::key_hint`].
    pub fn key_hint(&self) -> Option<String> {
        let key: Vec<char> = self.separate_key()?.chars().collect();
        (key.len() >= 8).then(|| key[key.len() - 4..].iter().collect())
    }

    /// The client's view, with what the restreamer last reported.
    pub fn view(&self, live: DestinationLive) -> Destination {
        Destination {
            id: self.id.clone(),
            platform: self.platform.clone(),
            label: self.label.clone(),
            uri_host: uri_host(&self.server),
            server: self.server_shown(),
            has_key: self.has_key(),
            key_hint: self.key_hint(),
            stream: self.stream.clone(),
            enabled: self.enabled,
            live,
            rendition: self.rendition.clone(),
            plan: None,
            refused: None,
            playback: None,
            params: None,
        }
    }
}

/// `rtmps://live-api-s.facebook.com:443`, from anything with a scheme. The
/// path and the query are dropped because either can carry a key: an SRT
/// address takes its passphrase in the query.
pub fn uri_host(uri: &str) -> String {
    let uri = uri.trim();
    match uri.split_once("://") {
        Some((scheme, rest)) => {
            let end = rest.find(['/', '?']).unwrap_or(rest.len());
            let hostport = &rest[..end];
            let host = hostport.rsplit('@').next().unwrap_or(hostport);
            format!("{}://{host}", scheme.to_lowercase())
        }
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stored(platform: &str, server: &str, key: Option<&str>) -> StoredDestination {
        StoredDestination {
            id: platform.into(),
            platform: platform.into(),
            label: platform.into(),
            server: server.into(),
            key: key.map(Into::into),
            stream: "*".into(),
            enabled: true,
            rendition: None,
        }
    }

    #[test]
    fn the_view_never_carries_the_key_or_the_query() {
        let d = stored("srt", "srt://10.0.0.9:9000?passphrase=hunter2hunter2", None);
        let json = serde_json::to_string(&d.view(DestinationLive::default())).unwrap();
        assert!(!json.contains("hunter2"), "{json}");
        assert!(json.contains("\"uri_host\":\"srt://10.0.0.9:9000\""), "{json}");
        let yt = stored("youtube", "rtmp://a.rtmp.youtube.com/live2", Some("abcd-1234"));
        let json = serde_json::to_string(&yt.view(DestinationLive::default())).unwrap();
        assert!(!json.contains("abcd"), "{json}");
        assert!(json.contains("\"state\":\"off\""), "{json}");
    }

    #[test]
    fn the_view_shows_the_server_and_the_key_hint_only_when_the_key_is_apart() {
        let d = stored("custom", "rtmp://bob:pw@streaming.livebox.co.in:1935/live?token=t0k3n", Some("live-key-abcd"));
        let v = d.view(DestinationLive::default());
        assert_eq!(v.server.as_deref(), Some("rtmp://streaming.livebox.co.in:1935/live"));
        assert_eq!(v.key_hint.as_deref(), Some("abcd"));
        let json = serde_json::to_string(&v).unwrap();
        assert!(!json.contains("live-key") && !json.contains("t0k3n") && !json.contains("pw@"), "{json}");
        let whole = stored("custom", "rtmp://host/app/the-stream-key", None).view(DestinationLive::default());
        assert_eq!((whole.server, whole.key_hint), (None, None), "a whole address may end in its key");
        let short = stored("custom", "rtmp://host/app", Some("abc12")).view(DestinationLive::default());
        assert_eq!(short.key_hint, None, "four of five characters would be most of the key");
        assert_eq!(short.server.as_deref(), Some("rtmp://host/app"));
    }

    #[test]
    fn a_platform_that_needs_a_key_has_none_until_one_is_given() {
        let yt = stored("youtube", "rtmp://a.rtmp.youtube.com/live2", None);
        assert!(!yt.has_key());
        assert!(stored("youtube", "rtmp://a/live2", Some("real")).has_key());
        assert!(!stored("youtube", "rtmp://a/live2", Some("YOUR-STREAM-KEY")).has_key());
        // A whole address pasted into a custom server is a whole address.
        assert!(stored("custom", "rtmp://host/app/stream", None).has_key());
        assert!(stored("srt", "srt://host:9000", None).has_key());
    }

    #[test]
    fn the_url_is_the_server_and_the_key() {
        let yt = stored("youtube", "rtmp://a.rtmp.youtube.com/live2", Some("k\n"));
        assert_eq!(yt.url(), "rtmp://a.rtmp.youtube.com/live2/k");
        assert_eq!(yt.policy(), RetryPolicy::Cdn);
        assert_eq!(stored("custom", "rtmp://h/a/s", None).policy(), RetryPolicy::Own);
    }
}
