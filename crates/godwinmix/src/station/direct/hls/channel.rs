//! A channel's watch link: its `hls` destination, packaged by the same HLS
//! packager as a show's `hls://` output and served from the station's port.
//!
//! ```text
//!   listener (ingest): channel stream ──► relay "<app>/<stream>" ──┐
//!                                                                  ▼
//!   packager:  one thread per link ──► cmafmux, ring  (the show outputs' code)
//!                                                                  │
//!   station:   /hls/channel/<channel>/<destination>/index.m3u8?key=...
//!              key checked here, then forwarded as /hls/<destination>/master.m3u8
//!              with the show header `channel:<channel>` ◄──────────┘
//! ```
//!
//! The packager knows a link by the pair (`channel:<id>`, destination), the
//! pair a show's output is known by, so its rings, its viewer count and its
//! report need nothing new. The viewer key is made from that pair, so the
//! link stays the same across restarts. Only the paths a player sees are a
//! channel's own; the playlists' URIs are relative and resolve under them.

use super::wants::Want;
use super::{spec, Packagers};
use crate::channels::{WatchLinks, WatchWant};
use crate::station::packager::wire::{Source, CHANNEL};
use crate::station::state::Station;
use godwinmix_protocol::destination::{DestinationLive, Playback};
use std::collections::BTreeMap;
use std::sync::{Arc, Weak};

/// The key a watch link is kept under, beside the shows' outputs.
pub fn key(channel: &str, destination: &str) -> (String, String) {
    (format!("{CHANNEL}{channel}"), destination.to_string())
}

/// Every watch link that should run, with what it reads.
pub fn wants(st: &Station) -> BTreeMap<(String, String), Want> {
    let Some(channels) = st.channels.get() else { return BTreeMap::new() };
    channels.watch_wants().into_iter().map(|w| (key(&w.channel, &w.destination), want(&w))).collect()
}

fn want(w: &WatchWant) -> Want {
    let source = w.source.as_ref().and_then(|(relay, path)| Some(Source { relay: relay.parse().ok()?, path: path.clone() }));
    let why_not = source.is_none().then(|| format!("waiting for a stream on {} to go live", w.channel));
    Want { spec: spec::read(&w.server), source, why_not }
}

impl Packagers {
    /// One watch link's state, with the links a player opens.
    pub fn channel_view(&self, channel: &str, destination: &str) -> Option<(DestinationLive, Playback)> {
        let (show, output) = key(channel, destination);
        let (live, playback) = self.view(&show, &output)?;
        let key = playback.master_url_path.rsplit("key=").next().unwrap_or_default().to_string();
        let base = format!("/hls/channel/{channel}/{destination}");
        let links = Playback {
            master_url_path: format!("{base}/index.m3u8?key={key}"),
            dash_url_path: format!("{base}/manifest.mpd?key={key}"),
            viewers: playback.viewers,
        };
        Some((live, links))
    }
}

/// The station's answer to the channels about their watch links.
pub struct Links {
    station: Weak<Station>,
    runtime: tokio::runtime::Handle,
}

impl Links {
    /// Give the channels the station's watch links. Called once the
    /// channels are open, on the station's runtime.
    pub fn attach(st: &Arc<Station>) {
        let (Some(channels), Ok(runtime)) = (st.channels.get(), tokio::runtime::Handle::try_current()) else { return };
        channels.use_watch_links(Arc::new(Links { station: Arc::downgrade(st), runtime }));
    }
}

impl WatchLinks for Links {
    fn changed(&self) {
        let st = self.station.clone();
        // Off the channels' thread: apply reads the channels again.
        self.runtime.spawn_blocking(move || {
            if let Some(st) = st.upgrade() {
                st.direct.hls.apply(&st);
            }
        });
    }

    fn view(&self, channel: &str, destination: &str) -> Option<(DestinationLive, Playback)> {
        self.station.upgrade()?.direct.hls.channel_view(channel, destination)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_link_waits_for_its_stream_and_then_reads_it_off_the_relay() {
        let mut w = WatchWant { channel: "sunday".into(), destination: "watch-link".into(), server: String::new(), source: None };
        let idle = want(&w);
        assert!(idle.source.is_none());
        assert_eq!(idle.why_not.as_deref(), Some("waiting for a stream on sunday to go live"));
        w.source = Some(("127.0.0.1:1935".into(), "sunday/main".into()));
        let live = want(&w);
        assert_eq!(live.source.map(|s| s.path), Some("sunday/main".to_string()));
        assert_eq!(key("sunday", "watch-link"), ("channel:sunday".to_string(), "watch-link".to_string()));
    }
}
