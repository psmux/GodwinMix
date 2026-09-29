//! The platforms a channel's stream can be sent on to, and how hard to retry
//! each one. Data, so the core, the restreamer in the ingest plugin and the
//! tests all read the same table.
//!
//! `ui/client/destinations.js` keeps its own copy of the servers because the
//! page draws the form before it has asked the core anything, and a test in
//! this module reads that file and fails when the two disagree.

use serde::{Deserialize, Serialize};

/// Whether a platform asks for a stream key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyRule {
    /// Nothing is sent without one: YouTube, Facebook, Twitch.
    Required,
    /// Asked for, but a whole address pasted into the server box is enough.
    Optional,
    /// The address is everything, as with SRT.
    None,
}

/// How a platform is reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Carriage {
    /// RTMP or RTMPS, chosen by the scheme of the server.
    Rtmp,
    /// MPEG-TS over SRT.
    Srt,
}

/// One tile on the destination form.
#[derive(Debug, Clone, Copy)]
pub struct Platform {
    pub id: &'static str,
    pub title: &'static str,
    /// The ingest with no trailing slash; the key is joined onto it. Empty for
    /// a platform whose address the person types.
    pub server: &'static str,
    pub key: KeyRule,
    pub carriage: Carriage,
    pub policy: RetryPolicy,
    /// What an address on this platform is recognised by.
    pub hosts: &'static [&'static str],
}

/// Every platform, in the order the form shows them.
pub const PLATFORMS: &[Platform] = &[
    Platform {
        id: "youtube",
        title: "YouTube",
        server: "rtmp://a.rtmp.youtube.com/live2",
        key: KeyRule::Required,
        carriage: Carriage::Rtmp,
        policy: RetryPolicy::Cdn,
        hosts: &["rtmp.youtube.com"],
    },
    Platform {
        id: "facebook",
        title: "Facebook",
        server: "rtmps://live-api-s.facebook.com:443/rtmp",
        key: KeyRule::Required,
        carriage: Carriage::Rtmp,
        policy: RetryPolicy::Cdn,
        hosts: &["live-api-s.facebook.com"],
    },
    Platform {
        id: "twitch",
        title: "Twitch",
        server: "rtmp://live.twitch.tv/app",
        key: KeyRule::Required,
        carriage: Carriage::Rtmp,
        policy: RetryPolicy::Cdn,
        hosts: &["live.twitch.tv", "contribute.live-video.net"],
    },
    Platform {
        id: "custom",
        title: "Custom RTMP",
        server: "",
        key: KeyRule::Optional,
        carriage: Carriage::Rtmp,
        policy: RetryPolicy::Own,
        hosts: &[],
    },
    Platform {
        id: "srt",
        title: "SRT",
        server: "",
        key: KeyRule::None,
        carriage: Carriage::Srt,
        policy: RetryPolicy::Own,
        hosts: &[],
    },
];

/// The platform with this id.
pub fn platform(id: &str) -> Option<&'static Platform> {
    PLATFORMS.iter().find(|p| p.id == id)
}

/// Every platform id, for an error that has to say which would have worked.
pub fn platform_ids() -> Vec<String> {
    PLATFORMS.iter().map(|p| p.id.to_string()).collect()
}

/// Server plus key, as every RTMP ingest on the table wants them. Both are
/// trimmed, because a key pasted out of a dashboard often carries a newline.
pub fn join_key(server: &str, key: &str) -> String {
    let base = server.trim().trim_end_matches('/');
    let key = key.trim();
    if key.is_empty() {
        base.to_string()
    } else {
        format!("{base}/{key}")
    }
}

/// The two reconnect behaviours, named as outputs name them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RetryPolicy {
    /// A server you run, which takes you back at once.
    Own,
    /// A public platform that penalises a client for hammering it.
    Cdn,
}

/// A doubling backoff with a floor and a ceiling.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Retry {
    pub initial_delay_ms: u64,
    pub max_delay_ms: u64,
    pub multiplier: f64,
}

impl RetryPolicy {
    /// The numbers. The core's `ReconnectConfig::preset` reads these, so an
    /// output and a channel destination on the same platform back off alike.
    pub const fn retry(self) -> Retry {
        match self {
            RetryPolicy::Own => Retry { initial_delay_ms: 100, max_delay_ms: 2_000, multiplier: 1.6 },
            RetryPolicy::Cdn => {
                Retry { initial_delay_ms: 1_000, max_delay_ms: 30_000, multiplier: 2.0 }
            }
        }
    }
}

impl Retry {
    /// The wait before attempt number `attempt`, counting from zero.
    pub fn delay_for(&self, attempt: u32) -> std::time::Duration {
        let scaled = self.initial_delay_ms as f64 * self.multiplier.powi(attempt.min(16) as i32);
        std::time::Duration::from_millis(scaled.min(self.max_delay_ms as f64) as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The page keeps its own copy of the servers. This is what stops the two
    /// drifting: a server changed on one side and not the other fails here.
    #[test]
    fn the_page_offers_the_same_servers_as_this_table() {
        let js = include_str!("../../../../ui/client/destinations.js");
        // The output kinds above the table have ids of their own.
        let js = &js[js.find("export const PLATFORMS").expect("the table is there")..];
        for p in PLATFORMS {
            let id = format!("id: \"{}\"", p.id);
            let at = js.find(&id).unwrap_or_else(|| panic!("destinations.js has no {}", p.id));
            let rest = &js[at..];
            let server = format!("server: \"{}\"", p.server);
            let next = rest[1..].find("id: \"").map(|n| n + 1).unwrap_or(rest.len());
            assert!(rest[..next].contains(&server), "{} differs: want {server}", p.id);
        }
    }

    #[test]
    fn a_key_is_joined_on_with_one_slash_and_trimmed() {
        assert_eq!(join_key("rtmp://a/live2/", " abc\n"), "rtmp://a/live2/abc");
        assert_eq!(join_key("rtmp://a/live2", ""), "rtmp://a/live2");
    }

    #[test]
    fn the_presets_are_the_numbers_outputs_have_always_used() {
        let own = RetryPolicy::Own.retry();
        assert_eq!(own.delay_for(0).as_millis(), 100);
        assert_eq!(own.delay_for(20).as_millis(), 2_000);
        let cdn = RetryPolicy::Cdn.retry();
        assert_eq!(cdn.delay_for(0).as_millis(), 1_000);
        assert_eq!(cdn.delay_for(3).as_millis(), 8_000);
        assert_eq!(cdn.delay_for(10).as_millis(), 30_000);
    }
}
