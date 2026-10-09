//! The two destinations that never leave this machine: `file`, a recording
//! of the stream as it arrives, and `hls`, a watch link served from the
//! control port.
//!
//! A recording is one more row in the listener's table, with a `file://`
//! address the restreamer already writes MPEG-TS to, so nothing is decoded.
//! The address names the file `<channel>-{stream}-{time}.ts`; the listener
//! fills in the stream it records and the local time the file opened, which
//! makes a new file every time the stream goes live.
//!
//! A watch link is not in the listener's table at all. The station's HLS
//! packager reads the stream off the listener's relay and packages it, the
//! same process and the same code a show without compositing uses for its
//! `hls://` output. The station learns which links to run through
//! [`WatchLinks`], and answers `/hls/channel/<channel>/<destination>/...`.
//! A core with no station has no packager, and says so on the link.

use std::path::PathBuf;
use std::sync::Arc;

use godwinmix_protocol::destination::{DestinationLive, DestinationState, Playback};

use super::store::DestinationRecord;
use super::Channels;

/// The platform id of a recording.
pub const FILE: &str = "file";
/// The platform id of a watch link.
pub const HLS: &str = "hls";

/// The station's side of the watch links.
pub trait WatchLinks: Send + Sync {
    /// A destination, or a stream one reads, may have moved. Must not wait:
    /// it is called from the channels' own threads.
    fn changed(&self);
    /// What one watch link is doing, and where a player opens it.
    fn view(&self, channel: &str, destination: &str) -> Option<(DestinationLive, Playback)>;
}

/// One watch link that should run, and what it reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchWant {
    pub channel: String,
    pub destination: String,
    /// `hls://` and its params, or empty for the defaults.
    pub server: String,
    /// The listener's relay and `<app>/<stream>` on it, once a stream it
    /// reads is live.
    pub source: Option<(String, String)>,
}

/// The folder a recording goes to: the one given, or the recordings folder.
pub fn record_folder(server: &str) -> Option<PathBuf> {
    let given = server.trim();
    if given.is_empty() {
        return godwinmix_core::plugin::outputs::record::default_folder();
    }
    let path = given.strip_prefix("file://").unwrap_or(given);
    let b = path.as_bytes();
    let path = if b.len() > 2 && b[0] == b'/' && b[1].is_ascii_alphabetic() && b[2] == b':' { &path[1..] } else { path };
    Some(PathBuf::from(path))
}

/// The address the listener writes a channel's recordings to.
pub fn record_url(channel: &str, server: &str) -> Option<String> {
    let folder = record_folder(server)?.display().to_string().replace('\\', "/");
    let folder = folder.trim_end_matches('/');
    let lead = if folder.starts_with('/') { "" } else { "/" };
    Some(format!("file://{lead}{folder}/{channel}-{{stream}}-{{time}}.ts"))
}

/// What a client is shown for where a recording goes: the folder.
pub fn record_shown(server: &str) -> String {
    record_folder(server).map(|f| f.display().to_string()).unwrap_or_default()
}

impl Channels {
    /// The station's watch links. Set once, when the station opens.
    pub fn use_watch_links(&self, links: Arc<dyn WatchLinks>) {
        let _ = self.watch.set(links);
    }

    /// Tell the station something a watch link reads moved.
    pub(super) fn watch_moved(&self) {
        if let Some(w) = self.watch.get() {
            w.changed();
        }
    }

    /// Every watch link that is switched on, on a channel that is on.
    pub fn watch_wants(&self) -> Vec<WatchWant> {
        let records = self.records.lock().clone();
        let live = self.live.lock().clone();
        let mut out = Vec::new();
        for r in records.iter().filter(|r| r.enabled) {
            for d in r.destinations.iter().filter(|d| d.enabled && d.platform == HLS) {
                let here = |l: &&super::Live| l.channel == r.id && l.state == "live" && !l.relay.is_empty();
                let stream = match d.stream.as_str() {
                    "*" => live.iter().filter(here).min_by_key(|l| std::cmp::Reverse(l.since_ms)),
                    name => live.iter().filter(here).find(|l| l.name == name),
                };
                let server = self.stored(r).into_iter().find(|s| s.id == d.id).map(|s| s.server).unwrap_or_default();
                out.push(WatchWant {
                    channel: r.id.clone(),
                    destination: d.id.clone(),
                    server,
                    source: stream.map(|l| (l.relay.clone(), format!("{}/{}", l.app, l.name))),
                });
            }
        }
        out
    }

    /// A watch link as a client sees it: what the station says, or why
    /// there is nothing to say.
    pub(super) fn watch_view(&self, channel: &str, d: &DestinationRecord) -> (DestinationLive, Option<Playback>) {
        if !d.enabled {
            return (DestinationLive::default(), None);
        }
        match self.watch.get().and_then(|w| w.view(channel, &d.id)) {
            Some((live, playback)) => (live, Some(playback)),
            None if self.watch.get().is_none() => {
                let error = "a watch link is packaged by the station, and this mixer runs without one. \
                             Start godwinmix without --show to serve it."
                    .to_string();
                (DestinationLive { state: DestinationState::Failed, error: Some(error), ..Default::default() }, None)
            }
            None => {
                let error = Some("handing the link to the HLS packager".to_string());
                (DestinationLive { state: DestinationState::Waiting, error, ..Default::default() }, None)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recording_is_named_for_its_channel_and_filled_in_by_the_listener() {
        let url = record_url("sunday-service", "D:/Recordings").unwrap();
        assert_eq!(url, "file:///D:/Recordings/sunday-service-{stream}-{time}.ts");
        let url = record_url("sunday-service", "file:///srv/rec/").unwrap();
        assert_eq!(url, "file:///srv/rec/sunday-service-{stream}-{time}.ts");
    }

    #[test]
    fn no_folder_is_the_recordings_folder() {
        let folder = record_folder("").expect("a home folder");
        assert!(folder.ends_with(std::path::Path::new("Videos").join("GodwinMix")), "{}", folder.display());
        assert_eq!(record_folder("file:///C:/rec"), Some(PathBuf::from("C:/rec")));
    }
}
