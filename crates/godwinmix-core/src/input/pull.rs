//! Which sources go and fetch their feed, and so are held to a connect
//! deadline, and what a plugin says about itself.
//!
//! A source that pulls from a server and has delivered nothing since it was
//! started is stuck: a server that took the connection and never answered, a
//! relay that swallowed it, a handshake that hung. Nothing posts an error for
//! any of those, so the supervisor restarts it on a deadline
//! (`stall.connect_timeout_secs`). A source that waits to be sent to is a
//! different thing. An SRT or UDP listener, or a plugin waiting for a phone to
//! publish, is working as intended while it waits, and restarting it every
//! few seconds would only close the door on a publisher that was about to
//! knock. So the deadline is for the core's own pulling kinds alone.

use super::InputPipeline;
use crate::plugin::{Capability, Health, Tier};

impl InputPipeline {
    /// Whether this source fetches its feed from a server.
    pub fn pulls_feed(&self) -> bool {
        let m = self.manifest();
        m.tier == Tier::Core && pulls(m.plugin, &self.config.uri)
    }

    /// Whether this source is a plugin that answers `health` for itself. The
    /// core's own kinds declare it too, but answer from state the supervisor
    /// already reads.
    pub fn answers_health(&self) -> bool {
        self.manifest().tier != Tier::Core && self.capabilities().has(Capability::Health)
    }

    /// The kind's own view of itself, unless the kind is busy in a restart
    /// or a call. Blocks for as long as the kind takes to answer, so it is
    /// asked on a thread of its own (`mixer::supervise`).
    pub fn try_plugin_health(&self) -> Option<Health> {
        self.kind.try_lock().map(|k| k.health())
    }
}

/// Whether `plugin` pulls the feed at `uri`. Pure, for the tests.
pub fn pulls(plugin: &str, uri: &str) -> bool {
    let uri = uri.trim().to_ascii_lowercase();
    match plugin {
        "rtmp" => true,
        "hls" => !listens(&uri),
        _ => false,
    }
}

/// An address this side receives on rather than fetches from.
fn listens(uri: &str) -> bool {
    if ["udp://", "rtp://", "rist://"].iter().any(|p| uri.starts_with(p)) {
        return true;
    }
    let Some(rest) = uri.strip_prefix("srt://") else { return false };
    // `srtsrc` listens when told to, and when it is given no host to call.
    let host = rest.split([':', '/', '?']).next().unwrap_or("");
    uri.contains("mode=listener") || host.is_empty() || host == "0.0.0.0" || host == "@"
}

#[cfg(test)]
mod tests {
    use super::pulls;

    #[test]
    fn a_fetched_feed_pulls_and_a_listener_does_not() {
        assert!(pulls("rtmp", "rtmp://relay.example/live/cam"));
        assert!(pulls("rtmp", "rtmps://relay.example/live/cam"));
        assert!(pulls("hls", "https://cdn.example/live/index.m3u8"));
        assert!(pulls("hls", "rtsp://10.0.0.5/stream1"));
        assert!(pulls("hls", "srt://10.0.0.5:9000"));
        assert!(!pulls("hls", "srt://:9000"));
        assert!(!pulls("hls", "srt://0.0.0.0:9000"));
        assert!(!pulls("hls", "srt://10.0.0.5:9000?mode=listener"));
        assert!(!pulls("hls", "udp://0.0.0.0:5000"));
        assert!(!pulls("hls", "rist://@:5004"));
        assert!(!pulls("file", "https://cdn.example/clip.mp4"));
        assert!(!pulls("ingest", "rtmp://0.0.0.0:1935/live"));
    }
}
